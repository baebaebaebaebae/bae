# Storage Profiles

How bae manages where release files and library metadata live.

## Three storage modes

A release's files can exist on one or more profiles, or be unmanaged. Each copy is in one of these modes:

### Unmanaged (no profile)

Files stay wherever the user has them. bae records each file's location in `release_files.source_path` but doesn't copy, move, or touch anything. Deleting a release from the library removes DB records but leaves files on disk.

Modeled as: no `release_storage` row for the release. `storage_profile_id` is `None` during import.

### Local profile

bae copies files into the profile's directory. Each file's `source_path` points to its copy. The original files are untouched — the user can delete them after import. Not encrypted — the UI does not allow creating encrypted local profiles.

### Cloud profile

bae encrypts and uploads files to S3. Each file's `source_path` stores the full S3 URI. Always encrypted.

Encryption is per-file using XChaCha20-Poly1305 (libsodium `crypto_secretstream`), chunked at 64KB for random-access decryption. The nonce is stored in `release_files.encryption_nonce`. The encryption key comes from `KeyService` (OS keyring), not from the profile — one key per library, not per profile.

## Profile layout

Each profile owns its directory or bucket exclusively — no sharing between libraries. The "must be empty on creation" constraint enforces this.

Every profile stores a full replica of the library metadata (DB + images). It may also have some or all of the library's release files.

**Local profile:**
```
{location_path}/
  manifest.json
  library.db
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

**Cloud profile:**
```
s3://{bucket}/
  manifest.json.enc
  library.db.enc
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

`manifest.json` identifies both the library and the profile. Plaintext on local profiles, encrypted on cloud profiles. Present on every profile, written during sync.

```json
{
  "library_id": "...",
  "library_name": "My Music",
  "encryption_key_fingerprint": "a1b2c3d4...",
  "profile_id": "...",
  "profile_name": "Fast SSD",
  "replicated_at": "2026-02-08T..."
}
```

Release files live under `storage/` in an opaque hash-based layout. `prefix` = first 2 chars of the file ID, `subprefix` = next 2 chars. No filenames, no extensions — original filenames and content types live in the DB. The path is deterministic from the file ID alone: `storage/{prefix}/{subprefix}/{file_id}`.

Every profile has the full metadata needed to restore a library's catalog. Release files may need to be fetched from other profiles.

## Library home

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

The library home is created on first launch. It's not special — it has a `storage_profiles` row like any other profile. Its `location_path` in the DB points to `~/.bae/libraries/{uuid}/`.

```
~/.bae/libraries/{uuid}/
  config.yaml           # device-specific settings, not replicated
  manifest.json         # library + profile identity, replicated
  library.db
  images/...
  storage/...
```

bae-server doesn't use `~/.bae/` — it points directly at any profile directory or S3 bucket and serves from it.

Two files at the profile root:
- **`manifest.json`** — identifies both the library and this specific profile. Contains `library_id`, `library_name`, `encryption_key_fingerprint`, `profile_id`, `profile_name`, `replicated_at`. The `profile_id` matches a `storage_profiles` row in the DB — this is how bae matches a directory to its DB record when paths change (different machine, different mount point). During metadata sync, desktop writes a manifest to each target profile with that profile's own `profile_id`.
- **`config.yaml`** — device-specific settings (torrent ports, subsonic config, keyring hint flags). Not replicated. Only exists at the library home.

## Metadata sync

Desktop is the single writer. It mutates the library home's DB directly. After mutations, metadata syncs to all other profiles:

1. `VACUUM INTO` creates an atomic DB snapshot
2. For each profile (except the library home):
   - Local: copy snapshot + images + manifest to `{location_path}/`
   - Cloud: encrypt and upload snapshot, images, and manifest
3. Clean up the snapshot

Sync triggers after `LibraryEvent::AlbumsChanged` with debounce. Also available as a manual "Sync Now" button.

If a profile is unreachable (external drive unmounted, S3 unavailable), sync is skipped and retried next time.

Starts with the naive-but-correct approach: full DB snapshot + all images every time. Incremental sync (only changed images) is an optimization for later.

## Readers

bae-server and other read-only instances point at a profile directory or S3 bucket and have the full metadata replica. They read `manifest.json` (decrypting if on a cloud profile) to identify the library and match the profile to a DB row. They don't need `~/.bae/` or the library home.

A local profile on an external drive works the same way. Plug it into another machine, point bae-server at it, and you have a full library.

## Default profile

`is_default` marks one profile as the default, pre-selected in import forms. Can be any profile.

## Profile lifecycle

**Create:** The target path/bucket must be empty. This prevents accidentally pointing at existing data or colliding with another library.

**Delete:** Cannot delete a profile that has releases linked to it.

## Import flow

1. Import UI shows a storage profile dropdown (populated from `get_all_storage_profiles()`).
2. User picks a profile or "None" (unmanaged).
3. `ImportService` calls `get_storage_profile(id)` to load the full profile.
4. Creates `ReleaseStorageImpl` from the profile — this handles write + encrypt.
5. For each file: `storage.write_file()` copies/uploads, creates the `DbFile` record with `source_path`.
6. Inserts `release_storage` row linking the release to the profile.
7. For unmanaged: `run_none_import()` records `DbFile` entries pointing at original locations, no copy.

## Reading files back

**Playback** (`playback/service.rs`, `playback/data_source.rs`): Creates a storage reader from the profile. `CloudStorageReader` handles S3 range requests + per-chunk decryption for streaming. `LocalFileStorage` reads from disk. Uses `SparseBuffer` for efficient seeking without downloading the whole file.

**Subsonic API** (`subsonic.rs`): Downloads the full file, decrypts if needed, serves it.

**bae-server**: Headless Subsonic server. Points at any profile, reads the metadata replica, streams audio. Read-only.

## Transfer between profiles

`TransferService` (`storage/transfer.rs`) moves a release from one profile to another:

1. Reads all files from source (decrypting if needed)
2. Writes to destination (encrypting if needed)
3. Updates DB: deletes old file records, inserts new ones, updates `release_storage`
4. Queues old files for deferred deletion via `pending_deletions.json`

Also supports "eject to folder" — copies files to a user-chosen directory, converts release to unmanaged.

Cleanup service (`storage/cleanup.rs`) processes the deferred deletion manifest with retries, handles both local and S3 deletions.

## Encryption details

Cloud profiles are always encrypted. Local profiles are not encrypted.

The encryption key is per-library, stored in OS keyring via `KeyService`. The algorithm is XChaCha20-Poly1305 via libsodium:

- 64KB plaintext chunks, each independently encrypted
- Random nonce per file, stored in `release_files.encryption_nonce`
- Random-access: can decrypt any chunk without reading the whole file
- Range decryption for cloud streaming: calculate which chunks overlap the byte range, download just those chunks, decrypt individually

Metadata replicas on cloud profiles are encrypted (DB as a blob, images individually). Metadata replicas on local profiles are plaintext.

## DB schema

Two tables:

**`storage_profiles`** — profile configuration. `location` is "local" or "cloud". Local profiles have `location_path`. Cloud profiles have `cloud_bucket`, `cloud_region`, `cloud_endpoint`, `cloud_access_key`, `cloud_secret_key`. `encrypted` flag (always false for local, always true for cloud). `is_default` marks the profile pre-selected in import.

**`release_storage`** — links a release to a profile. One row per release-profile pair, FK to both `releases` and `storage_profiles`. A release can be on multiple profiles. A release with no `release_storage` rows is unmanaged.

Rust types: `DbStorageProfile`, `DbReleaseStorage`, `StorageLocation` enum (Local, Cloud).
