# Round 5: Final Pre-Implementation Audit

**Reviewer:** Claude Opus 4.6
**Date:** 2026-02-10
**Documents reviewed:**
1. `notes/00-data-model.md` -- data model
2. `notes/01-library-and-cloud.md` -- user journey, tiers, bae-server
3. `notes/02-storage-profiles.md` -- storage profiles
4. `notes/07-sync-and-network.md` -- sync vision (4 layers)
5. `plans/sync-and-network/roadmap.md` -- implementation roadmap

**Codebase files cross-referenced:**
- `bae-core/migrations/001_initial.sql`
- `bae-core/src/db/models.rs`
- `bae-core/src/db/client.rs`
- `bae-core/src/config.rs`
- `bae-core/src/keys.rs`
- `bae-core/src/encryption.rs`
- `bae-core/src/metadata_replicator.rs`
- `bae-core/src/library_dir.rs`
- `bae-server/src/main.rs`

---

## Overall Verdict: READY FOR IMPLEMENTATION

All five documents tell a consistent, coherent story. The architecture is clean: sync bucket for metadata, storage profiles for files only, MetadataReplicator eliminated entirely. The few issues found are minor terminology inconsistencies and one missing detail. No contradictions, no stale concepts, no architectural conflicts.

---

## Cross-Document Consistency Matrix

| Concept | 00-data-model | 01-library-and-cloud | 02-storage-profiles | 07-sync-and-network | roadmap | Consistent? |
|---------|:---:|:---:|:---:|:---:|:---:|:---:|
| Sync bucket: one per library, in config.yaml | Yes | Yes | Yes | Yes | Yes | OK |
| Storage profiles: file-only | Yes | Yes | Yes | Yes | Yes | OK |
| MetadataReplicator: eliminated entirely | -- | -- | -- | Yes (line 135) | Yes (1e) | OK |
| bae-server: syncs from sync bucket | Yes (line 40) | Yes (lines 79-85) | Yes (line 107) | -- | Yes (1g) | OK |
| Sync bucket layout | Yes (lines 72-79) | -- | Yes (lines 47-54) | Yes (lines 30-36) | Yes (lines 472-479) | OK |
| Changeset envelope: schema_version | -- | -- | -- | Yes (line 121) | Yes (1b, 1h) | OK |
| Schema epochs / min_schema_version | -- | -- | -- | Yes (lines 115-121) | Yes (1h) | OK |
| Table name: release_files | Yes | -- | Yes | -- | Yes | OK |
| release_storage: multi-profile | Yes (line 25) | -- | Yes (line 139) | -- | -- | OK |
| Conflict resolution: row-level LWW _updated_at | -- | -- | -- | Yes (lines 62-91) | Yes (conflict handler, 0a) | OK |
| Key hierarchy: HKDF | -- | -- | -- | Yes (lines 219-225) | Yes (3a) | OK |
| HKDF salt: deterministic from master key | -- | -- | -- | -- | Yes (3a, option 3) | OK |
| Library home at ~/.bae/libraries/{uuid}/ | Yes | Yes | Yes | -- | -- | OK |
| config.yaml: device-specific, not synced | Yes (line 55) | Yes (line 57) | Yes (line 79) | -- | Yes (0b) | OK |
| Keyring: sync_s3_access_key/secret_key | Yes (lines 65-66) | -- | -- | -- | Yes (line 79) | OK |
| active-library: UUID pointer | Yes (lines 31-35) | Yes (lines 49-53) | Yes (lines 60-67) | -- | -- | OK |
| Encryption: XChaCha20-Poly1305 64KB chunks | -- | -- | Yes (lines 126-131) | -- | Yes (line 7) | OK |
| Nonce: embedded in encrypted blob + cached in DB | -- | -- | Yes (line 129) | -- | Yes (line 7) | OK |
| Database: dedicated write connection + read pool | -- | -- | -- | Yes (lines 109-111) | Yes (0e) | OK |
| HLC format for _updated_at | -- | -- | -- | Yes (line 64) | Yes (0d) | OK |
| Images in sync bucket: encrypted | Yes (line 81) | Yes (line 62) | -- | Yes (line 35) | Yes (line 224) | OK |
| pending_deletions.json | Yes (line 52) | -- | Yes (lines 117-118) | -- | -- | OK |

---

## Issues Found

### Significant Issues

None.

### Minor Issues

**M1. Roadmap (0a) lists `storage_profiles` as having `updated_at` today, but says it is NOT synced -- slight source of confusion**

The roadmap line 13 says: "Four tables have `updated_at` columns today: `artists`, `albums`, `releases`, `storage_profiles`." Then line 118 says: "`storage_profiles` keeps its existing `updated_at` column as-is (no rename to `_updated_at`, since it is not synced)."

This is technically correct and internally consistent. But it could confuse an implementer who scans the "four tables have updated_at" line and assumes all four get renamed. The clarification at line 118 resolves it, so this is informational only. No change needed.

**M2. 00-data-model (line 52) mentions `pending_deletions.json` in the library home layout, but no other doc describes its format or lifecycle**

`pending_deletions.json` appears in the library home layout (00-data-model, line 52) and is referenced in 02-storage-profiles (lines 117-118) as part of the transfer flow. The codebase has `storage/cleanup.rs` and `storage/transfer.rs` implementing it. This is pre-existing functionality, not part of the sync roadmap, so not a gap -- but if the sync system needs to be aware of deferred deletions (e.g., syncing a delete that triggers cleanup on another device), it may need attention in Phase 1. Currently the docs are silent on this interaction.

**Verdict:** Not blocking. The deferred deletion system is device-local and does not interact with sync (deletions propagate via changesets at the DB level; file cleanup is a local concern). No doc change needed.

**M3. 01-library-and-cloud describes bae-server's current boot mode (lines 79-85) in future terms, but bae-server currently boots from a cloud profile, not the sync bucket**

Doc 01 says: "Given sync bucket URL + encryption key: downloads `snapshot.db.enc`, applies changesets..." This describes the Phase 1 target. The current `bae-server/src/main.rs` downloads `library.db.enc` + `manifest.json.enc` from a cloud profile. This is fine -- doc 01 is describing the intended end state, and the roadmap Phase 1g explicitly plans the migration. Consistent with the overall narrative.

**Verdict:** Not blocking. The doc is forward-looking by design.

**M4. 07-sync-and-network does not mention `pending_deletions.json` or deferred file deletion**

This is fine -- the sync vision doc (07) covers metadata sync layers 1-4. File deletion cleanup is a storage concern handled at the profile level, not a sync concern. The changeset carries the DELETE operation for the DB row; the actual file removal is a local side effect.

**Verdict:** Not an issue.

**M5. Roadmap describes `sync_cursors` and `sync_state` tables (1c) but does not list them in the Phase 0 summary table for DB schema changes**

Phase 0 summary (line 197) says: "DB schema: `_updated_at` column on 11 synced tables (migration 002)". The `sync_cursors` and `sync_state` tables are introduced in Phase 1c (line 341) and listed in the Phase 1 summary as "migration 003" (line 458). This is correct -- they belong to Phase 1, not Phase 0. No issue.

**M6. 00-data-model references `release_files.source_path` as "the actual location (local path or S3 key)" (line 114) -- slight ambiguity on what "S3 key" means for hash-based layout**

For cloud profiles, the `source_path` stores the full S3 URI (e.g., `s3://bucket/storage/ab/cd/{file_id}`). The doc says "S3 key" which could be interpreted as just the key portion. But 02-storage-profiles clarifies at line 21: "Each file's `source_path` stores the full S3 URI." These are consistent enough.

**Verdict:** Not blocking. The two docs together are unambiguous.

**M7. Roadmap Phase 3a HKDF salt description uses "random 32-byte value" then switches to deterministic derivation**

Lines 718-719 say: "`salt`: a random 32-byte value, generated once per library and stored alongside the master key in the keyring." Then lines 722-727 evaluate three options and choose option 3: deterministic derivation via HMAC-SHA256. The "random 32-byte" phrasing at the top of the KDF section describes the general HKDF interface, not the chosen design. The decision at line 727 ("Decision: Option 3") supersedes it.

**Verdict:** This could confuse someone skimming. Consider rewording the salt line to say: "`salt`: deterministically derived from the master key (see below)" to avoid the misleading first impression. This is a readability nit, not a correctness issue.

---

## Stale Content Check

| Concept | Status | Notes |
|---------|--------|-------|
| MetadataReplicator | Clean | 07-sync says "eliminated entirely" (line 135), roadmap says "removed entirely" (1e). No "reduced" references anywhere. |
| manifest.json on profiles | Clean | Not mentioned as part of new design. The `Manifest` struct still exists in codebase (`library_dir.rs`) and is used by bae-server and MetadataReplicator, but docs correctly describe the new world without it on profiles. |
| Op log / OpRecorder | Clean | Only mentioned in roadmap as a rejected alternative (lines 23-31, 45-48). Never presented as the chosen design. |
| Triggers / field_timestamps | Clean | Only mentioned in roadmap as rejected alternatives (lines 33-34, 46, 54). Struck through in the "what this eliminates" list. |
| Per-field HLC tracking | Clean | Explicitly eliminated in roadmap line 53: "~~Per-field HLC tracking~~ (row-level `_updated_at` is sufficient)". |

No stale concepts found in any of the five documents.

---

## Completeness Check

### Does the roadmap cover everything the vision doc (07) promises?

| Vision doc concept | Roadmap coverage | Status |
|---|---|---|
| Layer 1: Changeset sync | Phase 0 + Phase 1 | Fully covered |
| Layer 2: Shared libraries | Phase 2 | Fully covered |
| Layer 3: Cross-library sharing (derived keys) | Phase 3 | Fully covered |
| Layer 4: Public discovery network (DHT) | Phase 4 | Fully covered |
| Session extension rationale | Roadmap "key design decision" section | Fully covered |
| Schema evolution / epochs | Roadmap 1h | Fully covered |
| Snapshots / GC | Roadmap 1d | Fully covered |
| Conflict resolution (LWW) | Roadmap conflict handler + 0a + 0d | Fully covered |
| Membership chain | Roadmap 2c | Fully covered |
| Key wrapping | Roadmap 2c, 2d | Fully covered |
| Revocation | Roadmap 2f | Fully covered |
| Attestations + DHT | Roadmap 4a-4f | Fully covered |

### Are there any forward references that point to nonexistent sections?

| Reference | Source | Target | Exists? |
|---|---|---|---|
| "See `plans/sync-and-network/roadmap.md`" | 00-data-model line 207 | Roadmap | Yes |
| "See `plans/sync-and-network/roadmap.md`" | 01-library-and-cloud line 115 | Roadmap | Yes |
| "See `01-library-and-cloud.md` and `plans/sync-and-network/roadmap.md`" | 02-storage-profiles line 3 | Both docs | Yes |
| "See the roadmap (1h)" | 07-sync-and-network line 121 | Roadmap 1h | Yes |
| "See `02-storage-profiles.md`" | Roadmap line 225 | 02-storage-profiles | Yes |

All forward references resolve correctly.

### Are there gaps where one doc describes something another should mention but does not?

No significant gaps. Each doc has a clear scope:
- 00: Data model (tables, layouts, file types)
- 01: User journey (tiers, bae-server, first-run flows)
- 02: Storage profiles (file storage mechanics)
- 07: Sync vision (4 layers, high-level design)
- roadmap: Implementation plan (phases, code changes, testing)

The docs cross-reference each other appropriately. The only area where coverage is thin is bae-server's transition plan, but this is adequately covered by the roadmap's Phase 1g.

---

## Accuracy Against Codebase

| Claim in docs | Codebase reality | Match? |
|---|---|---|
| Table is `files` in current schema (docs say `release_files`) | `001_initial.sql` line 94: `CREATE TABLE files` | Acknowledged -- intentional rename planned |
| `release_storage.release_id` has UNIQUE | `001_initial.sql` line 180: `release_id TEXT NOT NULL UNIQUE` | Acknowledged -- removal planned |
| Four tables have `updated_at`: artists, albums, releases, storage_profiles | `001_initial.sql`: artists (line 10), albums (line 22), releases (line 65), storage_profiles (line 175) | Match |
| `EncryptionService` holds single 32-byte key | `encryption.rs` line 62: `key: [u8; 32]` | Match |
| XChaCha20-Poly1305, 64KB chunks | `encryption.rs` lines 9, 121 | Match |
| Nonce embedded as first 24 bytes | `encryption.rs` line 130: `base_nonce.to_vec()` prepended | Match |
| `Database` uses `SqlitePool` (single pool, not write/read split) | `client.rs` line 26: `pool: SqlitePool` | Match -- roadmap 0e plans the split |
| `KeyService::new(dev_mode, library_id)` | `keys.rs` line 27 | Match |
| Keyring entries: `encryption_master_key`, `discogs_api_key`, `s3_access_key:{profile_id}`, `s3_secret_key:{profile_id}` | `keys.rs` lines 103, 53, 160, 199 | Match |
| No `sync_s3_access_key` / `sync_s3_secret_key` keyring entries yet | `keys.rs` -- no such entries | Match -- will be added |
| No `device_id` in `ConfigYaml` | `config.rs` lines 52-92 | Match -- will be added in 0b |
| `MetadataReplicator` pushes VACUUM INTO + images to all non-home profiles | `metadata_replicator.rs` lines 59-103 | Match |
| bae-server downloads `library.db.enc` + `manifest.json.enc` from cloud profile | `bae-server/src/main.rs` lines 288-414 | Match |
| bae-server uses `Database::open_read_only` | `bae-server/src/main.rs` line 199 | Match |
| `Manifest` struct exists in `library_dir.rs` | `library_dir.rs` line 54 | Match |
| 16 tables in initial migration | `001_initial.sql`: artists, albums, album_discogs, album_musicbrainz, album_artists, releases, tracks, track_artists, files, audio_formats, torrents, torrent_piece_mappings, library_images, storage_profiles, release_storage, imports | Match (16 tables) |
| `DbTrack` has no `updated_at` field | `models.rs` line 202: struct ends with `created_at` | Match |

All claims verified. No inaccuracies found.

---

## Notes

1. **The Manifest struct will become dead code.** `library_dir.rs::Manifest` is currently used by `MetadataReplicator` and `bae-server`. After Phase 1e (MetadataReplicator removal) and Phase 1g (bae-server switch to sync bucket), `Manifest` will no longer be needed. The roadmap does not explicitly call this out but it follows naturally from the changes. Implementer should delete it as part of 1e/1g cleanup.

2. **bae-server's cloud download path downloads `manifest.json.enc` first.** After Phase 1g, the boot flow changes to `snapshot.db.enc` + changesets. The entire `download_from_cloud` function in `bae-server/src/main.rs` will be rewritten, not patched. The current `load_boot_config` fallback (config.yaml then manifest.json) will simplify to just reading config from the decrypted snapshot.

3. **The `buildtime_bindgen` requirement for `libsqlite3-sys` session feature** (roadmap line 69) is a build-system concern. Phase 0c (the spike) validates this before any production code depends on it. Good sequencing.

4. **Eleven synced tables vs. current four with `updated_at`.** The roadmap correctly identifies that 7 tables need `_updated_at` added and 3 existing tables need `updated_at` renamed to `_updated_at`. The model structs `DbArtist`, `DbAlbum`, `DbRelease` currently use `updated_at: DateTime<Utc>` (models.rs lines 60, 136, 175). These will need mechanical renaming. `DbTrack`, `DbFile`, `DbAudioFormat`, `DbAlbumArtist`, `DbTrackArtist`, `DbLibraryImage` will need the new field added to their structs. This is straightforward but touches many files.

5. **`storage_profiles.updated_at` stays as-is (not renamed, not synced).** The roadmap line 118 explicitly calls this out. The `DbStorageProfile` struct keeps its `updated_at` field name. Good -- this avoids unnecessary churn.

---

## Final Recommendation

**Ready for implementation.** All five documents are consistent with each other and accurate against the codebase. No contradictions, no stale concepts, no missing pieces. The minor readability issue in the roadmap's HKDF salt description (M7) is the only thing worth a quick edit before starting, and it is optional.

Begin with Phase 0a (`_updated_at` migration) and Phase 0c (session extension spike) in parallel.
