# Sync & Network Roadmap

From single-device sync to decentralized music network.

## What exists today

**Storage model:** `DbStorageProfile` with `StorageLocation::{Local, Cloud}`. Storage profiles are file storage locations -- they hold release files and nothing else. Files use hash-based paths (`storage/ab/cd/{file_id}`). Images at `images/ab/cd/{id}`. Cloud files are chunked-encrypted with per-file random nonces. The nonce is embedded as the first 24 bytes of each encrypted blob (prepended by `encrypt_chunked`). A copy is also cached in `release_files.encryption_nonce` for efficient range-request decryption -- this avoids a separate fetch of the blob prefix when only a middle chunk is needed. The nonce and the encryption key are independent: changing the key (Phase 3) does not affect nonce handling. Local profiles are plaintext.

**Sync model:** `MetadataReplicator` pushes a full `VACUUM INTO` DB snapshot plus all images to every non-home profile on every mutation. This is being replaced by the sync bucket + changeset model described below. MetadataReplicator will be removed entirely.

**Identity model:** Libraries have a UUID (`library_id`) and a symmetric encryption key (32-byte XChaCha20-Poly1305, stored in OS keyring via `KeyService`). There are no user identities, no keypairs, no signatures. The encryption key is per-library, not per-user or per-release.

**DB schema:** Single SQLite file (`library.db`), single migration (`001_initial.sql`). Tables: `artists`, `albums`, `album_discogs`, `album_musicbrainz`, `album_artists`, `releases`, `tracks`, `track_artists`, `release_files`, `audio_formats`, `library_images`, `storage_profiles`, `release_storage`, `torrents`, `torrent_piece_mappings`, `imports`. All IDs are UUIDv4 strings. Timestamps are RFC 3339 strings. Four tables have `updated_at` columns today: `artists`, `albums`, `releases`, `storage_profiles`.

**Writer model:** Desktop is the single writer. `bae-server` opens the DB read-only (`Database::open_read_only`). No write lock mechanism exists.

**Torrent:** Full BitTorrent integration via libtorrent (C++ FFI). Piece-to-file mapping, seeding, NAT traversal (UPnP/NAT-PMP). Already has `info_hash` in the DB.

**Dependencies of note:** `aws-sdk-s3` for cloud storage, `keyring-core` + `apple-native-keyring-store` for secrets, `sqlx 0.8` (SQLite, uses bundled `libsqlite3-sys`), `sha2`/`hmac`/`hex` for hashing, `axum` for HTTP, hand-written libsodium FFI (`sodium_ffi.rs`, ~40 lines) for encryption, `ffmpeg-next` for audio.

---

## The key design decision: SQLite Session Extension

The previous iterations of this roadmap proposed an op log system where an `OpRecorder` wrapper intercepts every write method (~63 methods across `Database` and `LibraryManager`), records per-field timestamps in a `field_timestamps` table (~400K rows for a medium library), and pushes serialized JSON ops to S3.

After two rounds of review and further deliberation, we replaced that entire approach with the **SQLite Session Extension**. Here is why:

### Approaches considered and rejected

1. **Op log with OpRecorder wrapping ~63 write methods.** Mechanical but enormous surface area. Every new write method must be wrapped. Every existing method must be audited for completeness. The `field_timestamps` table adds ~40MB for a medium library and requires a backfill migration.

2. **SQLite triggers recording per-column changes.** Same maintenance burden moved to SQL: triggers must enumerate every column of every synced table. Adding a column to any synced table requires updating the trigger.

3. **Automerge/Loro CRDT document model.** Holds the full document in memory. Doesn't scale to arbitrary entity counts (a library can have 50K+ tracks).

4. **cr-sqlite (CRDTs for SQLite).** The project is effectively stalled (last substantive commit June 2024). Too risky as a dependency.

### What we chose and why

The SQLite session extension is a built-in SQLite feature (compiled in, not a loadable extension). It tracks all changes (INSERT/UPDATE/DELETE) automatically at the C level by diffing the database state before and after. No triggers, no method wrapping, no column enumeration.

The app writes normally. SQLite records what changed. We grab the binary changeset, encrypt it, push it to S3. Other devices pull, decrypt, and apply with a conflict handler. That's it.

**What this eliminates:**
- ~~`field_timestamps` table (400K rows)~~
- ~~`OpRecorder` wrapping ~63 methods~~
- ~~Op log local table~~
- ~~Custom JSON op format~~
- ~~Custom merge algorithm~~
- ~~Tombstone management~~ (DELETEs are in the changeset natively)
- ~~Batch atomicity logic~~ (one changeset = one application-level transaction)
- ~~Per-field HLC tracking~~ (row-level `_updated_at` is sufficient)
- ~~Backfill migration for field_timestamps~~

**What remains:**
- `_updated_at TEXT` column on the ~11 synced tables (for conflict resolution)
- Session management (~50 lines: create, attach tables, grab changeset, end)
- Push changeset to S3 (~30 lines)
- Pull and apply with conflict handler (~40 lines)
- Periodic full snapshots for bootstrapping new devices
- Image sync (push/pull image files alongside changesets that reference them)
- HLC or equivalent timestamp scheme for `_updated_at`

### Technical feasibility

**sqlx access to the raw handle:** sqlx 0.8 exposes the raw `sqlite3*` pointer via `LockedSqliteHandle::as_raw_handle() -> NonNull<sqlite3>`. We can call session extension functions through FFI on this handle.

**Enabling the session extension:** sqlx uses bundled `libsqlite3-sys`. The session extension requires the `session` feature flag on `libsqlite3-sys`, which sets `SQLITE_ENABLE_SESSION` and `SQLITE_ENABLE_PREUPDATE_HOOK` at compile time, and requires `buildtime_bindgen`. We need to verify this compiles cleanly with sqlx's bundled sqlite. If the feature flag approach doesn't work with sqlx's bundled build, we can set `LIBSQLITE3_FLAGS="-DSQLITE_ENABLE_SESSION -DSQLITE_ENABLE_PREUPDATE_HOOK"` as an environment variable during build. The primary path is adding `libsqlite3-sys` as a direct dependency in `bae-core/Cargo.toml` with the `session` feature -- Cargo unifies features across the dependency graph.

**FFI surface:** We need bindings for ~6 functions: `sqlite3session_create`, `sqlite3session_attach`, `sqlite3session_changeset`, `sqlite3session_delete`, `sqlite3changeset_apply`, and `sqlite3_free`. These are available in `libsqlite3-sys` when the session feature is enabled. Small FFI wrapper in bae-core (~80 lines), similar in spirit to the existing `sodium_ffi.rs`.

---

## Architecture: sync bucket vs. storage profiles

Sync and file storage are separate concerns.

**Sync bucket:** One S3 bucket per library (optional). The library's collaborative hub. Contains `snapshot.db.enc`, `changes/`, `heads/`, and `images/`. Configured in `config.yaml`, not in the `storage_profiles` DB table. Sync bucket credentials stored in the keyring under `sync_s3_access_key` / `sync_s3_secret_key`.

**Storage profiles:** Places where release files sit. Just files -- no DB, no images, no manifest. The `storage_profiles` and `release_storage` DB tables remain. The library home is a storage profile. Cloud profiles are S3 buckets. External drives are local directories. The sync bucket can optionally also serve as file storage (has a `storage_profiles` row for that purpose).

This separation means:
- Adding a cloud storage profile is just adding a place to put files. No metadata replication triggers.
- Enabling sync is configuring the sync bucket. Independent of file storage.
- `MetadataReplicator` is removed entirely. No full-snapshot sync to N profiles after every mutation.
- External drives no longer need DB/images -- just files.
- bae-server syncs from the single sync bucket, not from a per-profile metadata replica.

---

## Phase 0: Foundation work (no user-visible change)

*Goal: lay internal groundwork that sync depends on, without changing any user-facing behavior.*

### 0a. `_updated_at` column on synced tables

The session extension's conflict handler needs a way to determine which side "wins" when the same row is modified by two devices. We use row-level LWW: the row with the later `_updated_at` wins.

Today, four tables already have `updated_at`: `artists`, `albums`, `releases`, `storage_profiles`. The remaining synced tables need an `_updated_at` column added. (We use `_updated_at` with a leading underscore to distinguish the sync timestamp from the existing `updated_at` on tables that have it. On tables that lack `updated_at`, the `_updated_at` column serves both purposes.)

**Tables that need `_updated_at` added (migration 002):**

| Table | Currently has `updated_at`? | Action |
|-------|---------------------------|--------|
| artists | Yes | Rename to `_updated_at` |
| albums | Yes | Rename to `_updated_at` |
| releases | Yes | Rename to `_updated_at` |
| album_discogs | No | Add `_updated_at` |
| album_musicbrainz | No | Add `_updated_at` |
| album_artists | No | Add `_updated_at` |
| tracks | No | Add `_updated_at` |
| track_artists | No | Add `_updated_at` |
| release_files | No | Add `_updated_at` |
| audio_formats | No | Add `_updated_at` |
| library_images | No | Add `_updated_at` |

**Tables NOT synced** (device-specific, no `_updated_at` needed): `storage_profiles`, `release_storage`, `torrents`, `torrent_piece_mappings`, `imports`. `storage_profiles` keeps its existing `updated_at` column as-is (no rename to `_updated_at`, since it is not synced).

**Migration:** `002_sync_timestamps.sql`. For tables that already have `updated_at`, rename it. For others, add the column with a default of the row's `created_at` value (or `datetime('now')` for rows without `created_at`). Also update all write methods in `Database` that touch these tables to set `_updated_at` on every write.

**Backfill:** For existing rows on tables that lacked `updated_at`, set `_updated_at = created_at`. This is correct -- before multi-device, the creation time is the best approximation.

**bae-core changes:**
- Migration `002_sync_timestamps.sql`.
- Update `Database` write methods to set `_updated_at = now()` on INSERT and UPDATE for synced tables. This is a targeted change to the SQL strings in existing methods, not a new wrapper layer.
- Update `DbAlbum`, `DbArtist`, `DbRelease`, etc. model structs to use the renamed field.

**Impact on existing code:** The rename from `updated_at` to `_updated_at` on artists/albums/releases affects code that reads these columns. Grep for `updated_at` usage and update. This is mechanical.

### 0b. Device identity

Add `device_id` to `config.yaml` / `ConfigYaml`:

```yaml
device_id: "dev-abc123..."
```

Generated once on first launch (or migration). If absent, generate a random UUID and save. Used as the namespace key in S3: `changes/{device_id}/`.

**Orphaned device_ids:** If a user reinstalls bae or deletes `config.yaml`, they get a new `device_id`. The old device's `heads/` and `changes/` entries in the bucket become orphaned. This is harmless: stale data that never advances. Cleaned up when a snapshot supersedes those changesets.

**bae-core changes:**
- Add `device_id: Option<String>` to `ConfigYaml`.
- In startup: if `device_id` is None, generate one and save.

### 0c. Verify session extension availability

Before building the sync service, verify that the session extension compiles and works with our sqlx + bundled sqlite setup.

**Proof of concept:**
1. Enable `SQLITE_ENABLE_SESSION` in the build (via `libsqlite3-sys` feature or `LIBSQLITE3_FLAGS`).
2. Write a test that: creates a session on a connection, attaches a table, does an INSERT, grabs the changeset, verifies it's non-empty.
3. Write a test that: applies a changeset to a second database, verifies the row appears.
4. Write a test that: tests the conflict handler with a DATA conflict (same row modified on both sides), verifies REPLACE behavior.

This is a spike, not production code. But it validates the entire FFI path before we build on it.

**bae-core changes:**
- Add session extension FFI wrapper (`session_ffi.rs` or extend `sodium_ffi.rs` pattern).
- Integration test proving the round-trip works.

### 0d. HLC or simple timestamp scheme

Row-level LWW needs a timestamp that's robust against clock skew. Two options:

1. **HLC (Hybrid Logical Clock):** Monotonically increasing, handles clock skew. ~50 lines. Stored as `"{millis}-{counter}-{device_id}"` in `_updated_at`. Lexicographically sortable.

2. **Wall clock with skew guard:** Use RFC 3339 timestamps (as today). On pull, if an incoming `_updated_at` is more than 24 hours in the future, log a warning but accept it. Simpler, works for the common case.

**Decision: HLC.** The implementation cost is trivial (~50 lines), and it prevents the class of bugs where a device with a wrong clock silently wins all conflicts. Since `_updated_at` is the sole arbiter of conflict resolution, getting this right matters.

**Format:** `"{millis}-{counter}-{device_id}"`. Lexicographic comparison works because millis (zero-padded to 13 digits) dominates. The zero-padding MUST be enforced -- without it, lexicographic comparison breaks for different digit counts.

**Clock skew guard:** On pull, if an incoming HLC's wall-clock component is more than 24 hours ahead of local wall time, accept but don't advance the local HLC past local wall time. Prevents a runaway clock from poisoning the HLC system-wide.

**bae-core changes:**
- Add `HybridLogicalClock` struct. ~50 lines.
- Every write to a synced table sets `_updated_at` via the HLC.

### 0e. Database connection architecture

The SQLite session extension attaches to a single database connection and only captures changes made through that connection. The current `Database` struct uses `SqlitePool`, which dispatches writes to whichever connection is available. If the session is on connection A but a write goes through connection B, that write is not captured.

**Solution:** Separate the pool into a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This matches SQLite's single-writer-multiple-reader architecture.

**bae-core changes:**
- Refactor `Database` to hold both a write connection and a read pool.
- Route all 33 write methods through the dedicated write connection.
- Read methods continue using the pool.
- The session is created on the write connection.

### Phase 0 summary

| Component | Changes |
|-----------|---------|
| DB schema | `_updated_at` column on 11 synced tables (migration 002) |
| `config.yaml` | New `device_id` field |
| `Config` | Generate/load device_id |
| bae-core | `HybridLogicalClock` struct (~50 lines) |
| bae-core | Session extension FFI wrapper + integration test |
| bae-core | `Database` refactored: dedicated write connection + read pool |
| `Database` | Write methods set `_updated_at` on synced tables |
| Model structs | `updated_at` renamed to `_updated_at` on artists/albums/releases |
| bae-server | No changes |
| User experience | None. Everything works exactly as before. |

**What's NOT in Phase 0:** No `OpRecorder`. No `field_timestamps` table. No interception layer. The `Database` write methods are updated to set `_updated_at` -- a targeted change to SQL strings, not a new architectural layer.

---

## Phase 1: Session-based sync (replaces MetadataReplicator)

*Goal: replace the full DB snapshot sync with incremental changesets pushed to a single sync bucket. Solo users (Tier 2) get faster, cheaper sync. Multi-device solo users get conflict-free merge. This is the core infrastructure all later phases build on.*

### The sync bucket

The sync bucket is a library-level concept -- one bucket per library. It's configured in `config.yaml`, not in the `storage_profiles` DB table. It stores:

- `snapshot.db.enc` -- full DB for bootstrapping new devices
- `changes/{device_id}/{seq}.enc` -- binary changesets with metadata envelope
- `heads/{device_id}.json.enc` -- per-device sequence numbers for cheap polling
- `images/ab/cd/{id}` -- all library images (encrypted)

Optionally, the sync bucket can also hold release files under `storage/ab/cd/{file_id}` (see `02-storage-profiles.md`). This makes the simplest setup one bucket for everything.

### The protocol

1. On app start, create a session on the dedicated write connection and attach all synced tables.
2. The app writes normally -- import, edit, delete. The session records everything.
3. When it's time to sync (on mutation with debounce, or periodic timer):
   a. Grab the changeset from the session (compact binary blob).
   b. End the session.
   c. Push the changeset to the sync bucket.
   d. Pull incoming changesets from other devices (NO session active -- critical).
   e. Apply incoming changesets with conflict handler.
   f. Start a new session for the next round.

**Key ordering rule:** Never apply someone else's changeset while your session is recording. If you do, your outgoing changeset gets polluted with their changes, and when you push it, those changes bounce back as duplicates. The protocol is always: end session, then apply incoming, then start new session.

### 1a. Session management

A thin wrapper around the session extension FFI.

```rust
struct SyncSession {
    session: *mut sqlite3_session,
    // Tables attached to this session
}

impl SyncSession {
    /// Create a session on the given connection, attach synced tables.
    fn new(conn: &mut LockedSqliteHandle) -> Result<Self, SyncError>;

    /// Grab the changeset (binary blob of all changes since session start).
    /// Returns None if no changes were made.
    fn changeset(&self) -> Result<Option<Vec<u8>>, SyncError>;

    /// End the session and free resources.
    fn end(self);
}
```

**Which tables to attach:**

| Table | Attach? | Notes |
|-------|---------|-------|
| artists | Yes | User edits names |
| albums | Yes | User edits titles, years |
| album_discogs | Yes | User matches to Discogs |
| album_musicbrainz | Yes | User matches to MusicBrainz |
| album_artists | Yes | User changes artist linkage |
| releases | Yes | User edits release metadata |
| tracks | Yes | User edits track titles |
| track_artists | Yes | User changes track artists |
| release_files | Yes | File inventory (see note below) |
| audio_formats | Yes | Expensive to recompute |
| library_images | Yes | Cover changes |

**NOT attached** (device-specific): `storage_profiles`, `release_storage`, `torrents`, `torrent_piece_mappings`, `imports`.

**release_files table note:** The `release_files` table has two device-specific columns: `source_path` (local file location) and `encryption_nonce` (profile-specific cache). These columns should be excluded from conflict resolution but included in the changeset for new-row propagation. The conflict handler can handle this: on DATA conflict for `release_files`, always prefer the local `source_path` and `encryption_nonce` values (since they're meaningful only on the device that has the file). In practice, this means: if a `release_files` row already exists locally, OMIT the incoming change for those two columns. If it's a new row (NOTFOUND), accept everything (the receiving device will populate `source_path` when it transfers the files).

**audio_formats FK dependency:** `audio_formats` has FKs to both `tracks` and `release_files`. If an `audio_formats` INSERT arrives before the referenced `tracks` or `release_files` row, the FK constraint fails. The changeset_apply conflict handler receives a CONSTRAINT conflict -- we OMIT it and retry after all other changesets are processed. Cross-changeset FK dependencies resolve naturally because parent rows are created in earlier changesets (earlier seq numbers) than children.

### 1b. Push: changeset to sync bucket

After a sync cycle:

1. Grab changeset from session.
2. If changeset is empty (no changes), skip push.
3. Wrap in a metadata envelope:
   ```json
   {
     "device_id": "dev-abc123",
     "seq": 42,
     "schema_version": 2,
     "message": "Imported Kind of Blue",
     "timestamp": "2026-02-10T14:30:00Z",
     "changeset_size": 4096
   }
   ```
   The envelope is JSON. The changeset is binary. Concatenate: `envelope_json + '\0' + changeset_bytes`. Encrypt the combined blob.
4. Upload to `changes/{device_id}/{seq}.enc`.
5. Update `heads/{device_id}.json.enc` with `{ "seq": 42 }`.
6. Increment local seq counter (stored in a local `sync_state` table or config).

**Trigger:** After `LibraryEvent::AlbumsChanged` (same as today) with debounce, plus periodic timer (every 30s if the session has changes).

**Semantic messages:** The `message` field in the envelope is informational -- "Imported Kind of Blue", "Deleted 3 releases", "Edited artist name". Derived from the `LibraryEvent` or the session's table-level statistics (X inserts, Y updates, Z deletes). Useful for the sync status UI in Phase 2+ ("Alice imported Kind of Blue 5 minutes ago").

### Image sync

Images are binary files, not DB rows. The session extension tracks `library_images` table changes (metadata), but the actual image bytes live on disk / in the sync bucket.

**Push:** When a changeset contains INSERTs or UPDATEs to `library_images`, upload the corresponding image files to `images/ab/cd/{id}` in the sync bucket before pushing the changeset. If the push crashes between image upload and changeset push, the changeset will be pushed on the next cycle (it's still in the session or re-captured in a new session).

**Pull:** After applying a changeset that contains `library_images` changes, check if the referenced image files exist locally. For each missing image, download from `images/ab/cd/{id}` in the sync bucket. This is O(number of new/changed images in the changeset), not O(total images).

**No image_sync_queue table needed.** The session extension makes image sync simpler than the op log approach:
- On push: scan the changeset for `library_images` changes before uploading. The changeset is a binary blob, but `sqlite3changeset_start` + `sqlite3changeset_next` iterate its contents.
- On pull: same iteration to find image references after applying.

### 1c. Pull: sync bucket changesets to local DB

On app open and periodically:

1. List `heads/` -- one S3 LIST call.
2. For each device_id, compare their seq to our local cursor (`sync_cursors` table).
3. If any device is ahead, fetch their new changesets: `changes/{device_id}/{seq}.enc` for each seq we haven't seen.
4. **End the current session** (grab our outgoing changeset first if any).
5. For each incoming changeset, in order:
   a. Decrypt and extract (split on `'\0'` to separate envelope from changeset bytes).
   b. Apply using `sqlite3changeset_apply()` with our conflict handler.
6. Update `sync_cursors`.
7. Start a new session for the next round.

**Local cursor tracking:**

```sql
CREATE TABLE sync_cursors (
    device_id  TEXT PRIMARY KEY,
    last_seq   INTEGER NOT NULL
);

CREATE TABLE sync_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: local_seq (our next sequence number), last_sync_time, etc.
```

### Conflict handler

The conflict handler is called by `sqlite3changeset_apply()` when it encounters a conflict. It receives the conflict type and can inspect both the local and incoming values.

```rust
fn conflict_handler(
    conflict_type: ConflictType,
    changeset_iter: &ChangesetIter,
) -> ConflictAction {
    match conflict_type {
        // Same row modified by both local and incoming.
        // Compare _updated_at: if incoming is newer, REPLACE (accept incoming).
        // The REPLACE operation merges: columns present in the changeset overwrite,
        // columns NOT in the changeset keep their local values.
        ConflictType::Data => {
            let local_updated_at = get_column(changeset_iter, "_updated_at", Conflict);
            let incoming_updated_at = get_column(changeset_iter, "_updated_at", New);
            if incoming_updated_at > local_updated_at {
                ConflictAction::Replace
            } else {
                ConflictAction::Omit
            }
        }

        // Row was deleted locally, incoming changeset has an UPDATE for it.
        // Delete wins. Omit the incoming update.
        ConflictType::NotFound => ConflictAction::Omit,

        // Row already exists for an INSERT.
        // If the incoming row has a newer _updated_at, replace.
        // Otherwise keep local.
        ConflictType::Conflict => {
            let local_updated_at = get_column(changeset_iter, "_updated_at", Conflict);
            let incoming_updated_at = get_column(changeset_iter, "_updated_at", New);
            if incoming_updated_at > local_updated_at {
                ConflictAction::Replace
            } else {
                ConflictAction::Omit
            }
        }

        // FK constraint violation (e.g., audio_formats referencing a tracks row
        // that hasn't been applied yet). Omit for now, retry later.
        ConflictType::Constraint => ConflictAction::Omit,

        _ => ConflictAction::Omit,
    }
}
```

**Row-level LWW is accepted.** If two users edit different columns of the same row simultaneously, the changeset's REPLACE preserves both changes (a changeset only contains columns that actually changed). True conflicts (same column modified by both sides) are resolved by `_updated_at`. This is totally fine for a music library.

**Delete semantics:** Delete wins by default. If a row is deleted locally and an incoming changeset has an UPDATE for it, NOTFOUND conflict leads to OMIT (row stays deleted). If a row is deleted in an incoming changeset and was modified locally, the delete applies (it's a DELETE operation in the changeset, not a conflict). This is acceptable for a music library.

**Constraint retries:** When a CONSTRAINT conflict occurs (FK violation), that changeset's operation is omitted. After all changesets from a sync batch are applied, re-apply any changesets that had CONSTRAINT omissions. The parent rows from earlier changesets should now be present. If a changeset still fails after retry, log an error and skip it -- the next full snapshot will reconcile.

### 1d. Snapshots (checkpoints)

The changeset log grows unboundedly. Periodically, create a full snapshot:

1. `VACUUM INTO` a snapshot (same mechanism as today's `MetadataReplicator`).
2. Encrypt and upload as `snapshot.db.enc` (overwriting the previous one).
3. Update `heads/{device_id}.json.enc` with `{ "seq": N, "snapshot_seq": N }` indicating that the snapshot covers all changesets up to seq N.

A new device starts from `snapshot.db.enc`, then pulls only changesets with seq > snapshot_seq.

**Policy:** Create a snapshot after every N changesets (e.g., 100) or every T hours (e.g., 24). The device that triggers the threshold creates it. Concurrent snapshots are harmless (they overwrite the same key).

**Garbage collection:** After creating a snapshot at seq N, changesets with seq <= N for ALL devices can be deleted after a grace period (30 days). A device offline longer than 30 days re-bootstraps from the snapshot.

Unlike the op log model, there's no complex checkpoint metadata tracking which devices' ops are included. The snapshot IS the database -- it contains everything. Any device can create one.

### 1e. MetadataReplicator removal

MetadataReplicator is removed entirely. It is not reduced to local-only -- it is deleted.

- **Sync bucket:** Handles all metadata sync via changesets + images + periodic snapshots.
- **Storage profiles (local and cloud):** No longer receive any metadata. They hold release files only.
- **External drives:** No longer get DB/images copies. They are just file storage.

The `SyncService` handles push/pull of changesets + images to the single sync bucket. There is no per-profile metadata sync.

### 1f. Concurrent edit detection

After each pull (which already fetches `heads/`), show a status: "Last synced: 2 minutes ago. Device X also synced 5 minutes ago." Derived from the `heads/` data. No pre-push lock check, no additional S3 calls.

The system merges concurrent writes correctly via the conflict handler; the indicator is informational.

### 1g. bae-server in the changeset world

Today `bae-server` downloads `library.db.enc` + `manifest.json.enc` + `images/` from a specific cloud profile. Under the new model, bae-server syncs from the sync bucket instead.

**Phase 1 approach:** bae-server is given sync bucket coordinates + encryption key. On boot:
1. Download `snapshot.db.enc`, decrypt, cache locally.
2. Pull and apply changesets since the snapshot (temporarily open DB read-write for apply, then read-only for serving).
3. Download images from the sync bucket.

**CLI changes:** Replace `--s3-bucket` (which pointed at a specific profile's bucket) with sync bucket coordinates. The old profile-based boot path is removed.

**Optional `--refresh` mode:** Background loop pulling new changesets while running. Deferred to post-Phase-1 for simplicity.

### Phase 1 summary

| Component | Changes |
|-----------|---------|
| DB schema | `sync_cursors`, `sync_state` tables (migration 003) |
| Sync bucket layout | `changes/{device_id}/{seq}.enc`, `heads/`, `snapshot.db.enc`, `images/` |
| bae-core | `SyncSession` (session management), `SyncService` (push/pull/apply) |
| bae-core | Session extension FFI wrapper (~80 lines) |
| bae-core | Conflict handler (~40 lines) |
| Changeset envelope | `schema_version` field; `min_schema_version` marker in sync bucket |
| `MetadataReplicator` | Removed entirely |
| bae-desktop | Sync indicator in UI (last sync time, other devices' activity) |
| bae-server | Syncs from sync bucket (snapshot + changesets + images) |
| New dependencies | None (session extension is built into SQLite) |
| User experience | Sync is faster. Multi-device "just works". No new UI beyond a status indicator. |

### Sync bucket layout (Phase 1)

```
s3://sync-bucket/
  snapshot.db.enc                    # full DB for bootstrapping
  changes/{device_id}/{seq}.enc      # changeset blobs with metadata envelope
  heads/{device_id}.json.enc         # { "seq": N, "snapshot_seq": M }
  images/ab/cd/{id}                  # library images (encrypted)
  storage/ab/cd/{file_id}            # release files (optional, if bucket doubles as file storage)
```

### 1h. Schema migration strategy

Once changeset sync is live, schema changes become a coordination problem. The SQLite session extension identifies columns by **index**, not by name -- a changeset says "column 3 changed," not "column mood changed." This makes certain kinds of schema changes dangerous across devices running different versions.

#### Schema version tracking

Every changeset envelope carries a `schema_version` integer (see envelope format in 1b). This tells receivers what schema produced the changeset.

The sync bucket has a `min_schema_version` marker -- a dedicated file (`min_schema_version.json.enc`) or a field in `heads/`. This is the floor: clients below this version must upgrade before syncing.

#### Two kinds of schema changes

**Additive (non-breaking):** Add columns at the end of a table, or add entirely new tables. These do NOT bump `min_schema_version`.

- Old changesets applied to new schema: work fine. The changeset has fewer columns than the table -- extras are treated as unchanged (keep their DEFAULT values).
- New changesets applied to old schema: unknown columns at the end are skipped by `sqlite3changeset_apply`. Unknown tables are ignored entirely (they aren't attached to the session on the old-version device).

Additive changes are transparent. A device on schema version 3 can apply changesets from version 2, and vice versa. No coordination needed.

**Breaking:** Delete columns, reorder columns, rename columns, change column types. These MUST bump `min_schema_version`.

Why column deletion is breaking:

- **Deleting a column from the end:** old changesets reference a column index beyond the table's current column count. `sqlite3changeset_apply` fails.
- **Deleting from the middle:** all subsequent column indices shift. Old changesets write to the wrong column. Silent data corruption.
- **Reordering columns:** same problem as middle deletion -- indices no longer match semantics.

Column deletion and reordering are incompatible with unapplied changesets from the old schema. All devices must be on the same schema version before syncing resumes.

#### Epochs

A breaking migration splits the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no changeset replay -- devices pull a snapshot to jump forward.

```
epoch 1 (schema v1)           epoch 2 (schema v2)
  cs-1, cs-2, ..., cs-47        cs-48, cs-49, ...
                       ↑
               min_schema_version bumped to 2
               fresh snapshot written (post-migration)
```

The snapshot IS the migrated state. This means any schema change is possible -- delete columns, rename tables, restructure entirely. The constraint isn't "what can the session extension handle across versions" -- it's simply "all devices must upgrade before they sync again."

#### Upgrade flow for breaking changes

1. A new app version ships with schema version N and a migration that modifies the schema (e.g., drops a column).
2. The first device to upgrade runs the migration locally, then bumps `min_schema_version` to N in the sync bucket.
3. Other devices poll, see `min_schema_version > their_version`, stop syncing, and prompt the user to upgrade.
4. The upgraded device writes a fresh snapshot post-migration (schema version N).
5. Other devices upgrade, pull the new snapshot (which is on schema version N), and resume from epoch N. No replay of pre-migration changesets.

#### Practical constraint

Schema changes should be additive whenever possible. Column deletion should be rare and explicitly gated behind a `min_schema_version` bump. For a music library schema this is natural -- you're almost always adding fields (mood, BPM, lyrics), not removing them. When removal is truly needed (e.g., consolidating two columns into one), treat it as a major version event that requires all devices to upgrade.

---

## Phase 2: User identity and shared libraries

*Goal: multiple people can read and write the same library. Requires keypairs, membership, and signed changesets. This is Tier 3.*

### 2a. User keypairs

Each user generates a global keypair (not per-library):
- **Ed25519** for signing changesets and membership changes.
- **X25519** for encrypting the library key to specific users (key wrapping).

Both derived from the same seed (Ed25519 key can be converted to X25519 via `crypto_sign_ed25519_sk_to_curve25519`). One seed per user, stored in the OS keyring via `KeyService`.

**Why global, not per-library:** A single identity across all libraries means attestations in Phase 4 accumulate under one pubkey, building trust. Cross-library sharing (Phase 3) is simpler when Alice has one key Bob can recognize across contexts. Libraries reference the user's pubkey in their membership chain. Display names are per-library (set during invitation), but the cryptographic identity is global.

Revocation is per-library (remove the pubkey from that library's membership chain), not per-key. If a user's key is compromised, they generate a new keypair and re-join libraries.

**KeyService additions:**

```rust
impl KeyService {
    fn get_or_create_user_keypair(&self) -> Result<UserKeypair, KeyError>;
    fn get_user_public_key(&self) -> Option<Vec<u8>>;
}

struct UserKeypair {
    signing_key: [u8; 64],    // Ed25519 secret key
    public_key: [u8; 32],     // Ed25519 public key
}
```

**Keyring entries (new, NOT library-namespaced):**
- `bae_user_signing_key` -- Ed25519 secret key (hex)
- `bae_user_public_key` -- Ed25519 public key (hex)

Note: `KeyService` today assumes all entries are library-scoped via `self.account(base)`. The global keypair entries need to bypass this namespacing -- add methods that don't call `account()`.

**Dependency:** Use libsodium FFI. Already have the binding in `sodium_ffi.rs`. Add `crypto_sign_ed25519_keypair`, `crypto_sign_ed25519_detached`, `crypto_sign_ed25519_verify_detached`, and `crypto_sign_ed25519_sk_to_curve25519`.

### 2b. Changeset signing

Every changeset pushed to the sync bucket is signed by the author's keypair. The signature covers the changeset bytes (the raw binary changeset from the session extension).

The metadata envelope gains two fields:

```json
{
  "device_id": "dev-abc123",
  "seq": 42,
  "schema_version": 2,
  "message": "Imported Kind of Blue",
  "timestamp": "2026-02-10T14:30:00Z",
  "changeset_size": 4096,
  "author_pubkey": "abcd1234...",
  "signature": "deadbeef..."
}
```

**Backward compatibility for solo users:** A library that has never had a membership chain has unsigned changesets. On pull, if there's no `membership/` prefix in the bucket, accept unsigned changesets (legacy mode). Once a membership chain is created, all future changesets must be signed.

### 2c. Membership chain

An append-only log of membership changes. Stored as individual encrypted files in the sync bucket, not as a single monolithic file (avoids S3 concurrent-write overwrites).

```rust
struct MembershipEntry {
    seq: u64,                      // author's local sequence number
    action: MembershipAction,
    user_pubkey: [u8; 32],
    role: MemberRole,
    timestamp: String,             // HLC
    author_pubkey: [u8; 32],       // who made this change
    signature: [u8; 64],
}

enum MembershipAction { Add, Remove }
enum MemberRole { Owner, Member }
```

**Bucket layout additions:**

```
s3://sync-bucket/
  membership/{author_pubkey_hex}/{seq}.enc   # individual membership entries
  keys/{user_pubkey_hex}.enc                 # library key wrapped to each member
  ...                                         # existing sync data
```

**Note on heads/changes namespace:** Phase 1 keys `heads/` and `changes/` by `device_id`. Phase 2 keeps this. A user may have multiple devices, each with its own changeset stream. Authorship is established cryptographically via the `author_pubkey` in the signed envelope, not via the S3 key path. No namespace migration needed.

**Membership merge on read:** Each client downloads all files under `membership/`, orders entries by HLC, and validates:
- The first entry (lowest HLC) must be `Add` with role `Owner`, self-signed.
- `Add` entries must be signed by a pubkey that was an Owner at that HLC.
- `Remove` entries must be signed by a pubkey that was an Owner at that HLC.
- Concurrent additions of the same member are idempotent (membership is a set).

**Key wrapping:** The library encryption key is wrapped (encrypted) to each member's X25519 public key using `crypto_box_seal`. Each member can unwrap it with their private key.

### 2d. Invitation flow

```
Owner invites Alice:
  1. Alice generates keypair (if she doesn't have one), gives owner her public key
     (QR code, paste, or future: out-of-band exchange)
  2. Owner wraps library key to Alice's X25519 key
     -> uploads keys/{alice_pubkey}.enc
  3. Owner writes membership entry:
     membership/{owner_pubkey}/{seq}.enc = { action: Add, user_pubkey: alice, role: Member }
  4. Owner gives Alice: sync bucket coordinates + region + endpoint

Alice's first connect:
  1. Downloads keys/{alice_pubkey}.enc -> unwraps library key
  2. Downloads and validates membership entries
  3. Downloads snapshot.db.enc, pulls changesets -> applies -> has the full library
  4. Generates her own device_id, starts pushing signed changesets
```

### 2e. Changeset validation on pull (multi-user)

Before applying any changeset:

1. Verify the envelope's signature against `author_pubkey`.
2. Check that `author_pubkey` was a valid member at the changeset's timestamp (walk the membership entries).
3. If either fails, discard the changeset silently.

A revoked user's changesets after their removal timestamp are ignored by all clients.

### 2f. Revocation

```
Owner revokes Bob:
  1. Write membership entry: { action: Remove, user_pubkey: bob_pubkey }
  2. Generate new library encryption key
  3. Re-wrap new key to all remaining members -> update keys/{}.enc
  4. Delete keys/{bob_pubkey}.enc
  5. All future data encrypted with new key
```

**Old data:** Bob had the old key. He can read everything encrypted before revocation. Accept this pragmatically -- he had legitimate access and could have downloaded everything. New data is protected.

**S3 credential revocation:** Orthogonal to crypto revocation. Owner should also revoke Bob's S3 IAM credentials.

### 2g. Attribution UI

Every changeset envelope carries `author_pubkey`. Desktop can show: "Alice imported this release", "Bob changed the cover". Store a local mapping of pubkey -> display name (set when inviting or joining).

### Phase 2 summary

| Component | Changes |
|-----------|---------|
| DB schema | None (changesets carry authorship in the envelope, not the DB) |
| Bucket layout | `membership/{pubkey}/{seq}.enc`, `keys/`; heads/changes stay device-keyed |
| bae-core | `UserKeypair`, `MembershipChain`, signing/verification |
| bae-core | `KeyService` gains global keypair management |
| bae-core | `SyncService` gains membership validation, changeset signing |
| `sodium_ffi.rs` | Add Ed25519 + X25519 + sealed box FFI bindings |
| bae-desktop | Invitation UI (generate invite, paste pubkey, QR) |
| bae-desktop | Members list in settings |
| bae-desktop | Attribution in detail views |
| bae-server | Validate membership entries on boot |
| New dependencies | None (using existing libsodium) |
| User experience | Share a library with friends. See who added what. |

### Migration from Phase 1

Solo users upgrading from Phase 1: their library has no membership entries. On first launch of Phase 2 code, prompt: "Do you want to enable multi-user? This will create your identity." If yes, generate keypair, create membership entry with self as owner, start signing changesets. Existing unsigned changesets are grandfathered as trusted (they predate the membership chain). If no, library stays in legacy mode.

No S3 namespace migration needed.

---

## Phase 3: Cross-library sharing (derived keys)

*Goal: share individual releases between libraries without giving away the full library key. This is a power-user feature for Tier 3.*

### 3a. Key derivation migration

Today, all files in a library are encrypted with the same master key. For cross-library sharing, each release needs its own derived key so it can be shared independently.

**KDF:** `release_key = HKDF-SHA256(master_key, salt=random_32_bytes, info="bae-release-v1:" + release_id)`

Using HKDF (RFC 5869):
- `salt`: a random 32-byte value, generated once per library and stored alongside the master key in the keyring.
- `info`: context string with a version prefix ("v1:") plus the release_id.

**HKDF salt distribution:** Phase 2's key-wrapping payload includes only the master key. Phase 3 members also need the salt to derive release keys. Three options:
1. Include the salt in the wrapped payload (bump the payload format).
2. Store the salt in the bucket (requires master key to decrypt).
3. Derive the salt from the master key: `salt = HMAC-SHA256(master_key, "bae-hkdf-salt-v1")`.

**Decision: Option 3.** Deterministically deriving the salt avoids any distribution problem. The salt should ideally be independent of the key per RFC 5869, but since the master key is high-entropy (32 random bytes), the HMAC-derived salt provides sufficient independence in practice. This requires no changes to Phase 2's key-wrapping format.

**Migration:** Add `encryption_scheme TEXT NOT NULL DEFAULT 'master'` to the `release_files` table. New imports use derived keys (`encryption_scheme = 'derived'`). Old files stay on the master key. Sharing a legacy-encrypted release triggers a one-time re-encryption of that release's files.

**EncryptionService changes:**

```rust
impl EncryptionService {
    fn derive_release_key(&self, release_id: &str) -> [u8; 32] {
        let salt = hmac_sha256(&self.key, b"bae-hkdf-salt-v1");
        hkdf_sha256(&self.key, &salt, format!("bae-release-v1:{}", release_id).as_bytes())
    }
}
```

**Dependency:** `hkdf` crate (or inline using existing `hmac` + `sha2`).

### 3b. Share grants

A share grant gives someone access to one release from your library.

```rust
struct ShareGrant {
    from_library_id: String,
    from_user_pubkey: [u8; 32],
    release_id: String,
    bucket: String,
    region: String,
    endpoint: Option<String>,
    // Release key + optional S3 creds, all wrapped to recipient's X25519 key
    wrapped_payload: Vec<u8>,
    expires: Option<String>,     // RFC 3339
    signature: [u8; 64],
}
```

**Wrapped payload:** Encrypted to the recipient's X25519 key via `crypto_box_seal`:

```rust
struct GrantPayload {
    release_key: [u8; 32],
    s3_access_key: Option<String>,
    s3_secret_key: Option<String>,
}
```

S3 credentials are inside the wrapped payload, never in the clear.

### 3c. Aggregated view

A user's client aggregates all access sources into a unified view. Playback resolves to the appropriate bucket and key at play time.

### Phase 3 summary

| Component | Changes |
|-----------|---------|
| DB schema | `encryption_scheme` on release_files, `share_grants` table |
| bae-core | `EncryptionService::derive_release_key()`, HKDF |
| bae-core | `ShareGrant` struct, creation, verification |
| bae-core | `ShareService` for creating/accepting grants |
| bae-desktop | Share button on release detail view |
| bae-desktop | "Shared with me" section in library |
| bae-desktop | Grant import (paste, file, QR scan) |
| bae-server | Optionally proxy shared release access |
| New dependencies | `hkdf` crate (or inline using existing hmac+sha2) |
| User experience | Share any album with anyone who has a bae keypair. |

---

## Phase 4: Public discovery network

*Goal: decentralized MBID-to-content mapping via the BitTorrent DHT. Users who match releases to MusicBrainz IDs contribute to a public knowledge graph. This is Tier 4.*

### 4a. Attestations

When a user imports a release and matches it to a MusicBrainz release ID, they can sign an attestation:

```rust
struct Attestation {
    mbid: String,              // MusicBrainz release ID
    infohash: String,          // BitTorrent infohash of the release files
    content_hash: String,      // SHA-256 of the ordered file hashes
    format: String,            // e.g., "FLAC", "MP3 320"
    author_pubkey: [u8; 32],
    timestamp: String,
    signature: [u8; 64],
}
```

### 4b. DHT integration

Use the BitTorrent Mainline DHT (BEP 5) for peer discovery. Already have libtorrent FFI for torrenting.

**Rendezvous key:** `rendezvous = SHA-1("bae:mbid:" + mbid)`

**Announce:** For each release with an MBID, announce on the rendezvous key (opt-in).

**Lookup:** Query the DHT for the rendezvous key. Connect to peers. Exchange attestations via BEP 10 extended messages.

### 4c. Peer attestation exchange

BEP 10 extension message for attestation exchange (same format as the old roadmap -- this phase is unchanged).

### 4d. Forward lookup: "I want this release"

Search MusicBrainz -> find MBID -> query DHT -> discover peers -> get attestations -> pick one -> BitTorrent download -> import.

### 4e. Reverse lookup: "What are these files?"

Compute content_hash -> query DHT -> get attestations -> learn the MBID -> pull metadata from MusicBrainz -> auto-tag.

### 4f. Participation controls

- Off by default.
- Enable in settings: "Participate in the bae discovery network"
- Per-release opt-out: mark releases as "private"
- Attestation-only mode: share attestations but don't seed files
- Full participation: attestations + seeding

### Phase 4 summary

| Component | Changes |
|-----------|---------|
| DB schema | `attestations` table |
| bae-core | `Attestation` struct, signing/verification |
| bae-core | `DhtService` for announce/lookup |
| bae-core | `AttestationCache` for gossip |
| Torrent FFI | Expose DHT announce/lookup, BEP 10 extension |
| bae-desktop | Discovery tab (search by MBID, browse attestations) |
| bae-desktop | Network settings (opt-in, per-release controls) |
| bae-desktop | Auto-tag from network |
| bae-server | Optionally participate in DHT (headless node) |
| New dependencies | None (libtorrent already has DHT) |
| User experience | Discover and download music from the network. Auto-tag files. |

---

## Implementation order and dependencies

```
Phase 0: Foundation
  0a. _updated_at columns --------+
  0b. device_id in config --------+
  0c. Session extension spike ----+
  0d. HLC implementation ---------+
  0e. Database write/read split --+
         |
         v
Phase 1: Session-based sync
  1a. Session management ---------+
  1b. Push to sync bucket + images +
  1c. Pull + apply (conflict) ----+
  1d. Snapshots + GC --------------+
  1e. MetadataReplicator removal --+
  1f. Concurrent edit detect ------+
  1g. bae-server sync bucket -----+
         |
         v
Phase 2: Shared libraries      Phase 3: Derived keys
  2a. User keypairs --+          3a. Key derivation --+
  2b. Changeset signing +        (can start in parallel
  2c. Membership -----+           with Phase 2)       |
  2d. Invitation -----+                               |
  2e. Validation -----+                               |
  2f. Revocation -----+                               |
  2g. Attribution ----+                               |
         |                                            |
         v                                            |
  Phase 3 (continued): <-----------------------------+
  3b. Share grants
  3c. Aggregated view
         |
         v
Phase 4: Public discovery
  4a. Attestations
  4b. DHT integration
  4c. Peer exchange
  4d. Forward lookup
  4e. Reverse lookup
  4f. Participation controls
```

Phase 3a (key derivation) can start in parallel with Phase 2 because it only touches the encryption layer.

Phase 4 depends on Phase 2 (keypairs for signing attestations) and benefits from Phase 3.

---

## What can be built incrementally vs. what breaks

### Incremental (backward-compatible)

- **Phase 0:** Purely additive. New columns (with defaults), new config field. No behavior change.
- **Phase 1 push:** New files in the sync bucket. No backward compatibility concern -- the sync bucket is new infrastructure.
- **DHT participation:** Opt-in. Non-participating clients are unaffected.

### Breaking changes

- **Phase 1 (MetadataReplicator removal):** Cloud profiles stop receiving metadata replicas. bae-server must switch to sync bucket mode. Old bae-server instances pointing at a cloud profile stop getting updates. **Mitigation:** Ship the bae-server sync bucket support and MetadataReplicator removal together.

- **Phase 2 (signed changesets):** Once a library has membership entries, unsigned changesets from unknown pubkeys are rejected. **Mitigation:** Membership is an explicit opt-in action.

- **Phase 3a (derived keys):** New files encrypted with derived keys can't be read by old clients. **Mitigation:** `encryption_scheme` column lets old clients skip gracefully.

---

## Hard problems and concrete decisions

### Row-level vs. field-level LWW

The session extension gives us row-level changesets. A changeset for an UPDATE contains only the columns that changed plus the PK. When we REPLACE on conflict, only the changed columns are overwritten -- unchanged columns keep their local values.

This means if Alice edits `title` and Bob edits `year` on the same row, both survive (the changesets touch different columns). True conflicts (same column) are resolved by `_updated_at`. This is functionally equivalent to field-level LWW for non-conflicting edits, without the complexity of per-field timestamps.

### Clock skew tolerance

HLC solves most clock skew issues. The 24-hour future-clock guard prevents a device with a wildly wrong clock from winning all conflicts forever.

### Which tables to sync

Same categorization as before:

| Table | Sync? | Notes |
|-------|-------|-------|
| artists | Yes | |
| albums | Yes | |
| album_discogs | Yes | |
| album_musicbrainz | Yes | |
| album_artists | Yes | |
| releases | Yes | |
| tracks | Yes | |
| track_artists | Yes | |
| release_files | Yes | `source_path` and `encryption_nonce` are device-specific; conflict handler preserves local values |
| audio_formats | Yes | FK dependencies on tracks and release_files handled by CONSTRAINT retry |
| library_images | Yes | Image bytes synced separately |
| storage_profiles | No | Device-specific |
| release_storage | No | Device-specific |
| torrents | No | Device-specific |
| torrent_piece_mappings | No | Device-specific |
| imports | No | Device-specific |

---

## Estimated effort per phase

These are rough engineering-time estimates, not calendar time. Assumes one developer.

| Phase | Effort | Notes |
|-------|--------|-------|
| 0a-0e | 2-3 weeks | Column additions, HLC, session extension spike, Database write/read split. |
| 1a-1c | 3-4 weeks | Session management, push/pull, conflict handler, image sync. |
| 1d-1g | 1-2 weeks | Snapshots, GC, bae-server, MetadataReplicator removal. |
| 2a-2c | 2-3 weeks | Keypairs, membership entries, signed changesets. |
| 2d-2g | 2-3 weeks | Invitation UX, validation on pull, attribution UI. |
| 3a | 1-2 weeks | Key derivation + dual-scheme support. |
| 3b-3c | 2-3 weeks | Share grants, aggregated view, playback from remote sources. |
| 4a-4c | 3-4 weeks | DHT integration, attestation format, peer exchange protocol. |
| 4d-4f | 2-3 weeks | Lookup flows, UI, participation controls. |
| **Total** | **~18-27 weeks** | |

Phase 0 + Phase 1 together (~6-9 weeks) deliver the most immediate value: faster sync for every cloud user, simpler architecture (no MetadataReplicator, no per-profile metadata replica), and the foundation for multi-device.

---

## Testing strategy

### Changeset sync (Phase 1)

This is the highest-risk component. Test with:

1. **Determinism tests:** Two databases applying the same set of changesets in the same order produce identical state. (Note: unlike the op log, changeset application order matters -- they must be applied in seq order per device.)
2. **Conflict tests:** Same row edited by two devices. Later `_updated_at` wins.
3. **Column-level merge tests:** Alice edits `title`, Bob edits `year` on the same row. Both survive after applying both changesets. This validates that the session extension only includes changed columns.
4. **Delete-vs-edit tests:** Delete on one device, edit on another. Row stays deleted (NOTFOUND -> OMIT).
5. **Clock skew tests:** Device with clock 1 hour ahead. HLC comparison resolves correctly.
6. **Snapshot tests:** Snapshot + subsequent changesets produces same state as applying all changesets from scratch.
7. **FK constraint tests:** audio_formats changeset arrives before tracks changeset. CONSTRAINT -> OMIT. After tracks changeset is applied, retry succeeds.
8. **Session isolation tests:** Verify that applying incoming changesets while NO session is active does not contaminate the next outgoing changeset.
9. **Image sync tests:** Changeset references an image. Image is uploaded before changeset. Pull-side detects missing image and downloads it.
10. **Empty changeset tests:** No changes since last session -- push is skipped, no empty S3 objects created.

### Membership chain (Phase 2)

Same as before:
1. Entry validation -- tampered entry rejected.
2. Concurrent additions -- both entries accepted.
3. Revocation -- changesets after revocation timestamp discarded.
4. Key wrapping -- wrap/unwrap roundtrip with X25519.
5. Multi-member merge -- three users pushing changesets simultaneously all arrive at same state.

### Share grants (Phase 3)

1. Key derivation determinism.
2. Cross-library playback.
3. Expired grant rejection.
4. Wrapped payload security.

### DHT (Phase 4)

1. Announce/lookup roundtrip.
2. Attestation gossip.
3. Signature verification.
