# Storage Profiles

How bae manages where release files and library metadata live.

## Three storage modes

Every release is in exactly one of these modes:

### Unmanaged (no profile)

Files stay wherever the user has them. bae records each file's location in `files.source_path` but doesn't copy, move, or touch anything. Deleting a release from the library removes DB records but leaves files on disk.

Modeled as: no `release_storage` row for the release. `storage_profile_id` is `None` during import.

### Local profile

bae copies files into the profile's directory. Each file's `source_path` points to its copy. The original files are untouched — the user can delete them after import. Not encrypted — the UI does not allow creating encrypted local profiles.

### Cloud profile

bae encrypts and uploads files to S3. Each file's `source_path` stores the full S3 URI. Always encrypted.

Encryption is per-file using XChaCha20-Poly1305 (libsodium `crypto_secretstream`), chunked at 64KB for random-access decryption. The nonce is stored in `files.encryption_nonce`. The encryption key comes from `KeyService` (OS keyring), not from the profile — one key per library, not per profile.

## Profile layout

Each profile owns its directory or bucket exclusively — no sharing between libraries. The "must be empty on creation" constraint enforces this.

Every profile stores two things: audio files and a replica of the library metadata.

**Local profile:**
```
{location_path}/
  manifest.json
  library.db
  covers/{release_id}
  artists/{artist_id}
  ab/cd/{file_id}
```

**Cloud profile:**
```
s3://{bucket}/
  manifest.json               # unencrypted
  library.db.enc
  covers/{release_id}         # individually encrypted
  artists/{artist_id}         # individually encrypted
  ab/cd/{file_id}             # encrypted
```

`manifest.json` identifies both the library and the profile. Always unencrypted so a reader can identify what it's looking at and validate the encryption key before downloading anything large. Present on every profile, written during sync.

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

Audio files use an opaque hash-based layout. `prefix` = first 2 chars of the file ID, `subprefix` = next 2 chars. No filenames, no extensions — original filenames and content types live in the DB. The path is deterministic from the file ID alone.

Every profile is self-contained — it has all the data needed to restore a full library.

## Library home

The library home is the first local profile, created on first launch. Desktop runs against it. It's not special — it has a `storage_profiles` row like any other profile.

The default location is `~/.bae/`.

```
~/.bae/
  active-library        # pointer to active library (absent = use ~/.bae/)
  config.yaml           # device-specific settings, not replicated
  manifest.json         # library identity, replicated to all profiles
  library.db
  covers/...
  artists/...
  ab/cd/...
```

Multiple libraries are supported but each owns its own directory. A second library would live at a completely separate path (e.g., `~/other-music/`).

Two files at the root:
- **`manifest.json`** — identifies library and profile (`library_id`, `library_name`, `encryption_key_fingerprint`, `profile_id`, `profile_name`, `replicated_at`). Replicated to every profile. Not secret.
- **`config.yaml`** — device-specific settings (torrent ports, subsonic config, keyring hint flags). Not replicated. Only exists at the library home.

## Metadata sync

Desktop is the single writer. It mutates the library home's DB directly. After mutations, metadata syncs to all other profiles:

1. `VACUUM INTO` creates an atomic DB snapshot
2. For each profile (except the library home):
   - Local: copy snapshot + covers + artists + manifest to `{location_path}/`
   - Cloud: encrypt snapshot, upload to `s3://{bucket}/library.db.enc`, encrypt and upload covers + artists, upload `manifest.json` (unencrypted)
3. Clean up the snapshot

Sync triggers after `LibraryEvent::AlbumsChanged` with debounce. Also available as a manual "Sync Now" button.

If a profile is unreachable (external drive unmounted, S3 unavailable), sync is skipped and retried next time.

Starts with the naive-but-correct approach: full DB snapshot + all covers/artists every time. Incremental sync (only changed images) is an optimization for later.

## Readers

bae-server and other read-only instances point at any profile and have everything they need — the DB replica and the audio files. They don't need to know about other profiles or the library home.

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
- Random nonce per file, stored in `files.encryption_nonce`
- Random-access: can decrypt any chunk without reading the whole file
- Range decryption for cloud streaming: calculate which chunks overlap the byte range, download just those chunks, decrypt individually

Metadata replicas on cloud profiles are encrypted (DB as a blob, covers and artists individually). Metadata replicas on local profiles are plaintext.

## DB schema

Two tables:

**`storage_profiles`** — profile configuration. `location` is "local" or "cloud". Local profiles have `location_path`. Cloud profiles have `cloud_bucket`, `cloud_region`, `cloud_endpoint`, `cloud_access_key`, `cloud_secret_key`. `encrypted` flag (always false for local, always true for cloud). `is_default` marks the profile pre-selected in import.

**`release_storage`** — links a release to its profile. One row per release, FK to both `releases` and `storage_profiles`. A release with no `release_storage` row is unmanaged.

Rust types: `DbStorageProfile`, `DbReleaseStorage`, `StorageLocation` enum (Local, Cloud).
