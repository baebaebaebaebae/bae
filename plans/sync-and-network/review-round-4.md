# Sync & Network Roadmap -- Review Round 4

## Verdict: Approve

The architectural simplification is clean and well-executed. The four documents tell a consistent story: sync goes through one bucket, storage profiles are just file locations, MetadataReplicator is eliminated entirely. The old per-profile metadata replication model is gone from all four docs with no lingering references. The round 3 findings (pool/connection architecture, session extension enablement) are both addressed in the roadmap.

Two significant findings and a few minor ones, detailed below. None are blockers -- the significant items are contradictions between the docs and the existing DB schema that should be corrected before implementation begins.

---

## Verification of round 3 findings

### Finding #1 (pool vs. connection architecture): Resolved

Round 3 requested that the `Database` refactor (dedicated write connection + read pool) be explicit in the roadmap. This is now Phase 0e in the roadmap (lines 181-192):

> **0e. Database connection architecture**
>
> The SQLite session extension attaches to a single database connection and only captures changes made through that connection. The current `Database` struct uses `SqlitePool`, which dispatches writes to whichever connection is available.
>
> **Solution:** Separate the pool into a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool.

This is correctly placed before Phase 1 in the dependency graph (line 815), correctly counted in the Phase 0 summary table, and the effort estimate for Phase 0 was adjusted to 2-3 weeks (up from the round 3 estimate of 1.5-2). Well handled.

### Finding #2 (session extension enablement path): Resolved

The roadmap (lines 68-69) now spells out both the primary path and the fallback:

> The primary path is adding `libsqlite3-sys` as a direct dependency in `bae-core/Cargo.toml` with the `session` feature -- Cargo unifies features across the dependency graph.

This matches the suggestion from round 3. Phase 0c (the spike) is well-scoped to validate this.

---

## New findings

### 1. [Significant] `release_files` vs. `files`: table name mismatch in 00-data-model.md

`/Users/dima/dev/bae/notes/00-data-model.md` calls the table `release_files` throughout (lines 114, 128, 133, 152, 170, 203). The actual DB table is named `files` (`001_initial.sql` line 94), the Rust struct is `DbFile`, and the roadmap correctly refers to the `files` table (roadmap line 13, 114, 276, 282, 901).

This was identified and fixed in the roadmap during round 1 (finding R1-1), but the data model doc was not updated to match.

Six occurrences to fix in `00-data-model.md`:
- Line 114: "The `release_files` table tracks each one" -- should be `files`
- Line 128: section heading "### `release_files` -- release files" -- should be `files`
- Line 133: the schema block shows `release_files` -- should be `files`
- Line 152: "`file_id` links to the `release_files` row" -- should be `files`
- Line 170: "file_id TEXT FK -> release_files" -- should be `files`
- Line 203: "Looks up `release_files WHERE id = ?`" -- should be `files WHERE id = ?`

### 2. [Significant] "A release can be on multiple profiles" contradicts DB schema

`/Users/dima/dev/bae/notes/02-storage-profiles.md` line 7 says:

> A release's files can exist on one or more profiles, or be unmanaged.

And line 139 says:

> One row per release-profile pair, FK to both `releases` and `storage_profiles`. A release can be on multiple profiles.

But the actual `release_storage` table has `release_id TEXT NOT NULL UNIQUE` (`001_initial.sql` line 180), which enforces exactly one profile per release. The Rust method `get_release_storage` returns `Option<DbReleaseStorage>` (singular), and `delete_release_storage` deletes by `release_id` without a profile qualifier.

The docs describe a future state where the UNIQUE constraint is removed so a release can live on multiple profiles simultaneously (e.g., local + cloud). This is a reasonable design direction, but the docs present it as current fact. Either:

a. The docs should note this is a planned schema change (remove the UNIQUE constraint on `release_id`, add a composite PK or unique on `(release_id, storage_profile_id)`), or
b. The docs should reflect the current one-profile-per-release reality and add the multi-profile capability as a future change.

This matters because the transfer flow (`02-storage-profiles.md` lines 109-120) describes "moves a release from one profile to another" which is compatible with the current UNIQUE constraint, but the "one or more profiles" framing creates confusion about what the DB actually supports today.

### 3. [Minor] `is_home` column missing from storage_profiles description

`/Users/dima/dev/bae/notes/02-storage-profiles.md` line 137 describes the `storage_profiles` table schema but omits `is_home`:

> `location` is "local" or "cloud". Local profiles have `location_path`. Cloud profiles have `cloud_bucket`, `cloud_region`, `cloud_endpoint`. [...] `encrypted` flag (always false for local, always true for cloud). `is_default` marks the profile pre-selected in import.

The actual table has `is_home BOOLEAN NOT NULL DEFAULT FALSE` (`001_initial.sql` line 170), and `is_home` is used extensively in the codebase (`DbStorageProfile.is_home`, `delete_storage_profile` checks `is_home`, `get_replica_profiles` filters by `is_home = FALSE`). The doc mentions the library home is a storage profile (line 69) but doesn't mention the `is_home` column in the schema section.

### 4. [Minor] bae-server still references manifest.json in codebase

The roadmap (1g, line 442) says:

> Today `bae-server` downloads `library.db.enc` + `manifest.json.enc` + `images/` from a specific cloud profile. Under the new model, bae-server syncs from the sync bucket instead.

The current bae-server code (`/Users/dima/dev/bae/bae-server/src/main.rs`) downloads `manifest.json.enc` first (line 337), uses it to validate the key and extract boot config (lines 150-155), then downloads `library.db.enc` and images. Under the new model, bae-server gets `snapshot.db.enc` from the sync bucket and there is no `manifest.json.enc` at all.

The docs correctly describe this transition. However, the bae-server codebase also has a local-profile boot path that reads `manifest.json` as a fallback (lines 120-140). When MetadataReplicator is removed, no new `manifest.json` files will be created on profiles. The local-profile boot path (`load_boot_config`) falls back from `config.yaml` to `manifest.json` -- this fallback becomes dead code after MetadataReplicator removal.

This is not a docs issue per se, but worth noting: when implementing Phase 1g, the `manifest.json` fallback in bae-server should be removed, and the `Manifest` struct in `library_dir.rs` becomes unused (unless kept for backward compatibility with existing profile directories).

### 5. [Minor] `storage_profiles.updated_at` rename omission

The roadmap Phase 0a (line 100) says:

> Today, four tables already have `updated_at`: `artists`, `albums`, `releases`, `storage_profiles`.

And the action table (lines 106-108) renames `updated_at` to `_updated_at` on `artists`, `albums`, and `releases`. But `storage_profiles` has `updated_at` too and is listed as NOT synced (line 118). The roadmap correctly does not rename `storage_profiles.updated_at` (since it's not synced), but does not explicitly say "storage_profiles keeps its existing `updated_at` as-is." This could be confusing since the introductory sentence groups all four tables together.

A one-line clarification would help: "storage_profiles keeps its existing `updated_at` without rename (it is not synced)."

---

## Cross-reference: consistency across the four documents

### Sync bucket design

All four docs agree:
- One sync bucket per library (optional)
- Configured in `config.yaml`, not in `storage_profiles` DB table
- Contains: `snapshot.db.enc`, `changes/{device_id}/{seq}.enc`, `heads/{device_id}.json.enc`, `images/ab/cd/{id}`
- Optionally holds release files under `storage/ab/cd/{file_id}`

The bucket layout is shown identically in `00-data-model.md` (lines 72-79), `02-storage-profiles.md` (lines 47-54), and the roadmap (lines 468-477).

### Storage profiles as file-only locations

All four docs consistently describe profiles as holding only release files:
- `00-data-model.md` line 9: "no metadata replica, no DB copy, no manifest -- just encrypted files"
- `01-library-and-cloud.md` line 103: "No metadata is replicated to the storage profile -- it just holds files"
- `02-storage-profiles.md` line 3: "they hold encrypted release files and nothing else"
- Roadmap line 81: "Just files -- no DB, no images, no manifest"

No accidental mention of profiles having DB or images anywhere.

### MetadataReplicator elimination

All docs are clean:
- `00-data-model.md`: No mention of MetadataReplicator at all.
- `01-library-and-cloud.md`: No mention of MetadataReplicator at all.
- `02-storage-profiles.md`: No mention of MetadataReplicator at all.
- Roadmap line 9: "MetadataReplicator will be removed entirely." Line 424-431: "MetadataReplicator is removed entirely. It is not reduced to local-only -- it is deleted."

No lingering references to "reduced" or "local-only." Clean removal.

Note: `notes/07-sync-and-network.md` (NOT in scope for this review) still says "MetadataReplicator is reduced to local-profile-only snapshot sync (external drives)" at line 121. This should be updated separately to match the new architecture.

### `manifest.json` on profiles

All four docs are clean. No references to `manifest.json` on storage profiles. The only "manifest" reference in `00-data-model.md` is `pending_deletions.json` (line 52), which is a different concept (deferred file deletion, confirmed in codebase at `bae-core/src/storage/cleanup.rs`).

### bae-server model

`01-library-and-cloud.md` lines 80-85 describe bae-server:
> Given sync bucket URL + encryption key: downloads `snapshot.db.enc`, applies changesets, caches DB + images locally

Roadmap lines 442-452 (Phase 1g) describe the same transition.

`02-storage-profiles.md` line 107 says:
> Syncs from the sync bucket (downloads `snapshot.db.enc`, applies changesets, caches DB + images locally). Streams audio from whatever storage location files are on, decrypting on the fly. Read-only.

All three docs agree on bae-server's new model.

### Sync bucket as file storage

The dual-role concept is clearly explained:
- `00-data-model.md` line 12: "Sync bucket -- files can live alongside sync data under `storage/ab/cd/{file_id}`"
- `01-library-and-cloud.md` lines 105-107: "The sync bucket can also serve as a file storage location. Release files go under `storage/` in the same bucket."
- `02-storage-profiles.md` lines 43-56: Full subsection explaining the overlap, with the key line: "The sync bucket has a `storage_profiles` row in the DB like any other cloud profile."
- Roadmap line 81: "The sync bucket can optionally also serve as file storage (has a `storage_profiles` row for that purpose)."

The `storage_profiles` row for the sync bucket means the existing import/transfer code path works identically whether the target is the sync bucket or a separate cloud profile. This is well designed.

### Keyring entries

`00-data-model.md` lines 63-66 describe sync bucket credentials:
- `sync_s3_access_key` -- S3 access key for the sync bucket
- `sync_s3_secret_key` -- S3 secret key for the sync bucket

These are new entries that don't exist in the current `KeyService` implementation (`/Users/dima/dev/bae/bae-core/src/keys.rs`), which only has `get_profile_access_key` / `get_profile_secret_key` (per-profile). The roadmap (line 79) says "Sync bucket credentials stored in the keyring under `sync_s3_access_key` / `sync_s3_secret_key`." This is consistent -- new keyring entries for the sync bucket will need to be added to `KeyService` in Phase 1.

---

## Codebase verification

| Claim (across all four docs) | Source | Verified? |
|------|--------|-----------|
| Table is `files` (roadmap) vs. `release_files` (data model doc) | `001_initial.sql` line 94 | Table is `files`. Data model doc is wrong (Finding #1). |
| `release_storage.release_id` allows multiple profiles per release | `001_initial.sql` line 180: `release_id TEXT NOT NULL UNIQUE` | UNIQUE constraint = one profile per release. Doc is wrong (Finding #2). |
| `storage_profiles` has `is_home` column | `001_initial.sql` line 170 | Yes. Missing from `02-storage-profiles.md` (Finding #3). |
| `Database` uses `SqlitePool` | `client.rs` line 26 | Yes |
| `MetadataReplicator` pushes to all non-home profiles | `metadata_replicator.rs` line 62: `get_replica_profiles()` | Yes |
| bae-server downloads `manifest.json.enc` + `library.db.enc` | `bae-server/src/main.rs` lines 337, 389 | Yes |
| `ConfigYaml` has no `device_id` field | `config.rs` lines 52-92 | Yes |
| `ConfigYaml` has no sync bucket fields | `config.rs` lines 52-92 | Yes (to be added in Phase 1) |
| `KeyService` has no `sync_s3_access_key` methods | `keys.rs` | Yes (to be added in Phase 1) |
| Four tables have `updated_at`: artists, albums, releases, storage_profiles | `001_initial.sql` lines 10, 22, 65, 175 | Yes |
| `pending_deletions.json` exists | `bae-core/src/storage/cleanup.rs`, `library_dir.rs` | Yes |
| `Manifest` struct has `profile_id`, `profile_name`, `replicated_at` | `library_dir.rs` lines 54-61 | Yes |

---

## Notes (not issues)

### N1. Out-of-scope doc `07-sync-and-network.md` is stale

`/Users/dima/dev/bae/notes/07-sync-and-network.md` line 121 still says MetadataReplicator is "reduced to local-profile-only snapshot sync (external drives)." This contradicts the new architecture where MetadataReplicator is eliminated entirely. Not in scope for this review, but should be updated to avoid confusion.

### N2. The old `notes/storage-profiles.md` still exists

`/Users/dima/dev/bae/notes/storage-profiles.md` (without the `02-` prefix) appears to be the old version of the storage profiles doc. It contains `manifest.json` on profiles, the old per-profile metadata replication model, etc. This file should be removed or clearly marked as superseded by `02-storage-profiles.md` to prevent confusion.

### N3. `config.yaml` sync bucket fields not yet specified

The docs say sync bucket configuration lives in `config.yaml`, but the exact field names are not specified in any of the four docs. Presumably something like:

```yaml
sync_bucket: "my-music-sync"
sync_region: "us-east-1"
sync_endpoint: null  # for S3-compatible services
```

The roadmap Phase 1 should define these field names (or acknowledge they'll be designed during implementation). Not a blocker -- just a gap.

### N4. `device_id` config field well-specified

Phase 0b (roadmap lines 131-146) clearly specifies `device_id` in `config.yaml`, auto-generated on first launch, with good handling of orphaned device_ids. This is consistent with `00-data-model.md` line 55 ("sync bucket configuration, and device_id") and `01-library-and-cloud.md` line 57 ("sync bucket configuration, device_id").

### N5. The `pending_deletions.json` in the library home layout

`00-data-model.md` line 52 includes `pending_deletions.json` in the library home layout. This correctly reflects the existing codebase (used by `storage/cleanup.rs` for deferred file deletion during transfers). Good documentation of an existing feature.

---

## Summary

The four-document set is internally consistent and tells a coherent story about the architectural simplification. The separation of sync (one bucket) from file storage (profiles) is clean. MetadataReplicator elimination is thorough across all docs. The bae-server transition is well-described. Round 3 findings are fully resolved.

The two significant findings are documentation accuracy issues, not design problems:
1. Table name `release_files` in `00-data-model.md` should be `files` (matches actual DB)
2. "Multiple profiles per release" claim in `02-storage-profiles.md` contradicts the UNIQUE constraint on `release_storage.release_id`

Neither affects the sync architecture design. Both are straightforward to fix.
