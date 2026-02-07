# Roadmap: Library & Cloud

Product doc: `notes/library-and-cloud.md`

## Overview

Sequence of PRs to go from the current state (plain SQLite, encryption only on audio files, no cloud sync) to the vision (encrypted DB, cloud sync, multi-device, cloud bae instance).

Each PR is self-contained and shippable.

---

## PR 1: Key fingerprint

**Scope:** Small

Add SHA-256 fingerprint of the encryption key. Store it in `config.yaml`. Validate it before any decryption — wrong key is caught immediately with a clear error instead of producing garbage.

---

## PR 2: Library directory restructuring

**Scope:** Small-medium

Move from flat `~/.bae/` to `~/.bae/libraries/<library-id>/`. Foundation for multi-library and cloud sync.

- Update `Config` path logic
- Migration: detect old layout, move files to new layout on first run
- Update pointer file to reference library ID

---

## PR 3: Cover art cache (separate effort)

**Scope:** Medium

Extract cover art to `covers/` directory in library. Enables browsing library without decrypting audio files. Prerequisite for cloud cover sync.

---

## PR 4: Cloud DB sync (encrypt-on-upload)

**Scope:** Medium-large

Upload `library.db` to S3 (encrypted with library key before upload). Download and decrypt on new device setup. This is the core "enable cloud" feature.

Encrypt-the-whole-file approach (not SQLCipher) — keeps sqlx, avoids massive DB layer rewrite. Plaintext window on local disk is acceptable for desktop app.

- Add "library cloud storage" config (S3 creds for metadata sync, separate from audio storage profiles)
- Encrypt DB file before upload, decrypt on download
- Store key fingerprint alongside encrypted DB for validation
- "Enable cloud" settings flow
- "Restore from cloud" first-run flow

---

## PR 5: Startup unlock UX

**Scope:** Small-medium

When cloud-enabled and key is missing from keyring (new device, keyring wiped):

- Show unlock screen instead of empty library
- Accept recovery key paste
- Validate via fingerprint before opening DB
- Download encrypted DB from cloud, decrypt, open

---

## PR 6: Key export/import UX

**Scope:** Small

- Export key + fingerprint from settings
- Import key flow (paste, validate fingerprint)
- Show fingerprint prominently in settings for verification

---

## PR 7: Cloud bae instance (read-only)

**Scope:** Medium

- `--read-only` mode flag
- `--recovery-key` CLI arg (bypass keyring)
- Download DB from S3, open, serve subsonic API + web UI
- No writes to DB

---

## PR 8: Multi-library UI

**Scope:** Medium

- Library switcher in settings
- Create new library flow
- Per-library keyring entries
- Library list from `~/.bae/libraries/`

---

## SQLCipher: Deferred

SQLCipher would eliminate the plaintext window (DB always encrypted on disk). But it requires migrating from sqlx (async) to rusqlite (sync) — touching every DB call in the codebase. Or using the `sqlx-sqlite-cipher` fork (less maintained).

Not worth it yet. Encrypt-on-upload (PR 4) gets us to cloud with 10x less effort. Can revisit if the plaintext window becomes a real concern.
