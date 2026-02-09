# Library & Cloud

The user journey from local music player to cloud-synced, multi-device library.

## The Idea

bae starts simple and gets more capable as you need it to. You shouldn't have to think about encryption, keys, or cloud storage until the moment you want cloud. And when you do, encryption just happens — it's not a feature you configure, it's a consequence of going cloud.

## Tiers

### Tier 1: Local (no setup)

- Install bae, import music from folders/CDs
- Files stored locally, plain SQLite DB, no encryption, no key
- Library lives at `~/.bae/`

### Tier 2: Cloud (one decision)

- User decides they want backup or multi-device access
- bae asks for S3 credentials
- bae generates an encryption key and stores it in the OS keyring
  - On macOS, iCloud Keychain syncs it to other devices automatically
- Files encrypt on upload, images encrypt, DB encrypted as a snapshot for replication
- The only decision was "I want cloud." Encryption followed automatically.
- The user never typed an encryption key. They might not even know they have one.

### Tier 3: Power user

- Multiple storage profiles (different buckets, local + cloud mix)
  - e.g., fast S3 bucket for music you listen to often, cheap archival storage (S3 Glacier, Backblaze B2) for stuff you rarely access
- Export/import encryption key manually
- Run a read-only bae-server pointing at any profile
- Key fingerprint visible in settings for verification

## One Key Per Library

The key belongs to the library, not to individual storage profiles. Everything that goes to cloud gets encrypted with it.

Each library owns its buckets and directories exclusively — no sharing between libraries.

## What a Library Is

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

On first launch, bae creates the library home as the first local storage profile. The library home is just a profile — it has a `storage_profiles` row in the DB like any other.

Two files at the root of every profile:

- **`manifest.json`** — identifies library and profile (`library_id`, `library_name`, `encryption_key_fingerprint`, `profile_id`, `profile_name`, `replicated_at`). Replicated to every profile. Always unencrypted. A reader can identify the library, validate the key, and match the directory to a DB row from this alone.
- **`config.yaml`** — device-specific settings (torrent ports, subsonic config, keyring hint flags). Not replicated. Only at the library home.

Every storage profile — local or cloud — stores a full replica of the library metadata (DB + images). Release files are separate — a release's files can be replicated to some or all profiles, and a profile may have all, some, or none of the library's release files. See `02-storage-profiles.md` for the full layout.

| Data | Tier 1 (local) | Tier 2+ (cloud) |
|------|----------------|-----------------|
| library.db | Plain SQLite | Plain locally, encrypted snapshot replicated to cloud profiles |
| Cover art | Plaintext | Encrypted on cloud profiles, replicated to all |
| Release files | On their profile | Encrypted on cloud profiles |
| Encryption key | N/A | OS keyring (iCloud Keychain) |
| config.yaml | Local | Local (device-specific, not replicated) |

## SQLCipher (future)

SQLCipher encrypts the SQLite DB at the page level. Not yet implemented — the DB is plain SQLite locally, encrypted as a blob for replication to cloud profiles. SQLCipher would mean the local DB is always encrypted at rest, and cloud replication becomes trivial (just upload the file). Worth revisiting once the core storage model is solid.

## Key Fingerprint

SHA-256 of the key, truncated. Stored in `manifest.json` (replicated to every profile). Lets us detect the wrong key immediately instead of silently producing garbage.

## Single Writer, Multiple Readers

- Desktop is the single writer — mutates the library home's DB, replicates metadata to all profiles
- bae-server and other read-only instances point at any profile and serve from its metadata replica
- Guard needed: prevent two desktops from both writing to the same library (e.g., write lock marker in the DB or replicated metadata)

## bae-server

`bae-server` — a headless, read-only Subsonic API server.

- Points at any profile (local path or S3 bucket), reads the metadata replica
- Streams audio from the same profile, decrypting on the fly
- Optional `--web-dir` serves the bae-web frontend alongside the API
- `--recovery-key` for encrypted libraries, `--refresh` to re-pull metadata
- Stateless — no writes, no migrations, just serves what's in the profile

## Going from Local to Cloud

1. User creates a cloud profile (provides S3 credentials, bucket must be empty)
2. bae generates encryption key if one doesn't exist, stores in keyring
3. Release files can be transferred to the cloud profile, or stay local
4. Metadata automatically replicates to the cloud profile
5. The cloud profile is now a full backup — DB, images, and any release files on it

Example: library home is `prof-aaa` at `~/.bae/libraries/lib-111/`. User adds a cloud profile:

1. Insert `storage_profiles` row: `prof-bbb | cloud | bucket: my-music-bucket`
2. Metadata sync triggers — desktop writes to the bucket:
   - `library.db.enc` (encrypted DB snapshot)
   - `images/` (encrypted)
   - `manifest.json` with `profile_id: "prof-bbb"` (unencrypted)
3. User can now transfer releases from `prof-aaa` to `prof-bbb`, or import new releases directly to `prof-bbb`

## Metadata Replication

Desktop is the single writer. After mutations, it replicates metadata to all other profiles. Starts with the naive-but-correct approach (full snapshot + all images every time). Optimization opportunities: incremental image sync (only new/changed), diffing, compression.

### How It Works

1. `VACUUM INTO` creates a point-in-time snapshot of the DB. This is a SQLite feature that copies the entire database to a new file without locking or closing the connection pool — safe to run while the app is reading/writing. Also compacts the DB (removes deleted pages), so replicas are smaller.
2. For each profile (except the library home):
   - **Local profile:** copy snapshot + images + manifest to `{location_path}/`
   - **Cloud profile:** encrypt snapshot, upload to `s3://{bucket}/library.db.enc`, encrypt and upload images individually, upload `manifest.json` (unencrypted)
3. Clean up the snapshot file.

### Sync Triggers

- After `LibraryEvent::AlbumsChanged` (import, delete) with debounce
- Manual "Sync Now" button in settings
- If a profile is unreachable (drive unmounted, S3 unavailable), sync is skipped and retried next time

### First-Run: New Library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`) and a profile UUID (e.g., `prof-aaa`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`, insert `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-aaa` | local | `~/.bae/libraries/lib-111/` |
4. Write `manifest.json`:
   ```json
   {
     "library_id": "lib-111",
     "library_name": "My Music",
     "encryption_key_fingerprint": null,
     "profile_id": "prof-aaa",
     "profile_name": "Local",
     "replicated_at": null
   }
   ```
5. Write `config.yaml`, write `~/.bae/active-library` → `lib-111`
6. Re-exec binary — desktop launches normally

The library home is now a storage profile. `storage/` is empty — user imports their first album, files go into `storage/ab/cd/{file_id}`.

### First-Run: Restore from Profile

User picks "Restore from profile" and provides an S3 bucket + creds + encryption key:

1. Read `manifest.json` from the bucket:
   ```json
   {
     "library_id": "lib-111",
     "encryption_key_fingerprint": "abc123",
     "profile_id": "prof-bbb",
     ...
   }
   ```
2. Validate the encryption key's fingerprint against `"abc123"` ✓
3. Download + decrypt `library.db.enc` — now we have the full DB with all `storage_profiles` rows
4. Generate a new profile UUID (`prof-ccc`), create `~/.bae/libraries/lib-111/`
5. Insert a new `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-ccc` | local | `~/.bae/libraries/lib-111/` |
6. Write `manifest.json` with `profile_id: "prof-ccc"`
7. Write `config.yaml`, keyring entries, `~/.bae/active-library` → `lib-111`
8. Download images from the bucket
9. Re-exec binary

The new library home is `prof-ccc`. Its `storage/` is empty — release files still live on the cloud profile (`prof-bbb`) and the original machine's local profile (`prof-aaa`, unreachable from here). The user can stream from the cloud or transfer releases to `prof-ccc`.

## What's Not Built Yet

- SQLCipher (DB is plain SQLite locally, encrypted only for replication)
- Bidirectional sync / conflict resolution
- Periodic auto-upload
- Incremental metadata sync (image diffing, etc.)
- Write lock to prevent two desktops writing to the same library

## Open Questions

- Managed storage (we host S3) or always BYO-bucket?
- Second-device setup when iCloud Keychain is off — QR code? Paste key?
- DB sync conflict resolution
- Key rotation — probably YAGNI for now
