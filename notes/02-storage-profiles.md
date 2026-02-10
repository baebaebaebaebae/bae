# Storage Profiles

How bae manages where release files live. Storage profiles are file storage locations -- they hold encrypted release files and nothing else. Metadata sync is handled separately by the sync bucket (see `01-library-and-cloud.md` and `plans/sync-and-network/roadmap.md`).

## Storage modes

A release's files can exist on one or more profiles, or be unmanaged. Each copy is in one of these modes:

### Unmanaged (no profile)

Files stay wherever the user has them. bae records each file's location in `release_files.source_path` but doesn't copy, move, or touch anything. Deleting a release from the library removes DB records but leaves files on disk.

Modeled as: no `release_storage` row for the release. `storage_profile_id` is `None` during import.

### Local profile

bae copies files into the profile's directory. Each file's `source_path` points to its copy. The original files are untouched -- the user can delete them after import. Not encrypted -- the UI does not allow creating encrypted local profiles.

### Cloud profile

bae encrypts and uploads files to S3. Each file's `source_path` stores the full S3 URI. Always encrypted.

Encryption is per-file using XChaCha20-Poly1305 (libsodium `crypto_secretstream`), chunked at 64KB for random-access decryption. The nonce is stored in `release_files.encryption_nonce`. The encryption key comes from `KeyService` (OS keyring), not from the profile -- one key per library, not per profile.

## Profile layout

Storage profiles hold release files only. No DB, no images, no manifest. Each profile owns its directory or bucket exclusively -- no sharing between libraries.

**Local profile:**
```
{location_path}/
  storage/ab/cd/{file_id}
```

**Cloud profile:**
```
s3://{bucket}/
  storage/ab/cd/{file_id}
```

Release files live under `storage/` in an opaque hash-based layout. `prefix` = first 2 chars of the file ID, `subprefix` = next 2 chars. No filenames, no extensions -- original filenames and content types live in the DB. The path is deterministic from the file ID alone: `storage/{prefix}/{subprefix}/{file_id}`.

### The sync bucket as file storage

The library's sync bucket (if configured) can also serve as a file storage location. Release files go under `storage/` alongside the sync data:

```
s3://sync-bucket/
  snapshot.db.enc                    # sync data
  changes/...                        # sync data
  heads/...                          # sync data
  images/...                         # sync data
  storage/ab/cd/{file_id}            # release files
```

This is the simplest setup: one bucket for everything. The sync bucket has a `storage_profiles` row in the DB like any other cloud profile, so the import and transfer flows work identically.

## Library home

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

The library home is a storage profile -- it has a `storage_profiles` row in the DB with `location_path` pointing to `~/.bae/libraries/{uuid}/`. It's created on first launch along with the library.

```
~/.bae/libraries/{uuid}/
  config.yaml           # device-specific settings, not synced
  library.db
  images/...
  storage/...
```

**`config.yaml`** -- device-specific settings (torrent ports, subsonic config, keyring hint flags, sync bucket configuration, device_id). Not synced. Only exists at the library home.

## Default profile

`is_default` marks one profile as the default, pre-selected in import forms. Can be any profile.

## Profile lifecycle

**Create:** For cloud profiles, the target bucket should be empty (or contain only sync data if it's the sync bucket). This prevents accidentally pointing at existing data or colliding with another library. Local profiles point at a directory.

**Delete:** Cannot delete a profile that has releases linked to it.

## Import flow

1. Import UI shows a storage profile dropdown (populated from `get_all_storage_profiles()`).
2. User picks a profile or "None" (unmanaged).
3. `ImportService` calls `get_storage_profile(id)` to load the full profile.
4. Creates `ReleaseStorageImpl` from the profile -- this handles write + encrypt.
5. For each file: `storage.write_file()` copies/uploads, creates the `DbFile` record with `source_path`.
6. Inserts `release_storage` row linking the release to the profile.
7. For unmanaged: `run_none_import()` records `DbFile` entries pointing at original locations, no copy.

## Reading files back

**Playback** (`playback/service.rs`, `playback/data_source.rs`): Creates a storage reader from the profile. `CloudStorageReader` handles S3 range requests + per-chunk decryption for streaming. `LocalFileStorage` reads from disk. Uses `SparseBuffer` for efficient seeking without downloading the whole file.

**Subsonic API** (`subsonic.rs`): Downloads the full file, decrypts if needed, serves it.

**bae-server**: Syncs from the sync bucket (downloads `snapshot.db.enc`, applies changesets, caches DB + images locally). Streams audio from whatever storage location files are on, decrypting on the fly. Read-only.

## Transfer between profiles

`TransferService` (`storage/transfer.rs`) moves a release from one profile to another:

1. Reads all files from source (decrypting if needed)
2. Writes to destination (encrypting if needed)
3. Updates DB: deletes old file records, inserts new ones, updates `release_storage`
4. Queues old files for deferred deletion via `pending_deletions.json`

Also supports "eject to folder" -- copies files to a user-chosen directory, converts release to unmanaged.

Cleanup service (`storage/cleanup.rs`) processes the deferred deletion manifest with retries, handles both local and S3 deletions.

## Encryption details

Cloud profiles are always encrypted. Local profiles are not encrypted.

The encryption key is per-library, stored in OS keyring via `KeyService`. The algorithm is XChaCha20-Poly1305 via libsodium:

- 64KB plaintext chunks, each independently encrypted
- Random nonce per file, stored in `release_files.encryption_nonce`
- Random-access: can decrypt any chunk without reading the whole file
- Range decryption for cloud streaming: calculate which chunks overlap the byte range, download just those chunks, decrypt individually

## DB schema

Two tables:

**`storage_profiles`** -- profile configuration. `location` is "local" or "cloud". Local profiles have `location_path`. Cloud profiles have `cloud_bucket`, `cloud_region`, `cloud_endpoint`. S3 credentials (access key, secret key) are stored in the OS keyring per profile ID, not in the DB -- see `00-data-model.md` keyring section. `encrypted` flag (always false for local, always true for cloud). `is_default` marks the profile pre-selected in import. `is_home` marks the library home profile (cannot be deleted, created on first launch).

**`release_storage`** -- links a release to a profile. One row per release-profile pair, FK to both `releases` and `storage_profiles`. A release can be on multiple profiles (e.g., local + cloud, or sync bucket + archival). A release with no `release_storage` rows is unmanaged. Note: the current DB schema (`001_initial.sql`) has `release_id TEXT NOT NULL UNIQUE`, enforcing one profile per release. This UNIQUE constraint will be removed and replaced with a UNIQUE on `(release_id, storage_profile_id)` to support multi-profile storage.

Rust types: `DbStorageProfile`, `DbReleaseStorage`, `StorageLocation` enum (Local, Cloud).
