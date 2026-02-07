# Library & Cloud

The user journey from local music player to cloud-synced, multi-device library.

## The Idea

bae starts simple and gets more capable as you need it to. You shouldn't have to think about encryption, keys, or cloud storage until the moment you want cloud. And when you do, encryption just happens — it's not a feature you configure, it's a consequence of going cloud.

## Tiers

### Tier 1: Local (no setup)

- Install bae, import music from folders/CDs
- Files stored locally, plain SQLite DB, no encryption, no key
- Library lives at `~/.bae/`
- Zero friction

### Tier 2: Cloud (one decision)

- User decides they want backup or multi-device access
- bae asks for S3 credentials
- bae generates an encryption key and stores it in the OS keyring
  - On macOS, iCloud Keychain syncs it to other devices automatically
- DB converts to SQLCipher (encrypted SQLite), files encrypt on upload, covers encrypt
- The only decision was "I want cloud." Encryption followed automatically.
- The user never typed an encryption key. They might not even know they have one.

### Tier 3: Power user

- Multiple storage profiles (different buckets, local + cloud mix)
  - e.g., fast S3 bucket for music you listen to often, cheap archival storage (S3 Glacier, Backblaze B2) for stuff you rarely access
- Export/import encryption key manually
- Run a read-only cloud bae instance pointing at their S3
- Key fingerprint visible in settings for verification

## One Key Per Library

The key belongs to the library, not to individual storage profiles. Everything that goes to cloud gets encrypted with it.

Why not per-profile keys:
- "Which key decrypts which album?" — confusing
- Backup becomes "which keys do I need?"
- No real security benefit
- One key, one backup. Simple.

Two libraries can share a bucket. Each has its own key, so their data is opaque to each other.

## What a Library Is

```
~/.bae/libraries/<library-id>/
  config.yaml     # settings, preferences (device-specific, not synced)
  library.db      # metadata — plain SQLite (tier 1) or SQLCipher (tier 2+)
  covers/         # cover art cache (encrypted when cloud-enabled)
```

Storage profiles define where audio files live (local directories, S3 buckets). The library directory is the metadata layer. Audio files are the bulk data.

| Data | Tier 1 (local) | Tier 2+ (cloud) |
|------|----------------|-----------------|
| library.db | Plain SQLite | SQLCipher, synced to cloud |
| Cover art | Unencrypted cache | Encrypted, synced to cloud |
| Audio files | Local directory | Encrypted on storage profile |
| Encryption key | N/A | OS keyring (iCloud Keychain) |
| config.yaml | Local | Local (device-specific) |

## Why SQLCipher

SQLCipher encrypts the SQLite DB at the page level. Compared to encrypting/decrypting the whole `.db` file:

- **Always encrypted on disk.** No plaintext window — a crash can't leak an unencrypted DB.
- **Cloud sync is trivial.** The file is already encrypted. Just upload it.
- **Transparent.** Provide the key when opening, then reads/writes work normally. Minimal performance overhead (AES-NI).

The tradeoff: you need the key to open the DB at all. But that's fine — the key auto-loads from keyring, so the user doesn't notice. And if the key is missing, we show an unlock screen. This is how every encrypted app works.

## Startup (Tier 2+)

1. **Normal:** Key auto-loads from keyring. DB opens. Library renders. User notices nothing.
2. **New device (iCloud Keychain):** Key syncs automatically. Same as above.
3. **New device (no iCloud Keychain):** Unlock screen. Paste recovery key. Library loads.
4. **Wrong key:** Fingerprint check catches it immediately. Clear error.
5. **No key, no recovery key:** Library is inaccessible. Same as forgetting your 1Password master password.

## Key Fingerprint

SHA-256 of the key, truncated. Stored alongside encrypted data. Lets us detect the wrong key immediately instead of silently producing garbage.

## Cloud Layout

```
s3://my-music/
  <library-id>/
    library.db      # SQLCipher
    covers/         # encrypted cover art
  audio/            # encrypted music files
```

## Cloud bae Instance

A read-only bae instance running in the cloud (web/subsonic accessible):

- Points at S3 bucket
- Pulls library.db, opens with key
- Serves web UI and subsonic API
- Streams audio, decrypting on the fly

## Single-Write Multi-Read (future)

- Desktop is the single writer — mutates DB, uploads to S3
- Cloud instances pull DB on change, serve from it
- SQLite is perfect for this: single file, writer uploads, readers swap in
- No database server needed

## Going from Local to Cloud

1. User provides S3 credentials
2. bae generates encryption key, stores in keyring
3. Existing library.db converts to SQLCipher
4. Cover art encrypts
5. Existing local audio can optionally upload, or stay local
6. New imports default to cloud storage

## What's Built (PR #107)

Cloud DB sync — unidirectional backup to S3. Local is authoritative, cloud is a backup copy. No SQLCipher yet; the DB is plain SQLite locally, encrypted as a blob for upload.

### How Upload Works

1. `VACUUM INTO` creates a point-in-time snapshot of the DB at `library.db.snapshot`. This is a SQLite feature that copies the entire database to a new file without locking or closing the connection pool — safe to run while the app is reading/writing.
2. Read the snapshot into memory, encrypt with XChaCha20-Poly1305 (`EncryptionService.encrypt()`).
3. Upload to `s3://bucket/bae/{library_id}/library.db.enc`.
4. Upload `meta.json` (unencrypted) with key fingerprint + timestamp, so we can validate the key before downloading the large DB.
5. Encrypt and upload each file in `covers/` individually.
6. Clean up the snapshot file.

### Why VACUUM INTO

Can't just `tokio::fs::read("library.db")` — SQLite may be mid-write, producing a corrupt copy. `VACUUM INTO` is atomic and doesn't interfere with the connection pool. It also compacts the DB (removes deleted pages), so the upload is smaller.

### Why Not Reuse CloudStorage Trait

`S3CloudStorage.object_key()` applies hash-based key partitioning for audio files (distributes load across S3 prefixes). Cloud sync needs exact control over S3 keys (`bae/{library_id}/library.db.enc`), so `CloudSyncService` creates its own `aws_sdk_s3::Client`.

### Credential Storage

Split between config and keyring to work before the DB exists (chicken-and-egg: need creds to download the DB on restore):

- **Non-secret** (bucket, region, endpoint): `config.yaml`
- **Secrets** (access_key, secret_key): macOS keyring via `KeyService`
- **Hint flag** (`cloud_sync_enabled: bool`): avoids keyring reads on startup

### Sync Triggers

- After `LibraryEvent::AlbumsChanged` (import, delete) with 2s debounce
- Manual "Sync Now" button in Cloud Sync settings tab
- No periodic/scheduled sync

### First-Run Restore

On first run (no `~/.bae/library` pointer file), `main.rs` detects this BEFORE `Config::load()` (which would create the pointer file). Launches a minimal Dioxus welcome screen with two choices:

- **Create new library**: writes pointer file, re-execs binary
- **Restore from cloud**: user enters library ID, S3 creds, encryption key → validates key fingerprint → downloads DB + covers → writes config + keyring + pointer file → re-execs binary

### S3 Layout

```
s3://bucket/bae/{library_id}/
  library.db.enc     # XChaCha20-Poly1305 encrypted DB
  meta.json          # unencrypted: { fingerprint, uploaded_at }
  covers/
    {release_id}.jpg  # individually encrypted
```

## What's Not Built Yet

- SQLCipher (DB is plain SQLite locally, encrypted only for upload)
- Bidirectional sync / conflict resolution
- Periodic auto-upload
- Incremental sync
- Cloud bae instance

## Open Questions

- Managed storage (we host S3) or always BYO-bucket?
- Second-device setup when iCloud Keychain is off — QR code? Paste key?
- DB sync conflict resolution
- Key rotation — probably YAGNI for now
