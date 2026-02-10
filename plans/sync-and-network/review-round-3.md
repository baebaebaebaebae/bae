# Sync & Network Roadmap -- Review Round 3

## Verdict: Approve

The roadmap has been fundamentally revised from the op log approach to the SQLite session extension. This is a better design: it eliminates the OpRecorder (~63 method wrappers), field_timestamps table, custom JSON op format, custom merge algorithm, and tombstone management. What remains is dramatically simpler -- ~50 lines of session management, ~40 lines of conflict handler, and the same push/pull/snapshot infrastructure.

The two documents (roadmap and vision doc) are internally consistent and agree with each other. The technical claims about the codebase are accurate. The round 1 and round 2 findings have been resolved (verified below). There are two new findings -- one significant (pool/connection architecture for sessions) and one minor (session feature enablement path) -- plus a few notes. None are blockers.

---

## Verification of previous findings

### Round 1 findings -- all resolved

**R1-1. Table name `files` vs `release_files`**: Fixed. Roadmap line 11 correctly says `files.encryption_nonce`.

**R1-2. Encryption nonce characterization**: Rewritten. Line 11 now correctly explains the nonce is embedded as the first 24 bytes of the encrypted blob by `encrypt_chunked` (confirmed at `/Users/dima/dev/bae/bae-core/src/encryption.rs` line 130: `let mut output = base_nonce.to_vec()`), with `files.encryption_nonce` as a cached copy. The sentence "The nonce and the encryption key are independent: changing the key (Phase 3) does not affect nonce handling" is accurate and clarifies the Phase 3 interaction.

**R1-3. Transactional atomicity**: Eliminated as a concern. The session extension captures all changes as a single binary changeset per sync cycle. One changeset = one application-level transaction. The complex batch atomicity logic from the op log model is no longer needed. Line 52: "~~Batch atomicity logic~~ (one changeset = one application-level transaction)".

**R1-4. OpRecorder layering**: Eliminated entirely. No OpRecorder. The session extension tracks changes at the C level. Line 23: "The app writes normally. SQLite records what changed."

**R1-5. S3 namespace migration (device_id to user_pubkey)**: Resolved in round 2. Phase 2 keeps `heads/` and `changes/` keyed by `device_id` (roadmap line 530). Authorship established via `author_pubkey` in the signed envelope.

### Round 2 findings -- both resolved

**N1. HKDF salt distribution**: Resolved. Roadmap lines 627-632 specify Option 3 -- deriving the salt deterministically from the master key: `salt = HMAC-SHA256(master_key, "bae-hkdf-salt-v1")`. The rationale is correct: the master key is high-entropy (32 random bytes), so the HMAC-derived salt provides sufficient independence. This requires no changes to Phase 2's key-wrapping format.

**N2. audio_formats FK ordering**: Resolved. Roadmap lines 244-245 explicitly call out the FK dependency: "If an `audio_formats` INSERT arrives before the referenced `tracks` or `files` row, the FK constraint fails." The conflict handler returns OMIT for CONSTRAINT conflicts, and retries after all other changesets are processed (line 367).

---

## New findings

### 1. [Significant] Pool vs. connection architecture for session tracking

The SQLite session extension attaches to a single database connection. It captures changes made through that connection only. The current `Database` struct uses `SqlitePool` (confirmed at `/Users/dima/dev/bae/bae-core/src/db/client.rs` line 26):

```rust
pub struct Database {
    pool: SqlitePool,
}
```

All 33 write methods execute against `&self.pool`, which dispatches to whichever connection in the pool is available. If the session is attached to connection A, but a write goes through connection B, that write is NOT captured in the changeset.

The roadmap (line 207) shows `SyncSession::new(conn: &mut LockedSqliteHandle)` which implies awareness that sessions are per-connection. But it does not address how the pool-based `Database` architecture needs to change.

For the session extension to work, one of the following is needed:

a. **Single-connection pool**: Configure the pool with `max_connections(1)` so all writes go through the same connection. Simple but eliminates concurrent reads.

b. **Dedicated write connection**: Separate the pool into a single write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This is a moderate refactor of `Database`.

c. **Pin the session to a pool connection**: Acquire a connection from the pool for the duration of the sync session, write through it, and return it when the session ends. But this means all writes during a sync cycle must be manually routed through that specific connection -- not compatible with the current `&self.pool` pattern.

Option (b) is the most natural and matches SQLite's single-writer-multiple-reader model. The roadmap should specify this, because it affects the Phase 1a implementation significantly (modifying the `Database` struct is foundational work, not a detail).

**Suggested addition to Phase 1a:** "The `Database` struct must be refactored to separate a dedicated write connection (with session attached) from the read pool. All write methods execute against the write connection; read methods continue using the pool. This matches SQLite's single-writer-multiple-reader architecture."

### 2. [Minor] Session extension enablement path through sqlx

The roadmap (line 69) says: "The session extension requires the `session` feature flag on `libsqlite3-sys`, which sets `SQLITE_ENABLE_SESSION` and `SQLITE_ENABLE_PREUPDATE_HOOK` at compile time, and requires `buildtime_bindgen`."

Verified: `libsqlite3-sys 0.30.1` (the version in the dependency tree) has `session` and `preupdate_hook` as separate features. However, `sqlx-sqlite 0.8.6` only exposes `preupdate-hook` as a feature -- there is no `session` feature on `sqlx-sqlite`.

This means enabling the session extension requires adding `libsqlite3-sys` as a direct dependency in `bae-core/Cargo.toml` with the `session` feature:

```toml
[dependencies]
libsqlite3-sys = { version = "0.30", features = ["session"] }
```

This works because Cargo unifies features across the dependency graph -- enabling `session` on the direct dep also enables it on the copy used by sqlx-sqlite. The `preupdate_hook` feature is also needed and can be enabled through sqlx's `sqlite-preupdate-hook` feature or directly on `libsqlite3-sys`.

The roadmap's fallback suggestion ("we can set `LIBSQLITE3_FLAGS`") is a valid alternative. But the primary path should be clarified: add `libsqlite3-sys` as a direct dependency with feature flags, not just rely on sqlx's feature propagation.

This is marked minor because the Phase 0c spike (line 133-147) will validate the exact build configuration before any production code depends on it. The spike is well-scoped for this purpose.

---

## Design verification

### Technical claims cross-referenced against codebase

| Claim | Location | Verified? |
|-------|----------|-----------|
| "Four tables have `updated_at`" (artists, albums, releases, storage_profiles) | `001_initial.sql` lines 10, 22, 65, 175 | Yes |
| "All IDs are UUIDv4 strings" | `001_initial.sql` (TEXT PRIMARY KEY throughout) | Yes |
| "Timestamps are RFC 3339 strings" | `client.rs` (`.to_rfc3339()` throughout) | Yes (except `imports.updated_at` which is INTEGER -- but imports is device-specific, not synced) |
| "`Database::open_read_only`" | `client.rs` line 45 | Yes |
| "sqlx 0.8 exposes raw `sqlite3*` via `LockedSqliteHandle::as_raw_handle()`" | `sqlx-sqlite 0.8.6` source, `connection/mod.rs` line 378: `pub fn as_raw_handle(&mut self) -> NonNull<sqlite3>` | Yes |
| "sqlx uses bundled `libsqlite3-sys`" | `sqlx` feature `sqlite` includes `sqlx-sqlite/bundled` | Yes |
| `libsqlite3-sys` has `session` feature | `libsqlite3-sys 0.30.1` features list | Yes |
| "MetadataReplicator pushes a full VACUUM INTO DB snapshot plus all images" | `/Users/dima/dev/bae/bae-core/src/metadata_replicator.rs` line 74-78: `vacuum_into` then `upload` | Yes |
| "bae-server downloads `library.db.enc` on boot" | `/Users/dima/dev/bae/bae-server/src/main.rs` line 389 | Yes |
| `ConfigYaml` has no `device_id` field | `/Users/dima/dev/bae/bae-core/src/config.rs` lines 52-92 | Yes |
| `KeyService` uses library-scoped `account()` for all entries | `/Users/dima/dev/bae/bae-core/src/keys.rs` line 39-41 | Yes |
| "33 write methods in Database" | `client.rs` (grep for `pub async fn insert_\|update_\|delete_\|set_\|upsert_\|link_`) | Yes, exactly 33 |
| "~6 FFI functions needed" | `sqlite3session_create`, `_attach`, `_changeset`, `_delete`, `sqlite3changeset_apply`, `sqlite3_free` | Plausible |
| "XChaCha20-Poly1305, 64KB chunks, per-file random nonce prepended" | `/Users/dima/dev/bae/bae-core/src/encryption.rs` lines 9, 121-130 | Yes |
| "Nonce also cached in `files.encryption_nonce`" | `001_initial.sql` line 101: `encryption_nonce BLOB` | Yes |

### Table sync categorization verified

The 11 synced tables and 5 non-synced tables match the schema correctly:

**Synced (11):** `artists`, `albums`, `album_discogs`, `album_musicbrainz`, `album_artists`, `releases`, `tracks`, `track_artists`, `files`, `audio_formats`, `library_images`

**Not synced (5):** `storage_profiles`, `release_storage`, `torrents`, `torrent_piece_mappings`, `imports`

The `storage_profiles` table has an existing `updated_at` column but is correctly excluded from syncing (device-specific) and from the `_updated_at` rename migration.

### Conflict handler semantics

The conflict handler (roadmap lines 317-361) correctly maps to the SQLite session extension's conflict types:

- `DATA` (same row modified both sides): LWW by `_updated_at`. Correct.
- `NOTFOUND` (row deleted locally, incoming UPDATE): OMIT. Delete wins. Correct.
- `CONFLICT` (INSERT for existing PK): LWW by `_updated_at`. Correct.
- `CONSTRAINT` (FK violation): OMIT and retry. Correct.

The claim that "a changeset for an UPDATE contains only the columns that changed" (line 363) is accurate for the session extension -- it records old/new values only for modified columns, and `REPLACE` only overwrites those columns.

### Session lifecycle and ordering

The protocol (lines 190-201) correctly specifies the critical ordering rule: end the session before applying incoming changesets, then start a new session. This prevents changeset pollution. The testing strategy (line 908) explicitly tests this.

### Two-document consistency

The vision doc (`07-sync-and-network.md`) and the roadmap agree on all points:

- Both describe the session extension approach (no op log, no triggers).
- Both list the same bucket layout.
- Both use the same conflict resolution model (row-level LWW with `_updated_at`).
- Both describe the same 4-layer progression.
- The vision doc is deliberately higher-level; it does not contradict any specifics in the roadmap.

No drift detected.

---

## Notes (not issues)

### N1. HLC format and lexicographic comparison

The roadmap (line 158) specifies HLC format as `"{millis}-{counter}-{device_id}"` with millis zero-padded to 13 digits. This makes lexicographic comparison work because millis dominates. Worth noting during implementation: the zero-padding MUST be enforced, because `"9999999999999-0-dev1"` (13 digits) sorts after `"1000000000000-0-dev1"` (13 digits), but `"999-0-dev1"` (3 digits) sorts after `"1000-0-dev1"` (4 digits) lexicographically. The roadmap correctly states "zero-padded to 13 digits" -- just ensure the implementation does not skip the padding.

### N2. `buildtime_bindgen` dependency for session extension

The roadmap mentions that the `session` feature on `libsqlite3-sys` "requires `buildtime_bindgen`". Looking at the `libsqlite3-sys` feature definitions, `session` depends on `buildtime_bindgen` because the session extension APIs are not in the pre-generated bindings. This means enabling `session` also pulls in `bindgen` as a build dependency. The `bindgen` crate requires `libclang`, which is typically available on developer machines but may need attention in CI. Phase 0c (the spike) will surface this if it's a problem.

### N3. The `image_sync_queue` table from round 2 is gone

The previous roadmap version had an `image_sync_queue` table for tracking pending image uploads. The session extension approach eliminates this: image sync is driven by scanning the changeset for `library_images` changes (roadmap lines 275-283). The changeset itself is the queue. This is simpler and correct.

### N4. Effort estimates

The revised total (17-26 weeks) reflects the reduced scope from eliminating the OpRecorder and its infrastructure. Phase 0+1 at 5-8 weeks (down from 9-12 in round 2) is realistic given that:

- Phase 0 no longer requires wrapping 63 methods or backfilling field_timestamps.
- Phase 1 no longer requires a custom merge algorithm.
- The session extension handles insert/update/delete tracking automatically.

The 4-5 week savings estimate (line 892) compared to the op log approach seems reasonable. The remaining complexity is in the push/pull infrastructure, conflict handler, image sync, and snapshot/GC -- all of which were present in both approaches.

### N5. bae-server transition

The roadmap (lines 399-407) specifies that bae-server downloads `snapshot.db.enc` instead of `library.db.enc`. During the transition, the roadmap says to keep writing `library.db.enc` alongside the snapshot (line 425). This ensures backward compatibility. The bae-server code at `/Users/dima/dev/bae/bae-server/src/main.rs` line 389 hardcodes `library.db.enc` -- this needs updating in Phase 1g.

---

## Summary

The session extension redesign is a clear improvement over the op log approach. The technical claims are accurate against the codebase. The two documents are consistent. All 19 findings from rounds 1 and 2 are resolved.

The one significant new finding (pool vs. connection architecture, finding #1 above) is a real implementation concern that should be addressed in the roadmap text -- it affects Phase 1a's scope and the `Database` struct design. The session feature enablement path (finding #2) is a build configuration detail that the Phase 0c spike will resolve.

The roadmap is ready to guide implementation.
