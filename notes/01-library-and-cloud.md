# Library & Cloud

The user journey from local music player to cloud-synced, multi-device library.

## The Idea

bae starts simple and gets more capable as you need it to. You shouldn't have to think about encryption, keys, or cloud storage until the moment you want cloud. And when you do, encryption just happens -- it's not a feature you configure, it's a consequence of going cloud.

## Tiers

### Tier 1: Local (no setup)

- Install bae, import music from folders/CDs
- Files stored locally, plain SQLite DB, no encryption, no key
- Library lives at `~/.bae/`

### Tier 2: Cloud (two decisions)

Two independent capabilities -- they can be enabled separately or together:

**Sync (multi-device):** User configures a sync bucket (S3 credentials + bucket). bae generates an encryption key and stores it in the OS keyring. The sync bucket gets changesets, snapshots, and images -- everything needed for another device to join.

**Cloud file storage:** User creates a cloud storage profile (S3 credentials + bucket). Release files can be transferred there. This is separate from sync -- it's just a place to put files. The sync bucket itself can also serve as file storage (simplest setup: one bucket for everything).

- On macOS, iCloud Keychain syncs the encryption key to other devices automatically
- Files encrypt on upload, images encrypt in the sync bucket, DB snapshots encrypted for bootstrap
- The only decisions were "I want sync" and/or "I want cloud storage." Encryption followed automatically.
- The user never typed an encryption key. They might not even know they have one.

### Tier 3: Power user

- Multiple file storage profiles (different buckets, local + cloud mix)
  - e.g., fast S3 bucket for music you listen to often, cheap archival storage (S3 Glacier, Backblaze B2) for stuff you rarely access
- Export/import encryption key manually
- Run bae-server pointing at the sync bucket
- Key fingerprint visible in settings for verification

## One Key Per Library

The key belongs to the library, not to individual storage profiles or the sync bucket. Everything that goes to cloud gets encrypted with it.

Each library owns its buckets and directories exclusively -- no sharing between libraries.

## What a Library Is

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

On first launch, bae creates the library home. The library home has a `storage_profiles` row in the DB (for file storage -- it holds release files like any other profile).

**`config.yaml`** -- device-specific settings (torrent ports, subsonic config, keyring hint flags, sync bucket configuration, device_id). Not synced. Only at the library home.

| Data | Tier 1 (local) | Tier 2+ (cloud) |
|------|----------------|-----------------|
| library.db | Plain SQLite | Plain locally, encrypted snapshot in sync bucket |
| Cover art | Plaintext | Encrypted in sync bucket |
| Release files | On their profile | Encrypted on cloud profiles |
| Encryption key | N/A | OS keyring (iCloud Keychain) |
| config.yaml | Local | Local (device-specific, not synced) |

## Key Fingerprint

SHA-256 of the key, truncated. Stored in `config.yaml`. Lets us detect the wrong key immediately instead of silently producing garbage.

## Single Writer, Multiple Readers

- Desktop is the single writer -- mutates the library home's DB, pushes changesets to the sync bucket
- bae-server and other read-only instances sync from the bucket and serve from their local cache
- Guard needed: prevent two desktops from both writing to the same library (e.g., write lock marker in the sync bucket)

## bae-server

`bae-server` -- a headless, read-only Subsonic API server.

- Given sync bucket URL + encryption key: downloads `snapshot.db.enc`, applies changesets, caches DB + images locally
- Streams audio from whatever storage location files are on, decrypting on the fly
- Optional `--web-dir` serves the bae-web frontend alongside the API
- `--recovery-key` for encrypted libraries, `--refresh` to re-pull from sync bucket
- Stateless -- no writes, no migrations, ephemeral cache rebuilt from the sync bucket

## Going from Local to Cloud

Sync and file storage are independent. Either can be enabled first.

### Enabling sync

1. User provides S3 credentials for a sync bucket (bucket must be empty)
2. bae generates encryption key if one doesn't exist, stores in keyring
3. bae pushes a full snapshot + all images to the sync bucket
4. Subsequent mutations push incremental changesets
5. Another device can now join from the sync bucket

### Adding cloud file storage

1. User creates a cloud storage profile (provides S3 credentials, bucket must be empty)
2. Release files can be transferred to the cloud profile
3. No metadata is replicated to the storage profile -- it just holds files

### Simplest setup: one bucket for everything

The sync bucket can also serve as a file storage location. Release files go under `storage/` in the same bucket alongside the sync data. One bucket, one set of credentials. For many users, this is all they need.

### Separate buckets

Power users can have the sync bucket on fast storage and file storage on cheap archival buckets. Or file storage on an external drive. The sync bucket only holds changesets, snapshots, and images -- it stays small.

## Sync

Desktop is the single writer. After mutations, it pushes changesets to the sync bucket. Other devices pull and apply. See `plans/sync-and-network/roadmap.md` for the full protocol.

### Sync Triggers

- After `LibraryEvent::AlbumsChanged` (import, delete) with debounce
- Manual "Sync Now" button in settings
- If the sync bucket is unreachable, sync is skipped and retried next time

### First-Run: New Library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`) and a profile UUID (e.g., `prof-aaa`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`, insert `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-aaa` | local | `~/.bae/libraries/lib-111/` |
4. Write `config.yaml`, write `~/.bae/active-library` -> `lib-111`
5. Re-exec binary -- desktop launches normally

The library home is now a storage profile. `storage/` is empty -- user imports their first album, files go into `storage/ab/cd/{file_id}`.

### First-Run: Restore from Sync Bucket

User picks "Restore from sync bucket" and provides an S3 bucket + creds + encryption key:

1. Download + decrypt `snapshot.db.enc` from the bucket (validates the key -- if decryption fails, wrong key)
2. Generate a new profile UUID (`prof-ccc`), create `~/.bae/libraries/{library_id}/`
3. Insert a new `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-ccc` | local | `~/.bae/libraries/{library_id}/` |
4. Write `config.yaml` (with sync bucket config), keyring entries, `~/.bae/active-library` -> `{library_id}`
5. Download images from the bucket
6. Pull and apply any changesets newer than the snapshot
7. Re-exec binary

The new library home is `prof-ccc`. Its `storage/` is empty -- release files still live on their original storage profiles. The user can stream from cloud profiles or transfer releases to `prof-ccc`.

## What's Not Built Yet

- Bidirectional sync / conflict resolution (Phase 1 of roadmap)
- Periodic auto-upload
- Write lock to prevent two desktops writing to the same library

## Open Questions

- Managed storage (we host S3) or always BYO-bucket?
- Second-device setup when iCloud Keychain is off -- QR code? Paste key?
- Key rotation -- probably YAGNI for now
