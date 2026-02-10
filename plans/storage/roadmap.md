# Storage Profiles Roadmap

Specs: `notes/data-model.md`, `notes/library-and-cloud.md`, `notes/storage-profiles.md`

## Architecture decisions

| # | Question | Decision |
|---|----------|----------|
| Q1 | Encryption checkbox for local profiles? | **Spec wins** — no encrypted local profiles in UI. Cloud = always encrypted, local = never encrypted. |
| Q2 | Keep CloudSyncService alongside profile replication? | **Replace cloud sync** with profile-based replication entirely. |
| Q3 | Does library home store audio? | **Spec is literal** — library home stores audio, users can create a second profile elsewhere. |
| Q4 | One S3 bucket per profile, or shared bucket with prefix? | **One bucket per profile** — no prefix, clean layout. |
| Q5 | Rename pointer file (`~/.bae/library`)? | **Rename** to `active-library` with migration. |
| Q6 | Keep `known_libraries.yaml`? | **Keep as-is.** |
| Q7 | Phase ordering? | Phase 0 → 1 → 2 → 3-4. |

## What's already aligned with specs

- DB schema — all tables match `data-model.md`
- Encryption — XChaCha20-Poly1305, 64KB chunked, random-access decrypt
- Key fingerprint — SHA-256, stored in config.yaml, validated on startup
- Library directory structure — `LibraryDir` wrapper, `~/.bae/libraries/<id>/` layout with pointer file
- Library images — `library_images` table, covers + artist images lifecycle
- Cloud sync — encrypted DB snapshot + covers + artists to S3, download + decrypt on restore
- First-run flow — detects missing pointer, create-new or restore-from-cloud
- Unlock screen — missing keyring key detection, recovery key paste, fingerprint validation
- Storage profiles — full CRUD, local + cloud, import with profile selection
- Storage read/write — `ReleaseStorageImpl`, `S3CloudStorage`, encrypt-if-needed
- Transfer service — move between profiles, eject to folder, deferred cleanup
- bae-server — headless binary with clap CLI, read-only DB, cloud download, subsonic
- Subsonic API — full implementation with streaming, browsing, cover art
- Image server — HMAC-signed URLs, covers/artists/files/local routes
- ContentType enum — MIME-based with helpers
- Multi-library — discover, add, remove, rename, switcher UI
- bae-web — Dioxus web app, compiles to wasm

## Phase 0 — Bug fixes [DONE]

All shipped:
- ~~0.1: PRAGMA foreign_keys never enabled~~ — closed, sqlx already enables it (PR #146 closed, PR #150 adds doc comment)
- ~~0.2: Profile deletion orphans releases~~ — PR #148 (guard + UI error display)
- ~~0.3: Multiple default profiles possible~~ — PR #145
- 0.4: S3 credentials stored in plaintext DB — deferred (moot if SQLCipher comes)
- ~~0.5: Encryption label says "AES-256"~~ — PR #144
- 0.6: Transfer loads all files into memory — deferred (correct but memory-hungry)
- ~~0.7: Release deletion doesn't use deferred cleanup~~ — PR #147
- ~~0.8: Cloud profiles allow disabling encryption~~ — PR #149

## Phase 1 — Storage file layout migration

Current layout: `{location_path}/{release_id}/{original_filename}`
Spec layout: `{location_path}/{ab}/{cd}/{file_id}` (hash-based, no filenames)

Same change needed for cloud: `s3://bucket/{ab}/{cd}/{file_id}` instead of current `s3://bucket/files/{release_id}/{filename}`

Touches: `ReleaseStorageImpl`, `S3CloudStorage::object_key()`, `source_path` values in DB

## Phase 2 — Metadata replication to profiles

The big one. Spec says every profile carries its own `library.db`, `covers/`, `artists/`, `manifest.json`.

Currently: NO metadata replication to profiles. No `manifest.json` anywhere. Cloud sync is a separate library-level backup, not per-profile.

Needs:
- `manifest.json` struct (encryption fingerprint, library_id, created_at, last_synced_at)
- Metadata sync engine — subscribe to changes, VACUUM INTO, replicate to each profile
- Library home as a storage profile row
- Remove `CloudSyncService` (replaced by profile-based replication per Q2)

## Phase 3 — Reader instances

Mostly free after Phase 2 — bae-server already accepts `--library-path`.

Needs:
- Fall back to `manifest.json` when `config.yaml` absent
- Verify subsonic paths work with profile directories
- Cloud profile download support

## Phase 4 — Layout polish

- Pointer file rename (`library` → `active-library`) with migration (per Q5)
- `manifest.json` in library home

## Phase 5 — Explicitly deferred

- SQLCipher (DB encrypted at rest)
- Bidirectional sync / conflict resolution
- Periodic auto-upload
- Incremental sync
- Key export/import UX
- Web audio playback
