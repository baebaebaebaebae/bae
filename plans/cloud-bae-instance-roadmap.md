# Roadmap: Cloud bae Instance

Parent roadmap: `plans/library-and-cloud-roadmap.md` (PR 7)
Product doc: `notes/library-and-cloud.md`

## Overview

Run bae as a headless server — no desktop window, serves subsonic API + web UI. Points at an S3 bucket, downloads the encrypted library, decrypts it, and serves it. Read-only for MVP.

## What already exists

- **Subsonic API** (`bae-core/src/subsonic.rs`): Full implementation with `create_router()` returning an axum `Router`. Handles streaming, album/artist browsing, encryption/decryption. Just needs `SharedLibraryManager` + optional `EncryptionService`.
- **bae-ui**: Pure component library, already compiles to wasm32. No platform dependencies.
- **bae-mocks**: Web-only Dioxus app serving bae-ui components. Pattern for web UI serving.
- **CloudSyncService** (`bae-core/src/cloud_sync.rs`): Download DB + covers from S3.
- **Config**: Env var loading already works (dev mode), no config.yaml needed for headless.

## What's missing

- No CLI argument parsing (no clap)
- No headless binary — desktop is the only binary target
- Subsonic binds to `127.0.0.1` only, needs `0.0.0.0` for cloud
- No web UI serving (static files + wasm)
- Desktop app creates services that a headless instance doesn't need (playback, media controls, import, torrent)

---

## PR 7a: New `bae-server` crate with CLI

**Scope:** Small-medium

New workspace member `bae-server/` — a minimal headless binary.

- Add `clap` for CLI args:
  - `--recovery-key <hex>` (bypass keyring)
  - `--library-path <path>` (direct path to library dir, no pointer file)
  - `--port <port>` (subsonic port, default 4533)
  - `--bind <addr>` (default `0.0.0.0`)
- Startup: load config → create database (read-only) → create library manager → start subsonic server
- No playback, no import, no media controls, no desktop window
- Binds to `0.0.0.0` (configurable)
- No keyring — `--recovery-key` is the only way to provide the encryption key

### Why `--recovery-key` instead of keyring

Cloud instances don't have macOS keychains. The key comes from an env var or CLI arg. The `KeyService` keyring pattern is desktop-only.

---

## PR 7b: Read-only database mode

**Scope:** Small

- Add `read_only: bool` flag to `Database` construction
- When read-only: open SQLite with `?mode=ro` or use `PRAGMA query_only = ON`
- Skip running migrations (they're write operations)
- `bae-server` always opens read-only

This prevents accidental writes and is a safety net for the cloud instance. Also enables opening a DB that another process is writing to.

---

## PR 7c: Cloud DB download on startup

**Scope:** Small

When `bae-server` starts and the local library.db doesn't exist (or `--refresh` flag):

1. Build `CloudSyncService` from config + CLI args
2. Validate key fingerprint via `meta.json`
3. Download `library.db.enc` → decrypt → write to library path
4. Download `covers/` → decrypt → write to covers dir
5. Open database and continue normal startup

Reuses `CloudSyncService.download_db()` and `download_covers()` from PR 4.

CLI args for cloud:
- `--cloud-bucket <bucket>`
- `--cloud-region <region>`
- `--cloud-endpoint <endpoint>` (optional)
- `--cloud-access-key <key>`
- `--cloud-secret-key <key>`

Or read from env vars: `BAE_CLOUD_BUCKET`, `BAE_CLOUD_REGION`, etc.

---

## PR 7d: Web UI serving

**Scope:** Medium

Serve the bae web UI alongside the subsonic API. This is the big one.

### Approach: Dioxus fullstack or static WASM bundle?

**Option A: Static WASM bundle** — Build bae-ui as a WASM app (like bae-mocks), serve the static files via `tower-http::ServeDir` from `bae-server`. The web UI makes HTTP calls to the subsonic API on the same server. Simpler but requires a separate build step for the WASM bundle.

**Option B: Dioxus fullstack** — Use Dioxus's fullstack feature to serve SSR + hydration. More integrated but adds complexity and ties the server to Dioxus's server runtime.

**Recommendation: Option A** (static bundle). It's simpler, the subsonic API already exists, and the web UI only needs to call it. The WASM bundle is a build artifact checked into the repo or built in CI.

### What the web UI needs

- Album grid / list view (already in bae-ui)
- Album detail with track list (already in bae-ui)
- Audio playback via `<audio>` element pointing at `/rest/stream?id=<track_id>` (new — desktop uses cpal, web uses HTML audio)
- Cover art served via a new `/rest/getCoverArt` endpoint (or static file serving from covers dir)
- No settings, no import, no editing — read-only

### New subsonic endpoints needed

- `/rest/getCoverArt?id=<release_id>` — serve cover art (decrypted if needed)
- `/rest/search3?query=<q>` — search (optional, nice to have)

---

## PR 7e: Web audio playback

**Scope:** Small-medium

The desktop app uses cpal for audio. The web UI needs HTML5 `<audio>` element instead.

- New `WebPlayer` component in bae-ui (platform-gated or behind a feature)
- Points `<audio src="/rest/stream?id={track_id}">` at the subsonic stream endpoint
- Play/pause/seek/volume controls
- Now playing bar adapted for web

---

## Sequence

```
7a (bae-server crate + CLI) ← foundation
  ↓
7b (read-only DB) ← safety
  ↓
7c (cloud download on startup) ← connects to S3
  ↓
7d (web UI serving) ← the main feature
  ↓
7e (web audio playback) ← makes it usable
```

7a-7c can be done quickly. 7d is the bulk of the work. 7e makes it actually useful as a music player.

## Open questions

- Docker image? Probably yes for deployment, but out of scope for initial PRs.
- Authentication for the web UI? The subsonic API has no auth currently. Need at minimum basic auth or token for a public-facing server.
- Should `bae-server` also support import (non-read-only mode)? Defer to later.
- FLAC → MP3/AAC transcoding for web? `<audio>` supports FLAC in most browsers, but transcoding would reduce bandwidth. Defer.
