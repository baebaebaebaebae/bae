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

## Open Questions

- What does "enable cloud" look like in the UI?
- Managed storage (we host S3) or always BYO-bucket?
- Second-device setup when iCloud Keychain is off — QR code? Paste key?
- DB sync conflict resolution
- Cover art store: SQLCipher DB or encrypted files?
- Key rotation — probably YAGNI for now
