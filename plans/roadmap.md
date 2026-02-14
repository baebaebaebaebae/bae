# Roadmap

From where we are now to what's in the notes.

## Current state

**Stage 1 (import & play):** Complete. Folder import, MusicBrainz/Discogs matching, CUE/FLAC splitting, local playback with queue/repeat/seek, album/artist browsing, search, cover art.

**Stage 2 (sync):** Works over S3. Push/pull with LWW conflict resolution, changeset signing, membership chain, invitations, snapshots. Desktop has sync settings UI (S3 credentials, member list, invite form, manual sync trigger) and a 30-second auto-sync loop. Consumer clouds (Google Drive, Dropbox, OneDrive, pCloud) don't exist yet -- no CloudHome trait in code, no OAuth flows.

**Stage 3 (share links):** Token generation, Subsonic streaming, bae-web share page all work. Missing: settings UI for base URL/expiry/key rotation, toast feedback on copy, menu item hidden when unconfigured.

**Stage 4 (follow):** Not built. The Subsonic server runs on localhost but has no authentication. No concept of a "followed library" in the UI.

**Stage 5 (join):** Backend complete (membership chain, invitation/revocation, key wrapping, changeset signing). UX gap: invitee manually enters 5 S3 fields instead of pasting a single invite code. No CloudHome::grant_access -- access is granted out-of-band.

---

## Phase 1: Share link settings

Finish what's 90% done. Small scope, immediate quality-of-life improvement.

### What's built

- `generate_share_token()` / `validate_share_token()` with HMAC-SHA256 signing
- "Copy Share Link" menu item in track row (calls generate, copies URL to clipboard)
- bae-web `/share/:token` page with audio playback
- Subsonic `stream` and `getCoverArt` endpoints accept `shareToken`
- `share_base_url` config field (Optional<String>)
- SubsonicState holds share signing key derived from encryption key

### What's not built

- `share_default_expiry_days` config field -- doesn't exist, tokens always generated with no expiry
- `share_signing_key_version` config field -- doesn't exist, SIGNING_INFO is hardcoded to `"bae-share-link-v1"`
- Version parameter on generate/validate -- not parameterized
- SuccessToast component -- doesn't exist (only ErrorToast)
- `show_share_link` prop on TrackRow/TrackMenu -- menu item always rendered, even when share_base_url is None
- Share link settings card in Subsonic tab -- no base URL input, no expiry dropdown, no rotate button

### Changes

- `bae-core/src/config.rs` -- add `share_default_expiry_days: Option<u32>`, `share_signing_key_version: u32`
- `bae-core/src/share_token.rs` -- version parameter on generate/validate, use in HKDF info
- `bae-core/src/subsonic.rs` -- pass signing key version through SubsonicState
- `bae-ui/src/components/success_toast.rs` -- new, mirrors ErrorToast with green styling
- `bae-ui/src/components/settings/subsonic.rs` -- share link settings card (base URL input, expiry Select, rotate button)
- `bae-desktop/src/ui/components/settings/subsonic.rs` -- wire settings to config store
- `bae-desktop/src/ui/components/album_detail/page.rs` -- use expiry + key version, show toasts
- `bae-ui/src/components/album_detail/track_row.rs` -- `show_share_link: bool` prop

---

## Phase 2: CloudHome trait

The foundation everything else builds on. Introduce the trait in code, refactor the existing S3 sync to implement it.

### What's built

- `SyncBucketClient` trait (18 methods) -- the current storage abstraction for sync
- `S3SyncBucketClient` -- implements the trait using AWS SDK S3
- `S3CloudStorage` -- separate S3 client for file uploads/downloads with range requests
- `S3Config` struct with bucket, region, access key, secret key, endpoint

### What's not built

- CloudHome trait -- doesn't exist in code, only in design notes
- `cloud_home/` module -- doesn't exist
- Unified abstraction -- SyncBucketClient and S3CloudStorage are separate, both S3-only

### The trait

```rust
trait CloudHome: Send + Sync {
    // storage -- same interface, different API underneath
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn read(&self, path: &str) -> Result<Bytes>;
    async fn read_range(&self, path: &str, start: u64, end: u64) -> Result<Bytes>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;

    // access management -- varies by backend
    async fn grant_access(&self, member_id: &str) -> Result<JoinInfo>;
    async fn revoke_access(&self, member_id: &str) -> Result<()>;
}
```

### Changes

- `bae-core/src/cloud_home/mod.rs` -- trait definition, JoinInfo enum
- `bae-core/src/cloud_home/s3.rs` -- refactor S3CloudStorage + S3SyncBucketClient into S3CloudHome implementing the trait
- `bae-core/src/sync/bucket.rs` -- rewrite SyncBucketClient to use CloudHome trait instead of direct S3 calls
- Update all call sites (SyncService, bae-server bootstrap, bae-desktop main.rs)
- Tests: existing sync tests pass unchanged (same behavior, new abstraction)

### What stays the same

The sync protocol, changeset format, membership chain, encryption -- all unchanged. This is purely a refactor of the storage layer underneath.

---

## Phase 3: Consumer cloud backends

The biggest user impact. Most people don't have S3 buckets. Consumer clouds are the primary sync path.

### What's built

Nothing. Zero consumer cloud code, zero OAuth code.

### 3a: iCloud Drive

The simplest backend and the natural default for macOS users. Register as a proper iCloud app via a ubiquity container -- Apple handles sync, change notifications, and download management. No REST API, no OAuth, no token refresh.

**iCloud container setup:**
- Register `iCloud.com.bae.bae` ubiquity container in app entitlements
- Access via `NSFileManager.url(forUbiquityContainerIdentifier:)` -- dedicated folder in iCloud Drive
- Shows up in Finder as "bae" under iCloud Drive
- `NSMetadataQuery` monitors for changes from other devices -- no polling, system notifies bae when new changesets arrive
- Apple manages download priorities and file eviction

**CloudHome implementation:**
- `bae-core/src/cloud_home/icloud.rs` -- implement CloudHome trait using filesystem operations on the container
  - `write` → `fs::write`
  - `read` → `fs::read`
  - `read_range` → `File::seek` + `read`
  - `list` → `fs::read_dir`
  - `delete` → `fs::remove_file`
  - `exists` → `Path::exists`
  - `grant_access` → trigger `NSSharingService` to share the cloud home folder with the joiner (system UI confirmation required by Apple, but bae pre-fills the recipient)
  - `revoke_access` → remove collaborator via `NSSharingService`
- Cloud home path: container root / `libraries/{library_id}/`
- Detect iCloud availability on startup via `NSFileManager.ubiquityIdentityToken`
- Settings UI: "Use iCloud Drive" toggle, shows sync status, container location
- bae-server does not support iCloud Drive -- macOS users use bae-desktop (which has the built-in Subsonic server). bae-server works with S3 and API-based consumer clouds (Google Drive, Dropbox, OneDrive, pCloud).

**Sharing for join (Stage 5):** bae triggers `NSSharingService` to share the container folder. Apple requires user confirmation (system UI), but bae initiates it and pre-fills the recipient. The invite code carries the container identifier + encryption key (wrapped to joiner's pubkey). On the joiner's machine, the shared folder appears automatically in their iCloud Drive.

**macOS-specific code:** The `NSFileManager`, `NSMetadataQuery`, and `NSSharingService` calls need Objective-C bridging (via `objc2` crate or similar). This is macOS-only code, feature-gated behind a `cfg(target_os = "macos")` flag.

### 3b: Google Drive

- `bae-core/src/cloud_home/google_drive.rs` -- implement CloudHome trait using Google Drive API
  - `write` → Files.create/update
  - `read` → Files.get with `alt=media`
  - `read_range` → Range header on Files.get
  - `list` → Files.list with query
  - `delete` → Files.delete
  - `grant_access` → Permissions.create (role=writer)
  - `revoke_access` → Permissions.delete
- OAuth 2.0 flow: open browser → redirect to localhost callback → exchange code for tokens → store refresh token in keyring
- Cloud home lives in a shared folder: `bae/{library_name}/`
- Settings UI: "Sign in with Google" button in Sync tab, shows connected account

### 3c: Dropbox

- `bae-core/src/cloud_home/dropbox.rs` -- implement CloudHome trait using Dropbox API
  - Storage ops → /files/upload, /files/download, /files/list_folder, /files/delete_v2
  - `grant_access` → /sharing/add_folder_member
  - `revoke_access` → /sharing/remove_folder_member
- OAuth 2.0 with PKCE
- Cloud home in shared folder: `/Apps/bae/{library_name}/`

### 3d: OneDrive

- `bae-core/src/cloud_home/onedrive.rs` -- implement CloudHome trait using Microsoft Graph API
- OAuth 2.0 via Microsoft identity platform
- Storage in OneDrive app folder

### 3e: pCloud

- `bae-core/src/cloud_home/pcloud.rs` -- implement CloudHome trait using pCloud API
- OAuth 2.0

### Settings UI changes

- Replace the S3-specific fields in Sync settings with a provider picker
- Each provider shows its sign-in flow or credential form
- After sign-in, show connected account + disconnect button
- iCloud Drive shown first (if available on macOS), then Google Drive, Dropbox, etc.
- S3 fields remain as the "S3-compatible" option (for more technical users)

### Order

iCloud Drive first (simplest, covers primary platform), then Google Drive (largest cross-platform user base), then Dropbox, OneDrive, pCloud.

---

## Phase 4: Simplify join

Replace the manual 5-field exchange with a single invite code.

### What's built

- Full join flow in desktop Library settings: create S3 client → accept invitation → unwrap key → bootstrap from snapshot → pull changesets → save credentials
- Invite form in Sync settings: enter pubkey + role → creates membership entry → shows bucket/region/endpoint as text
- 5-field "Join Shared" form in `bae-ui/src/components/settings/join_library.rs`: bucket, region, endpoint, access key, secret key

### What's not built

- `join_code` module -- no invite code encoding/decoding
- Single-paste join -- still 5 separate fields
- CloudHome::grant_access integration -- access granted out-of-band

### Invite code format

Base64url-encoded JSON containing everything the joiner needs to connect:

- For consumer clouds: provider type + folder/share ID (access already granted via provider sharing)
- For S3: bucket + region + endpoint + access key + secret key

The encryption key is NOT in the invite code -- it's wrapped to the joiner's pubkey via the membership chain (as today).

### Flow

1. Invitee copies their pubkey from Sync settings → sends to owner
2. Owner pastes pubkey into invite form → clicks Invite → bae calls CloudHome::grant_access, creates membership entry, generates invite code → owner sends code to invitee
3. Invitee pastes invite code → bae decodes, connects to cloud home, accepts invitation (unwraps key), bootstraps from snapshot

### Changes

- `bae-core/src/join_code.rs` -- encode/decode invite codes
- `bae-core/src/sync/invite.rs` -- call CloudHome::grant_access during invitation
- `bae-ui/src/components/settings/sync.rs` -- replace share info panel with single copyable code after invite
- `bae-ui/src/components/settings/join_library.rs` -- replace 5-field form with single "Paste invite code" input
- `bae-desktop/src/ui/components/settings/library.rs` -- decode invite code, construct CloudHome from JoinInfo, proceed with existing join flow
- `bae-desktop/src/ui/components/settings/sync.rs` -- generate invite code after successful invitation

---

## Phase 5: Follow

A friend browses your full catalog without touching your cloud home. They connect to your bae-desktop or bae-server via Subsonic.

### What's built

- Subsonic API server in bae-core (full implementation: getArtists, getAlbum, stream, getCoverArt, search3, etc.)
- bae-server runs the Subsonic API as a standalone process, reads from S3
- bae-desktop runs the Subsonic API in background on localhost (configurable port, default 4533)
- Subsonic settings tab: enable/disable, port config
- Library switcher for local libraries (settings/library.rs): discover, switch, create, rename, remove

### What's not built

- Subsonic authentication -- no username/password/token validation on the API (it's open)
- Subsonic client -- bae only *serves* Subsonic, never *consumes* another server's API
- Followed library concept -- no config, no storage, no UI
- Library switcher for remote libraries -- switcher only handles local libraries
- Data source abstraction -- library page always reads from local DB, no Subsonic API path

### 5a: Subsonic authentication

- Add username/password authentication to the Subsonic API (bae-core/src/subsonic.rs)
- bae-server: `--username` / `--password` CLI args (or env vars)
- bae-desktop: configure credentials in Subsonic settings tab
- Subsonic API validates credentials on every request (existing Subsonic auth protocol: token = md5(password + salt))

### 5b: Followed library concept

- `bae-desktop` config: list of followed servers (URL + username + stored password in keyring)
- Subsonic client in bae-core or bae-desktop: call getArtists, getAlbum, stream, getCoverArt against remote server
- Map Subsonic API responses to bae-ui display types (Album, Artist, Track)

### 5c: Library switcher

- UI shows local libraries + followed libraries in a switcher (sidebar or dropdown)
- Followed libraries tagged "Following"
- Selecting a followed library switches the data source from local DB to Subsonic API
- All write operations (import, edit, delete) hidden for followed libraries
- Playback streams through the Subsonic `stream` endpoint

### Changes

- `bae-core/src/subsonic.rs` -- add authentication middleware, credential validation
- `bae-core/src/subsonic_client.rs` -- new module, Subsonic API consumer (getAlbumList2, getArtist, getAlbum, stream, getCoverArt)
- `bae-server/src/main.rs` -- `--username`/`--password` args
- `bae-ui/src/components/settings/subsonic.rs` -- credentials config for desktop's built-in server
- `bae-ui/src/components/settings/library.rs` -- "Follow" button, server URL/credentials form
- `bae-desktop` -- store followed server configs, library switcher logic
- `bae-ui/src/components/library.rs` -- data source abstraction (local vs remote)
- Playback: route through Subsonic stream endpoint for followed libraries

---

## Order of work

Phases that don't block each other run in parallel.

```
Phase 1 (share link settings) ─────────────────────────────────
Phase 2 (CloudHome trait) ──┬── Phase 3a (iCloud Drive)
                            ├── Phase 3b (Google Drive)
                            ├── Phase 3c (Dropbox)
                            ├── Phase 3d (OneDrive)
                            ├── Phase 3e (pCloud)
                            └── Phase 4  (simplify join)
Phase 5 (follow) ──────────────────────────────────────────────
```

- Phase 1, Phase 2, and Phase 5 all start in parallel
- Everything in Phase 3 + Phase 4 depends on Phase 2 (the CloudHome trait)
- After Phase 2 completes, all Phase 3 backends + Phase 4 can run in parallel
