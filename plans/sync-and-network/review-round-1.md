# Sync & Network Roadmap -- Review Round 1

## Summary judgment

This is a well-structured roadmap that correctly identifies the progression from single-device sync to decentralized network. The phasing is sound -- each phase ships independently and has clear value. The technical decisions (row-level LWW, HLC, per-field timestamps, libsodium reuse) are solid defaults for a music library where conflicts are low-stakes.

However, there are several accuracy problems where the roadmap mischaracterizes what currently exists, a critical gap in the op log merge semantics around multi-table transactions, underspecified migration paths at two phase boundaries, and some effort estimates that seem optimistic given the surface area.

**Verdict: Request Changes** -- the critical issues around transactional atomicity, table name errors, and the encryption nonce misconception need to be corrected before this roadmap can guide implementation.

---

## Critical issues

### 1. The `files` table is called `files`, not `release_files`

The roadmap's "What exists today" section (line 11) says:

> Files use hash-based paths (`storage/ab/cd/{file_id}`). Cloud files are chunked-encrypted with per-file random nonces (stored in `release_files.encryption_nonce`)

The actual table is `files`, not `release_files`:

```
-- bae-core/migrations/001_initial.sql, line 94
CREATE TABLE files (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    ...
    encryption_nonce BLOB,
    ...
);
```

The Rust model is `DbFile` (in `bae-core/src/db/models.rs`, line 214). The design doc `00-data-model.md` does use the heading "release_files" conceptually, but the actual SQL table and all code references use `files`. The roadmap's "Which tables to track" table on line 857 correctly uses `files`, but the earlier reference is wrong. This matters because Phase 3a proposes adding an `encryption_scheme` column:

```sql
ALTER TABLE files ADD COLUMN encryption_scheme TEXT NOT NULL DEFAULT 'master';
```

The SQL is correct (uses `files`), but the prose description on line 497 says "Add an `encryption_scheme` column to `files`" which is right -- just the earlier `release_files.encryption_nonce` reference is inconsistent.

**Fix:** Replace `release_files.encryption_nonce` with `files.encryption_nonce` on line 11.

### 2. Encryption nonces are NOT "stored in release_files.encryption_nonce" for random-access -- they're stored at the front of the encrypted blob

The roadmap (line 11) says cloud files have "per-file random nonces (stored in `release_files.encryption_nonce`) enabling random-access decryption for streaming." This is subtly misleading about the current architecture.

Looking at `bae-core/src/encryption.rs` (line 121-179), the `encrypt_chunked` method prepends the 24-byte base nonce to the output. The `encryption_nonce` column in the DB is optional (`BLOB`, nullable) and is used as an optimization for `decrypt_range_with_offset` (line 336-434) -- it lets the reader avoid fetching the first 24 bytes from cloud storage separately. But the nonce is ALSO embedded in the encrypted data itself.

The important nuance for the roadmap: when Phase 3a proposes changing to derived keys, the nonce format doesn't change -- only the key does. The nonce is not derived from the key. This is correct in the roadmap's design, but the initial characterization implies a tighter coupling between nonce storage and encryption scheme than actually exists.

### 3. Transactional atomicity gap in the op log model

The roadmap proposes row-level ops, where an import generates ~50 individual insert ops. Today, `insert_album_with_release_and_tracks` (`bae-core/src/db/client.rs`, line 515) wraps album + release + tracks in a single SQLite transaction. If the import fails midway, it rolls back atomically.

Under the op log model, these 50 ops would be a batch pushed as a single `ops/{device_id}/{seq}.enc` S3 object. But on the **pull side**, the merge algorithm (roadmap lines 243-262) processes ops individually:

```
for each op (ordered by HLC):
    match op.action:
        Insert: ...
        Update: ...
        Delete: ...
```

**The problem:** If a pull is interrupted after replaying 25 of 50 ops from a batch, the receiver has half an import -- an album with some tracks but missing others, files without audio_formats, etc. The roadmap doesn't specify whether batch replay is atomic (all-or-nothing within a local transaction).

**Required addition:** The merge replay MUST wrap each batch in a local SQLite transaction. If any op in the batch fails (e.g., FK constraint because the album insert wasn't processed yet due to ordering), roll back the entire batch and retry. The roadmap should specify this, because the current "for each op" pseudocode implies one-at-a-time application.

Additionally, within a batch the ops must be applied in dependency order (parent before child -- albums before releases before tracks), not just HLC order. An import generates all ops with the same HLC timestamp, so HLC ordering alone is insufficient. The roadmap should specify intra-batch ordering: topological order by table FK dependencies, or simply the natural insertion order within the batch.

### 4. The `LibraryManager` is the write gateway, not `Database` directly

The roadmap (line 87-88) says:

> Today, `LibraryManager` methods like `insert_album_with_release_and_tracks`, `mark_track_complete`, etc. call `Database` directly.

This is correct. But the `OpRecorder` proposal wraps `Database`:

```rust
struct OpRecorder {
    db: Database,
    ...
}
```

This is the wrong layer. Write calls flow through `LibraryManager` -> `Database`. If `OpRecorder` wraps `Database`, then LibraryManager's higher-level methods (which compose multiple Database calls in transactions, and emit `LibraryEvent::AlbumsChanged`) would need to be rewritten to go through `OpRecorder` instead.

The roadmap acknowledges this ("All `LibraryManager` write methods go through it") but the struct definition shows it wrapping `Database`, not sitting between `LibraryManager` and `Database`. The more natural design is:

- `LibraryManager` holds `OpRecorder` instead of `Database`
- `OpRecorder` holds `Database` and intercepts each low-level call
- Or: `OpRecorder` is a trait that `Database` implements, and `LibraryManager` uses the trait

Either way, the roadmap should be explicit about where `OpRecorder` sits. If it wraps `Database`, then `LibraryManager.database()` returns an `OpRecorder` (breaking the public API that bae-server and import code also call). If it wraps `LibraryManager`, the interception is at a higher level.

### 5. `device_id` to `user_pubkey` migration (Phase 1 -> Phase 2) is underspecified

Phase 1 keys everything by `device_id`:
```
heads/{device_id}.json.enc
ops/{device_id}/{seq}.enc
```

Phase 2 changes to:
```
heads/{user_pubkey_hex}.json.enc
ops/{user_pubkey_hex}/{seq}.enc
```

The roadmap (line 471-473) says existing solo users' ops are grandfathered as unsigned. But it doesn't address the S3 key namespace change. When a Phase 2 client starts, it would need to:

1. Read the old `heads/{device_id}` entries
2. Create new `heads/{user_pubkey}` entries
3. Either move or alias the old `ops/{device_id}/` prefix

Or keep both namespaces and have the pull logic check both. This is a non-trivial migration that the roadmap should address explicitly, because S3 doesn't support renaming keys -- you'd need to copy and delete.

---

## Design concerns

### 6. The `field_timestamps` table will be enormous and expensive

For a medium library (1000 albums, 15000 tracks, 30000 files), the `field_timestamps` table would have roughly:

- artists: ~2000 rows x ~8 fields = 16,000 entries
- albums: ~1000 x ~10 fields = 10,000
- tracks: ~15000 x ~8 fields = 120,000
- files: ~30000 x ~8 fields = 240,000

Total: ~400,000 rows in `field_timestamps` for the initial backfill alone. Each row has 5 columns of text. This is a substantial table.

The roadmap says "reads only happen during merge" (line 54). But during merge, for each incoming Update op, you need to look up the local HLC for that specific field. With the proposed index on `(table_name, row_id)`, this is an index seek + scan over all fields for that row. Acceptable, but worth noting this is not free.

**Alternative worth considering:** Instead of a separate table, add a single JSON column `field_hlcs` to each tracked table. Stores `{"title": "1234-0-dev1", "year": "1235-0-dev1"}`. One row per entity, not one row per field. Simpler queries (`SELECT field_hlcs FROM albums WHERE id = ?`), no join. The downside is wider rows in the main tables, but they're already wide. This trades table count for column width.

The roadmap should at least acknowledge the size concern and state why the separate table was chosen over alternatives.

### 7. Compaction race condition and garbage collection safety

The roadmap (line 264-275) says:

> The device that triggers the threshold creates it. No coordination needed -- worst case two devices create near-simultaneous checkpoints, which is harmless.

Near-simultaneous checkpoints are harmless for correctness, but garbage collection is not:

- Device A creates checkpoint at HLC T100, covering all ops up to seq 50 per device
- Device A starts deleting ops before seq 50
- Device B was offline since T90 and comes online
- Device B reads `heads/`, sees it's behind, tries to fetch ops from seq 40
- Those ops were just deleted by A

The roadmap says "Ops before the checkpoint can be garbage collected" but doesn't specify a safety margin. Standard practice is to keep ops for a configurable grace period after checkpointing (e.g., 7 days), or to never GC ops that are newer than the oldest known cursor. Since the roadmap has no central coordinator, there's no way to know the oldest cursor across all devices.

**Suggestion:** Either (a) never GC ops automatically -- let the user trigger it manually, or (b) keep ops for a generous grace period (e.g., 30 days past the checkpoint), or (c) require all devices to advance their cursors before GC (detectable via `heads/` entries).

### 8. The write lock mechanism is insufficient for the stated goal

The roadmap (lines 289-293) proposes a heartbeat-based advisory lock via `heads/{device_id}.json.enc`:

> Before pushing, check if another device has a recent heartbeat (< 5 minutes).

But `heads/` is read by listing and then fetching each entry. That's at least 1 LIST + N GET calls. If a user is pushing frequently (every mutation + 30s timer), checking all heads before each push adds significant latency and S3 cost.

More importantly, the lock is per-device in Phase 1 but per-user in Phase 2. In Phase 2, the same user on two devices is a legitimate multi-writer -- they'd always see their own heartbeat as "recent" and the warning would always fire.

**Suggestion:** Simplify. Either drop the advisory lock entirely (LWW handles conflicts, the UI can show "last synced from device X at time T" without a lock check), or implement it as a separate `lock.json.enc` file with a single heartbeat entry, updated only by the actively-writing device.

### 9. Image sync is hand-waved

The roadmap (lines 222-228) correctly identifies that images don't fit the op log, and recommends Option 1: "op records the image ID, image bytes stay in `images/`." But the implementation details are missing:

- When the merger sees an `Insert` op for `library_images`, it inserts the DB row. But how does it know the image file hasn't been uploaded yet? (The uploader might crash between pushing the op and uploading the image.)
- Who retries the image upload? The op is already marked as pushed.
- On pull, how does the receiver discover which images are missing? A full `images/` LIST on every pull? That's O(n) in the number of images.

**Suggestion:** Add an `image_sync_pending` flag to the op, or maintain a separate image manifest (list of image IDs and their S3 keys) alongside the op log, updated atomically with the head pointer.

### 10. The membership chain is a single file -- concurrent modifications lose data

The membership chain (`membership.enc`) is a single encrypted file in the bucket. When two owners try to add members simultaneously:

1. Owner A downloads `membership.enc`, appends entry for Alice, uploads
2. Owner B downloads `membership.enc` (before A's upload), appends entry for Bob, uploads
3. Owner B's upload overwrites Owner A's, and Alice's membership entry is lost

S3 does not have conditional writes (the roadmap acknowledges this on line 293). The membership chain needs its own conflict resolution. Options:

- Use S3 object versioning and merge on read
- Store the chain as multiple files (`membership/{seq}.enc`) like the op log
- Require a single designated admin device for membership changes

The roadmap should address this. It's a genuine problem for any multi-admin library.

### 11. Per-library keypairs vs. global identity

The roadmap (lines 346-348) notes:

> The keypair is per-library because a user might want different identities in different libraries. But typically they'll have one keypair and use it everywhere.

Per-library keypairs create a significant UX problem in Phase 4 (public discovery). If Alice attests MBIDs from two libraries using two different keypairs, other peers see two separate identities. Alice's attestation count is split, reducing her perceived trustworthiness. Peers who trust Alice in one context have no way to know it's the same person in another.

This also complicates Phase 3 (cross-library sharing). When Alice shares a release with Bob, she signs the grant with her library-specific key. If Bob later wants to check Alice's other attestations (Phase 4), he can't correlate them.

**Suggestion:** Make the keypair global (stored in the OS keyring under a non-library-namespaced entry). The keypair identifies the user, not the library. Libraries reference the user's public key in their membership chain. A user has one identity everywhere, with optional per-library aliases for display names.

### 12. HKDF info parameter should include version or domain separator

The roadmap (line 485) proposes:

```
release_key = HKDF-SHA256(master_key, salt="bae-release-key", info=release_id)
```

The salt and info parameters are reversed from standard HKDF usage. Per RFC 5869:
- `salt`: optional, ideally random, used in the Extract step
- `info`: context-specific, used in the Expand step

Using `"bae-release-key"` as the salt and `release_id` as info is technically fine but non-standard. More concerning: there's no version number. If the KDF scheme ever changes (different salt, different info format), old and new keys would silently differ for the same release_id. Include a version: `info = "v1:" + release_id`.

### 13. ShareGrant includes plaintext S3 credentials -- security concern

The `ShareGrant` struct (lines 526-542) optionally includes `s3_access_key` and `s3_secret_key`:

```rust
s3_access_key: Option<String>,
s3_secret_key: Option<String>,
```

These are signed by the sharer but transmitted in the clear (within the grant blob). If the grant is sent over an insecure channel (email, paste, etc.), S3 credentials are exposed. The release key is wrapped to the recipient's public key (safe), but the S3 creds are not.

**Fix:** Either (a) wrap the S3 creds alongside the release key using the recipient's public key, or (b) remove them from the grant and handle S3 access separately (e.g., via a proxy, or the recipient provides their own creds with cross-account bucket access).

---

## Questions

### 14. What happens when a device re-installs and loses its `device_id`?

Phase 0b generates `device_id` on first launch and stores it in `config.yaml`. If a user re-installs bae (or deletes `config.yaml`) on the same machine, they get a new `device_id`. Now the bucket has orphaned `heads/` and `ops/` entries for the old device_id. These will never advance and could confuse compaction/GC heuristics.

Does this matter? Probably not much, but the roadmap should state explicitly that orphaned device_ids are harmless (GC handles them) or describe cleanup.

### 15. How does bae-server participate in the op log world?

The roadmap (line 133) says "bae-server: No changes (read-only)" for Phase 0, and (line 305) "bae-server: Can optionally pull ops on startup (`--refresh`)" for Phase 1.

But today bae-server downloads `library.db.enc` as a full snapshot (see `bae-server/src/main.rs`, `download_from_cloud` function, line 290). Under the op log model, `library.db.enc` becomes a periodic checkpoint, not a per-mutation snapshot.

This means bae-server with `--refresh` would need to:
1. Download the latest checkpoint
2. Pull and replay ops since that checkpoint
3. Do all of this with a writable DB (currently it opens `Database::open_read_only`)

This is a significant change to bae-server that the roadmap doesn't scope. It needs its own sub-phase or explicit deferral.

### 16. What about the `audio_formats` table?

The "Which tables to track" section (line 856) marks `audio_formats` as "No -- Computed from files at import time." But `audio_formats` is 1:1 with tracks and contains data that's expensive to recompute (FLAC frame scanning for seektables, byte offset calculations). If a device pulls new track ops but doesn't have the audio files locally, it can't regenerate `audio_formats`.

Should `audio_formats` be synced? Or should it be lazily computed when files are transferred to the device? The roadmap should decide, because it affects whether new-device-from-checkpoint can play music without a full re-scan.

### 17. How does delete-then-insert interact with tombstones?

The merge algorithm (lines 243-262) says:

```
Insert:
    if row exists AND is not tombstoned:
        skip (duplicate insert)
    else if row is tombstoned AND tombstone HLC > op HLC:
        skip (delete wins)
    else:
        insert row, update field_timestamps
```

What about: Device A deletes a release. Device B, unaware of the delete, imports the same release (same MBID, different release_id). This is a new row with a new ID -- the tombstone is on the old ID, not the new one. So it inserts fine. Good.

But what about: Device A deletes release `rel-123`. Later, Device A re-imports the same files and gets `rel-123` again (UUIDs are random, so this won't happen -- but what if there's a deterministic ID scheme in the future?). The tombstone for `rel-123` has an earlier HLC. The re-insert has a later HLC. Per the algorithm, the insert wins (tombstone HLC < insert HLC). Good -- tombstones don't prevent legitimate re-inserts.

This seems correct but the roadmap should explicitly state that tombstones are HLC-compared, not permanent. The phrase "tombstone prevents resurrection" in the testing section (line 909) is misleading -- it should say "tombstone prevents resurrection **from earlier ops**."

---

## What's good

### The phasing is right

Each phase delivers independent value. Phase 0+1 benefit every cloud user immediately (faster sync). Phase 2 is opt-in per library. Phase 3 is per-release. Phase 4 is global opt-in. No phase forces complexity on users who don't want it. This respects the tier model from `01-library-and-cloud.md` well.

### Row-level LWW is the right call

The roadmap's decision to use row-level ops with field-level LWW (section "Op schema: row-level vs. semantic ops", line 830) is correct. Semantic ops would be a maintenance nightmare for this codebase -- there are already 33 write methods in `Database` and 30+ in `LibraryManager`. Each new feature would need its own op type. Row-level ops are mechanical and generic.

### HLC is the right clock choice

The hybrid logical clock decision (Phase 0c) correctly addresses wall-clock skew without requiring NTP synchronization or a central time authority. The 24-hour future-clock protection (line 838) is a good practical guard.

### Reusing libsodium is pragmatic

The decision to use existing libsodium FFI bindings for Ed25519/X25519 (Phase 2a, line 352) rather than adding `ed25519-dalek` avoids a new dependency. The existing `sodium_ffi.rs` is minimal (41 lines) and adding 4-5 more function bindings is straightforward.

### The table-sync categorization is thoughtful

The "Which tables to track" table (lines 844-862) makes reasonable decisions. Excluding `storage_profiles`, `release_storage`, and `imports` (device-specific) while syncing `albums`, `artists`, `tracks` is correct. The nuanced treatment of `files` (sync existence but not `source_path` or `encryption_nonce`) shows understanding of the dual-purpose nature of that table.

### The testing strategy targets the right risks

The testing plan (lines 903-930) correctly identifies determinism tests, conflict tests, and the delete-vs-edit case as highest priority. The compaction-equivalence test (line 912) is particularly important and often missed.

---

## Specific corrections

### Table name: `files` not `release_files`

- Line 11: `release_files.encryption_nonce` should be `files.encryption_nonce`
- The data model doc (`00-data-model.md`) uses the heading "release_files" but the actual DB table is `files`
- The roadmap's table on line 857 correctly uses `files` -- just the early reference is wrong

### Missing table from the list

The roadmap's "DB schema" summary (line 13) lists tables:

> artists, albums, album_discogs, album_musicbrainz, album_artists, releases, tracks, track_artists, files, audio_formats, library_images, storage_profiles, release_storage, torrents, torrent_piece_mappings, imports

This is complete. All 16 tables from `001_initial.sql` are accounted for.

### Write method count

The roadmap says "The existing `Database` has ~40 write methods" (line 116). The actual count is 33 write methods in `Database` (insert/update/delete/set/upsert/link). Close enough -- but note that `LibraryManager` has its own 30+ methods that delegate to `Database`. The OpRecorder needs to intercept at one layer, not both.

### Dependency: `keyring-core` not `keyring`

The roadmap (line 19) lists `keyring-core` as a dependency, which matches the actual Cargo.toml:

```
keyring-core = "0.7"
```

Correct.

### No `libsodium` crate -- it's raw FFI

The roadmap says "libsodium FFI for encryption" (line 19) which is accurate. Just to be precise: there's no Rust `libsodium` crate -- it's hand-written FFI in `sodium_ffi.rs` against the system libsodium library. The roadmap correctly recommends extending this FFI rather than adding a new crate.

### config.yaml has no `device_id` field yet

The roadmap (Phase 0b) proposes adding `device_id` to `ConfigYaml`. Confirmed: the current `ConfigYaml` (`bae-core/src/config.rs`, lines 51-92) does not have a `device_id` field. This is accurate -- it needs to be added.

### MetadataReplicator exists and works as described

The roadmap's description of `MetadataReplicator` ("pushes a full VACUUM INTO DB snapshot plus all images to every non-home profile") matches the actual implementation in `bae-core/src/metadata_replicator.rs`. The `sync_all` method (line 61) does exactly this: VACUUM INTO -> upload to each replica profile.

### LibraryEvent::AlbumsChanged exists

Confirmed at `bae-core/src/library/manager.rs`, line 35:

```rust
pub enum LibraryEvent {
    AlbumsChanged,
}
```

The roadmap's reference to this as the sync trigger is accurate.

### Database::open_read_only exists

Confirmed at `bae-core/src/db/client.rs`, line 45. The roadmap's claim that bae-server uses read-only mode is accurate.

### The torrent FFI claim is accurate

The roadmap says "Full BitTorrent integration via libtorrent (C++ FFI)." This is confirmed by `bae-core/src/torrent/ffi.rs` which uses `cxx::bridge` to bind to libtorrent's session, add_torrent_params, and custom storage constructor. The `info_hash` field exists on `DbTorrent` (line 655 of models.rs). UPnP/NAT-PMP configuration exists in `ConfigYaml` (lines 73-78 of config.rs).

### The roadmap omits `apple-native-keyring-store`

The dependency list (line 19) says `keyring-core + apple-native-keyring-store for secrets`. The `init_keyring()` function in `config.rs` (line 14-33) confirms this dependency is used on macOS. This is relevant because Phase 2's user keypair storage would also go through this same keyring infrastructure.
