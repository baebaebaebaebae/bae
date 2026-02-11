# Desktop Integration Roadmap: Sync, Shared Libraries, Share Grants

Everything in bae-core for these three features is implemented and tested. What's missing is the desktop wiring: triggering sync cycles, displaying status, and the UI flows for membership management and share grants.

This roadmap covers the work to bridge the existing bae-core primitives into the running desktop app. Each sub-phase is one PR.

## Inventory of what exists

### bae-core (complete, tested, merged)

**Sync engine:**
- `SyncService` -- full sync orchestrator: grab changeset, drop session, push, pull, start new session
- `SyncSession` -- wraps SQLite session extension, attaches 11 synced tables
- `pull_changes()` -- pulls and applies changesets with LWW conflict resolution and membership validation
- `Hlc` -- hybrid logical clock for `_updated_at` ordering
- `S3SyncBucketClient` -- concrete S3 implementation of `SyncBucketClient` trait
- `create_snapshot()`, `bootstrap_from_snapshot()`, `should_create_snapshot()`, `garbage_collect()`
- `build_sync_status()` -- derives `SyncStatus` from device heads
- `Database` -- dedicated write connection + read pool architecture; `writer_mutex()` exposes raw handle for session extension; `get/set_sync_cursor()`, `get/set_sync_state()`, `get_all_sync_cursors()`

**Shared libraries (membership):**
- `MembershipChain` -- append-only validated chain of Add/Remove entries, signed by Ed25519 keys
- `create_invitation()` -- wraps library key to invitee's X25519 key, uploads membership entry + wrapped key
- `accept_invitation()` -- downloads and unwraps library key from bucket
- `revoke_member()` -- Remove entry, key rotation, re-wrapping to remaining members
- `UserKeypair` -- Ed25519 signing + X25519 derivation, stored globally in keyring
- `AttributionMap` -- pubkey-to-display-name mapping (local-only, not synced)

**Share grants:**
- `create_share_grant()` -- derives per-release key, wraps payload (key + optional S3 creds) to recipient's X25519 key, signs
- `accept_share_grant()` -- verifies signature, checks expiry, decrypts payload
- `accept_and_store_grant()` -- accepts and persists to `share_grants` DB table
- `resolve_release()`, `list_shared_releases()`, `revoke_grant()` -- query local DB
- `ShareGrant` serializes to/from JSON (portable token format)

### bae-desktop (partial)

**What exists:**
- `AppServices` / `AppContext` -- DI structs. Has `library_manager`, `config` (with `device_id`), `key_service`, etc. Does NOT have: sync bucket client, sync service, HLC, membership chain.
- `AppService` -- owns `Store<AppState>`, subscribes to events, provides action methods. Has no sync-related methods or subscriptions.
- `SyncSection` (desktop bridge) -- reads `state.sync().*`, passes to `SyncSectionView`. Exists but is never populated with real data.
- `Settings` component -- matches `SettingsTab` enum exhaustively; `Sync` tab renders `SyncSection`. No membership or sharing tabs exist.

### bae-ui (partial)

**What exists:**
- `SyncState` store -- `last_sync_time`, `other_devices: Vec<DeviceActivityInfo>`, `syncing`, `error`
- `SyncSectionView` -- pure component rendering sync status (last synced, other devices, error). No "Sync Now" button yet.
- `SettingsTab` enum -- Library, Storage, Sync, Discogs, BitTorrent, Subsonic, About

**What doesn't exist:**
- Sync bucket configuration UI
- Sync trigger button
- Membership management UI components
- Share grant UI components
- Any display types for members, invitations, or share grants

### bae-server (reference implementation)

`bae-server/src/main.rs` demonstrates the full sync flow:
1. `S3SyncBucketClient::new()` with bucket/region/endpoint/access_key/secret_key/encryption
2. `bootstrap_from_snapshot()` to get initial DB
3. Open raw sqlite3, `pull_changes()` with cursors built from snapshot_seq
4. Close raw connection, open `Database::open_read_only()` for serving

This is the pattern the desktop will follow, but with a persistent session lifecycle instead of one-shot bootstrap.

---

## What's NOT in scope

- Image sync (push/pull images alongside changesets) -- TODOs in `service.rs` and `pull.rs`. Core protocol work, not integration.
- bae-web integration -- web frontend is separate.
- bae-mocks pages for new settings tabs -- mechanical follow-up after each UI phase.

---

## Phase 5: Sync Wiring

Connect the existing `SyncService` to the running desktop app.

### Phase 5a: Sync bucket configuration

**Goal:** User can configure sync bucket S3 coordinates in settings. Credentials stored in keyring.

**What changes:**

`bae-core/src/config.rs`:
- Add `sync_s3_bucket: Option<String>`, `sync_s3_region: Option<String>`, `sync_s3_endpoint: Option<String>` to `ConfigYaml` and `Config`
- Credentials (`sync_s3_access_key`, `sync_s3_secret_key`) go in keyring via `KeyService` (same pattern as storage profile credentials)
- Add `Config::sync_enabled() -> bool` convenience method (bucket + region + credentials all present)

`bae-ui/src/stores/sync.rs`:
- Add fields to `SyncState`: `sync_bucket: Option<String>`, `sync_region: Option<String>`, `sync_endpoint: Option<String>`, `sync_configured: bool`

`bae-ui/src/components/settings/sync.rs`:
- Extend `SyncSectionView` props: add sync bucket configuration fields + `on_save_config` callback + `on_test_connection` callback
- Add a "Sync Configuration" card with bucket/region/endpoint/access_key/secret_key fields and a Save button
- Show a connection test result (success/error message)

`bae-desktop/src/ui/components/settings/sync.rs`:
- `SyncSection` reads sync config from store, passes to `SyncSectionView`
- `on_save_config` handler: saves bucket/region/endpoint to Config, credentials to keyring via `KeyService`
- `on_test_connection` handler: attempts `S3SyncBucketClient::new()` (requires `EncryptionService` — sync bucket client encrypts/decrypts internally) + `list_heads()`, reports success/failure. Only available when encryption is enabled.

`bae-desktop/src/ui/app_service.rs`:
- `load_config()`: populate new sync config fields in store
- Add `save_sync_config()` and `test_sync_connection()` methods

**Dependencies:** None (first phase). Note: testing the connection requires `EncryptionService` since `S3SyncBucketClient::new()` takes it as a parameter. Users must also create the S3 bucket externally (bae does not create buckets). The "test connection" handler should surface clear error messages for bucket-not-found or access-denied errors.

**Enables:** Sync bucket is configured and persisted. Connection can be tested. No sync cycles yet.


### Phase 5b: Sync service initialization and session lifecycle

**Goal:** On app startup (if sync is configured), create the `S3SyncBucketClient`, `SyncService`, `Hlc`, start a `SyncSession` on the write connection, and store these in `AppServices`/`AppContext`.

**What changes:**

`bae-desktop/src/ui/app_context.rs`:
- Add `sync_handle: Option<SyncHandle>` to `AppServices` and `AppContext`
- `SyncHandle` is a new struct holding: `SyncService`, `S3SyncBucketClient`, `Hlc`, `UserKeypair` (if available), and a channel for triggering sync

`bae-desktop/src/main.rs`:
- After creating `database`, `key_service`, `encryption_service`, and `config`:
  - If `config.sync_enabled()` AND `encryption_service` is `Some`: create `S3SyncBucketClient` (requires `EncryptionService`), `SyncService`, `Hlc::new(config.device_id)`
  - Optionally load `UserKeypair` from keyring (needed for signed changesets)
  - Start initial `SyncSession` on the Database's write connection (via `writer_mutex()` + `lock_handle()` + `as_raw_handle()`)
  - Package into `SyncHandle`, pass into `AppServices`
  - If sync is configured but encryption is not enabled, log a warning and skip sync initialization

`bae-core` (possible small addition):
- `Database` may need a method to expose the raw `sqlite3*` pointer for session creation. Currently `writer_mutex()` gives a `Mutex<SqliteConnection>`, and the caller does `conn.lock_handle().await.as_raw_handle()`. This is workable but the pointer is only valid while the lock is held. Session lifetime management needs careful design here -- the session lives across the lock/unlock boundary because the write connection is dedicated and never returned to a pool.

**Key architectural decision:** The `SyncSession` must live as long as the app is running (between sync cycles). It cannot be behind the `Mutex` because the session is active while other code acquires the write lock to do inserts. The session is created on the raw `sqlite3*` connection, and since the write connection is dedicated (never pooled), the session remains valid even when the Mutex is unlocked. The session pointer must be stored outside the Mutex, alongside a raw `*mut sqlite3` cached at startup.

**Dependencies:** 5a

**Enables:** Sync infrastructure is initialized. Session is recording changes. No sync cycles triggered yet.


### Phase 5c: Background sync loop

**Goal:** A background task periodically runs sync cycles. Push local changes, pull remote changes, update cursors, update sync status in the store.

**What changes:**

`bae-desktop/src/ui/app_service.rs`:
- Add `subscribe_sync_events()` to `start_subscriptions()` (called on mount)
- New method: starts a background `spawn` that:
  1. Loads sync cursors from DB (`get_all_sync_cursors()`) via `library_manager.get().database()`
  2. Loads local seq from `sync_state` table
  3. Generates `timestamp` from `Hlc::now()` and a `message` string (e.g., "background sync" or a description of what triggered it)
  4. Runs `SyncService::sync(raw_handle, session, local_seq, cursors, bucket, timestamp, message, keypair, membership_chain)` -- this grabs changeset, drops session, pulls
  5. **Changeset staging**: If `outgoing` is Some, the changeset bytes are held in memory. Before pushing, these bytes should be staged (e.g., in a temp file or the `sync_state` table) so they survive a push failure. If `bucket.put_changeset()` + `bucket.put_head()` fail, the staged bytes can be retried on the next cycle instead of being lost.
  6. After successful push, persist the updated `local_seq` (from `SyncResult.outgoing.seq`) to the `sync_state` table
  7. Persists updated pull cursors to DB
  8. After pull, call `hlc.update(max_remote_timestamp)` to maintain causal ordering with remote clocks
  9. Starts a new `SyncSession`
  10. Builds `SyncStatus` from `pull_result.remote_heads` and updates `state.sync().*`
  11. After a pull that applied > 0 changesets: call `load_library()` to refresh album grid, emit `LibraryEvent::AlbumsChanged`
  12. Handles snapshot policy: if `should_create_snapshot()`, create and push
  13. Waits for next trigger (timer or event)

**Trigger mechanisms:**
- **Periodic timer:** Every 30 seconds, check if the session has changes (non-empty changeset). If so, sync.
- **On mutation:** After `LibraryEvent::AlbumsChanged`, debounce 2 seconds, then sync. This is the same event that already triggers `load_library()`.
- **Manual trigger:** A channel from UI to the sync loop (Phase 5d)

**Error handling:**
- Network errors: log, set `state.sync().error()`, retry on next timer tick
- `PullError::SchemaVersionTooOld`: set a specific error message telling user to upgrade
- Session errors: log, attempt to recreate session
- **Push failure recovery**: If push fails after session is dropped, the outgoing changeset bytes must not be lost. Stage them before pushing (see step 5 above). On next cycle, check for staged bytes and retry the push before starting a new cycle. Only after successful push (or explicit user discard) should staged bytes be cleared.

**Store updates after each sync:**
```
state.sync().last_sync_time().set(Some(now_rfc3339))
state.sync().other_devices().set(devices_from_heads)
state.sync().syncing().set(false)
state.sync().error().set(None)  // or Some(error_msg)
```

**Membership chain handling:** If the bucket has membership entries, download and validate. Pass to `pull_changes()` for changeset signature/membership validation. If no membership entries exist, pass `None` (solo/legacy library).

**Dependencies:** 5b

**Enables:** Sync is fully operational. Changes are pushed and pulled automatically. Status is visible in Settings > Sync.


### Phase 5d: Manual sync trigger, status UI, and post-pull refresh

**Goal:** User can click "Sync Now" in settings. Sync status shows last sync time, syncing state, and other devices. When incoming changesets are applied, the UI refreshes automatically.

**What changes:**

`bae-ui/src/components/settings/sync.rs`:
- Add `on_sync_now: EventHandler<()>` prop to `SyncSectionView`
- Add a "Sync Now" button (disabled when `syncing` is true or sync is not configured)
- Show `syncing` spinner when in progress

`bae-desktop/src/ui/components/settings/sync.rs`:
- Wire `on_sync_now` to send a message on the sync trigger channel (from 5c)

`bae-desktop/src/ui/app_service.rs`:
- Add `trigger_sync()` method that sends on the channel
- The background loop (5c) wakes up and runs a cycle
- After a pull that applied > 0 changesets:
  - Call `load_library()` to refresh the album grid
  - If album detail is open, call `load_album_detail()` to refresh current view
  - Emit a `LibraryEvent::AlbumsChanged` so other subscribers (playback queue, etc.) react

**Optional: Status bar indicator:**
- Small sync indicator in the title bar or app layout (e.g., a cloud icon with last-sync time)
- This is a nice-to-have; the settings page is sufficient for v1

**Dependencies:** 5c

**Enables:** Full sync UX. User sees status, can manually trigger, automatic background sync runs. Changes from other devices are visible immediately after sync.


### Phase 5 summary

| Sub-phase | PR scope | Files modified (key ones) |
|-----------|----------|--------------------------|
| 5a | Sync bucket config UI | `config.rs`, `sync.rs` (store + UI), `app_service.rs`, settings sync |
| 5b | SyncService/session init | `main.rs`, `app_context.rs`, new `SyncHandle` |
| 5c | Background sync loop | `app_service.rs` (new subscription), changeset staging |
| 5d | Manual trigger + status + post-pull refresh | `sync.rs` (UI button), `app_service.rs` |

---

## Phase 6: Shared Library UX

Wire the membership chain, invitation, and revocation flows into the desktop UI.

### Phase 6a: User keypair initialization

**Goal:** On first launch, or when user enables sharing, generate a `UserKeypair` and store it in the global keyring.

**What changes:**

`bae-desktop/src/main.rs`:
- After `KeyService` creation, call `key_service.get_or_create_user_keypair()` (already exists in bae-core)
- Store the keypair in `AppServices` for downstream use (sync signing, invitations, share grants)
- This is a prerequisite for all of Phase 6 and Phase 7

`bae-desktop/src/ui/app_context.rs`:
- Add `user_keypair: Option<UserKeypair>` to `AppServices` and `AppContext`

`bae-ui/src/stores/sync.rs` or new `identity` store:
- Add `user_pubkey: Option<String>` (hex) for display in settings

`bae-ui/src/components/settings/sync.rs`:
- Show "Your identity" section with the user's public key (truncated, with copy button)

**Dependencies:** None (can be done in parallel with Phase 5)

**Enables:** User has an identity. Changesets can be signed. Invitations can be created.


### Phase 6b: Membership display

**Goal:** Show current library members in the Sync settings tab.

**What changes:**

`bae-ui/src/stores/sync.rs`:
- Add `members: Vec<Member>` to `SyncState`
- New display type `Member { pubkey: String, display_name: String, role: MemberRole, is_self: bool }`
- `MemberRole` display enum: `Owner`, `Member` (shadows the bae-core type, which bae-ui cannot see)

`bae-ui/src/components/settings/sync.rs`:
- Add "Members" card to `SyncSectionView` showing the member list
- Each member row shows: display name (or truncated pubkey), role badge, "Remove" button (only for owners, not for self if last owner)

`bae-desktop/src/ui/components/settings/sync.rs`:
- On mount (or after sync), download membership entries from bucket, build `MembershipChain`, extract `current_members()`
- Build `AttributionMap` from chain, populate `members` in store
- Combine with user's own pubkey to set `is_self`

`bae-desktop/src/ui/app_service.rs`:
- Add `load_membership()` method: downloads entries from bucket, builds chain, updates store
- Called after sync cycle completes and on settings mount

**Dependencies:** 5b (needs bucket client), 6a (needs user pubkey for `is_self`)

**Enables:** User can see who's in their shared library.


### Phase 6c: Invite member flow

**Goal:** Owner can invite a new member by entering their public key.

**What changes:**

`bae-ui/src/components/settings/sync.rs` (or new `invite_dialog.rs`):
- "Invite Member" button in the Members card (only shown to owners)
- Opens a dialog/modal with:
  - Text input for invitee's public key (hex, paste from clipboard)
  - Role selector (Owner / Member)
  - "Invite" button
  - Status/error display

`bae-ui/src/stores/sync.rs`:
- Add `invite_status: Option<InviteStatus>` (Sending, Success, Error(String))

`bae-desktop/src/ui/components/settings/sync.rs`:
- `on_invite` handler calls `app.invite_member(pubkey, role)`

`bae-desktop/src/ui/app_service.rs`:
- Add `invite_member(pubkey_hex: &str, role: MemberRole)` method:
  1. Download membership entries from bucket. If none exist, **create the founder entry first**: a self-signed `Add` entry with `Owner` role for the current user's pubkey. Upload this to the bucket's `membership/` prefix. Build the initial `MembershipChain` from it.
  2. Get encryption key from keyring
  3. Call `create_invitation(bucket, chain, owner_keypair, invitee_pubkey, role, encryption_key, hlc.now())`
  4. Validate the chain locally BEFORE uploading the new membership entry and wrapped key to the bucket (defense against corrupt state)
  5. Update store: add new member to list, set invite status
  6. The invitee needs out-of-band: sync bucket coordinates + region + endpoint (manual for now -- display them for the owner to share)

**Out-of-band sharing (v1):**
- After successful invite, show a "Share these details with the invitee" panel containing:
  - Sync bucket name, region, endpoint
  - The invitee's own public key (for confirmation)
  - Copy-to-clipboard button
- Future: invite link / QR code

**Dependencies:** 6a, 6b, 5b (bucket)

**Enables:** Owner can invite someone. The invitee still needs to manually configure their app (Phase 6d).


### Phase 6d: Accept invitation flow

**Goal:** An invited user can join a shared library.

**What changes:**

This is a new "Join Library" flow, accessible from the library switcher or welcome screen.

`bae-ui`:
- New component: `JoinLibraryView` -- form with sync bucket coordinates (bucket, region, endpoint, access key, secret key)
- The user pastes these values (received out-of-band from the owner)

`bae-desktop`:
- New component: `JoinLibrary` wrapper
- On submit:
  1. Create `S3SyncBucketClient` with provided coordinates
  2. Call `accept_invitation(bucket, user_keypair)` to unwrap the library key
  3. Create a new library directory
  4. Save the encryption key to keyring
  5. `bootstrap_from_snapshot()` to get the initial DB
  6. Pull and apply any changesets since the snapshot
  7. Download images
  8. Write `config.yaml` with sync bucket details
  9. Switch to the new library

**Dependencies:** 6a (needs user keypair), 5a (sync bucket config UI patterns for entering S3 coordinates)

**Enables:** The full invitation round-trip works. Two users can share a library.


### Phase 6e: Remove member flow

**Goal:** Owner can remove a member from the library.

**What changes:**

`bae-ui/src/components/settings/sync.rs`:
- "Remove" button on each member row (hidden for self if last owner)
- Confirmation dialog: "Remove {name}? This will rotate the encryption key."

`bae-desktop/src/ui/app_service.rs`:
- Add `remove_member(pubkey_hex: &str)` method:
  1. Call `revoke_member(bucket, chain, owner_keypair, revokee_pubkey, hlc.now())`
  2. Receive new encryption key
  3. Persist new key to keyring
  4. Update `EncryptionService` with new key (requires `EncryptionService` to be accessible from `AppService` — pass through `SyncHandle` or `AppServices`)
  5. Reload membership in store

**Dependencies:** 6b, 6c (conceptually; needs working membership chain)

**Enables:** Full membership lifecycle: invite, view, remove.


### Phase 6f: Attribution display

**Goal:** Show who added/modified releases and albums.

**What changes:**

This is lower priority and can be done incrementally.

`bae-ui/src/display_types.rs`:
- Add `author: Option<String>` to relevant display types (Album, Release) -- the display name, not the raw pubkey

`bae-desktop/src/ui/display_types.rs`:
- When converting DB types to display types, look up the `_updated_at` column's device_id component in the `AttributionMap`

`bae-ui/src/components/album_detail/`:
- Show "Added by {name}" or "Last edited by {name}" in album metadata

**Caveat:** `_updated_at` contains the device_id, not the author pubkey. To map to a person, we need the changeset envelope's `author_pubkey` for the changeset that last touched the row. This requires either:
1. Scanning changeset envelopes (expensive, impractical for display)
2. Adding an `_author_pubkey` column to synced tables (extra column, but simple)
3. Showing device_id-based attribution (less useful but works without changes)

**Recommendation:** Start with option 3 (device-based: "Edited from device X"). If users want person-based attribution, add `_author_pubkey` column later.

**Dependencies:** 6b (needs membership data for display names)

**Enables:** Users see provenance of library data.


### Phase 6 summary

| Sub-phase | PR scope | Key files |
|-----------|----------|-----------|
| 6a | User keypair init | `main.rs`, `app_context.rs`, settings sync UI |
| 6b | Member list display | `sync.rs` (store + UI), `app_service.rs` |
| 6c | Invite member | invite dialog UI, `app_service.rs` |
| 6d | Accept invitation | new join library flow, `main.rs` |
| 6e | Remove member | remove confirmation UI, `app_service.rs` |
| 6f | Attribution display | display_types, album detail UI |

---

## Phase 7: Share Grants UX

Wire the cross-library release sharing into the desktop UI.

### Phase 7a: Create share grant

**Goal:** User can share a release with someone by generating a share grant token.

**What changes:**

`bae-ui/src/components/album_detail/`:
- New "Share" button in the release action bar (next to storage, delete, etc.)
- Opens a dialog with:
  - Text input for recipient's public key (hex)
  - Optional expiry date picker
  - "Create Share" button
  - Result: the grant JSON displayed in a copyable text area

`bae-ui/src/stores/album_detail.rs`:
- Add `share_grant_json: Option<String>`, `share_error: Option<String>`

`bae-desktop/src/ui/app_service.rs` (or album detail methods):
- Add `create_share_grant(release_id, recipient_pubkey_hex, expires)` method:
  1. Get encryption service (for `derive_release_key()`)
  2. Get release's storage profile for bucket coordinates
  3. Get S3 credentials from keyring
  4. Call `create_share_grant()` from bae-core
  5. Serialize to JSON, set in store for display

**Prerequisite:** The release must be on a cloud storage profile (the share grant needs bucket coordinates). If the release is local-only, show a message explaining they need to transfer it to cloud first.

**Dependencies:** 6a (needs user keypair for signing)

**Enables:** User can generate a portable share grant token.


### Phase 7b: Accept share grant

**Goal:** User can paste a share grant JSON token and gain access to a shared release.

**What changes:**

`bae-ui`:
- New UI for accepting a share grant. Options for where to put it:
  - A "Shared with Me" section accessible from settings or a dedicated tab
  - Or: a menu action "Import Share Grant" that opens a paste dialog
- The dialog has: a text area for pasting JSON, a "Accept" button, status/error display

`bae-ui/src/stores/`:
- New store (or extend `SyncState`): `shared_releases: Vec<SharedReleaseDisplay>`
- `SharedReleaseDisplay` display type: `{ grant_id, release_id, from_library_id, from_user_display_name, bucket, expires }`

`bae-desktop/src/ui/app_service.rs`:
- Add `accept_share_grant(json: &str)` method:
  1. Deserialize JSON to `ShareGrant`
  2. Call `accept_and_store_grant(db, &grant, &user_keypair)` from bae-core
  3. Reload shared releases list in store

**Dependencies:** 6a (needs user keypair for decryption)

**Enables:** User can receive shared releases.


### Phase 7c: View shared releases

**Goal:** User can see all releases shared with them, with status and metadata.

**What changes:**

`bae-ui`:
- New component: `SharedReleasesView` -- lists all active share grants
- Each row shows: release_id, from whom (truncated pubkey or display name), expiry, bucket
- "Revoke" button to remove a grant from local DB

`bae-desktop/src/ui/app_service.rs`:
- Add `load_shared_releases()` method:
  1. Call `list_shared_releases(db)` from bae-core
  2. Convert to display types
  3. Set in store
- Add `revoke_shared_release(grant_id)` method

**Where in the UI:** This could be:
- A new settings tab ("Sharing") -- adds to `SettingsTab` enum
- A section within the Sync settings tab
- A dedicated "Shared" view in the main navigation

**Recommendation:** Add as a section within the Sync tab for now. A dedicated tab can come later if the feature gets complex.

**Dependencies:** 7b

**Enables:** User can browse and manage their shared releases.


### Phase 7d: Playback from shared releases

**Goal:** When playing a track from a shared release, the playback system uses the share grant's bucket coordinates and decryption key.

**What changes:**

This is the most architecturally significant piece. The playback system currently assumes all files are either local or on a storage profile the user owns. Shared releases are on someone else's bucket with a per-release key.

`bae-core/src/playback/`:
- Before attempting playback of a file, check if the release has a share grant via `resolve_release(db, release_id)`
- If a `SharedRelease` is found, construct an S3 client using the grant's bucket/region/endpoint/s3_creds
- Decrypt using the grant's `release_key` (not the library's master key)
- Stream the audio through the existing playback pipeline

This requires changes to the playback service's file resolution logic, which is in bae-core, not bae-desktop. The desktop integration is just ensuring the `Database` is available to the playback service (which it already is via `LibraryManager`).

**Dependencies:** 7b, 7c

**Enables:** Full sharing workflow: share, accept, play.


### Phase 7 summary

| Sub-phase | PR scope | Key files |
|-----------|----------|-----------|
| 7a | Create share grant | album detail share dialog, `app_service.rs` |
| 7b | Accept share grant | paste dialog UI, `app_service.rs` |
| 7c | View shared releases | shared releases list UI, `app_service.rs` |
| 7d | Playback from shared | playback file resolution (bae-core) |

---

## Dependency graph

```
Phase 5a ─── 5b ─── 5c ─── 5d


Phase 6a ──────────────────────
    │
    ├── 6b ── 6c
    │    │
    │    6e
    │
    ├── 6d (also depends on 5a)
    │
    └── 6f

Phase 7a ─────────────────────
    │
    7b ── 7c ── 7d
```

Key dependencies:
- **5a -> 5b -> 5c -> 5d**: Sync config must exist before service init, service must exist before sync loop, loop must exist before manual trigger and post-pull refresh
- **6a is independent**: Can be done in parallel with Phase 5
- **6b needs 5b + 6a**: Needs bucket client and user pubkey
- **6c needs 6b**: Needs membership display. Creates founder entry if chain is empty.
- **6d needs 6a + 5a**: Joining needs keypair and reuses the S3 config UI patterns
- **7a needs 6a**: Needs keypair for signing
- **7b needs 6a**: Needs keypair for decryption
- **7d needs 7b + 7c**: Needs accepted grants and resolution logic

## Recommended implementation order

1. **6a** (user keypair) -- small, unblocks both Phase 6 and Phase 7
2. **5a** (sync config) -- enables the rest of sync
3. **5b** (sync service init)
4. **5c** (background sync loop) -- the big piece; includes changeset staging for push-failure recovery
5. **5d** (manual trigger + status + post-pull refresh) -- full sync UX in one PR
6. **6b** (member display) -- shows who's in the library
7. **6c** (invite member) -- includes founder entry bootstrap
8. **6d** (accept invitation) -- full sharing round-trip works
9. **6e** (remove member)
10. **7a** (create share grant)
11. **7b** (accept share grant)
12. **7c** (view shared releases)
13. **7d** (playback from shared)
14. **6f** (attribution) -- nice-to-have, not part of MVP, can be deferred indefinitely

---

## Key architectural notes

### Session lifetime across the write Mutex

The `SyncSession` is created on the raw `sqlite3*` pointer of the dedicated write connection. The session must remain active between sync cycles (recording all changes). But the write connection is behind a `tokio::Mutex`, and every write method in `Database` acquires the lock, does its query, and releases.

This works because:
1. The write connection is dedicated (never pooled). The `sqlite3*` pointer is stable.
2. The session extension operates at the C level on the connection, not on the Rust `SqliteConnection` wrapper. The session is active as long as the pointer is valid, regardless of whether the Rust Mutex is locked.
3. The raw pointer can be extracted once at startup and cached.

The sync loop must:
1. Lock the Mutex to grab the raw pointer (once, at startup)
2. Create the SyncSession on that pointer
3. On sync: lock the Mutex, grab changeset, drop session, do push/pull (which needs the raw pointer for apply), start new session, unlock

Actually, since `pull_changes()` takes a raw `*mut sqlite3`, and `SyncSession::start()` takes a raw `*mut sqlite3`, and both need to happen while the write connection is locked (to prevent concurrent writes during pull-apply), the sync loop should:
1. Lock the write Mutex
2. Grab changeset from current session, drop session
3. Do push (can release lock here -- push is network I/O)
4. Re-lock write Mutex
5. `pull_changes()` with the raw pointer (applies incoming changesets)
6. Start new session
7. Release lock

This means the write lock is held during changeset apply, which is correct -- no other writes should happen while we're applying incoming changes.

### Sync bucket credentials in the keyring

Following the storage profile pattern:
- `KeyService` methods: `get_sync_access_key()`, `set_sync_access_key()`, `get_sync_secret_key()`, `set_sync_secret_key()`
- Keyring entries: `bae_{library_id}_sync_s3_access_key`, `bae_{library_id}_sync_s3_secret_key`

### No new singletons

The `SyncHandle` is created in `main.rs` and passed through `AppServices`. The sync loop is spawned from `AppService::start_subscriptions()` (same pattern as playback, imports). No global state.

### Membership chain is ephemeral

The `MembershipChain` is downloaded from the bucket on each sync cycle (or on demand). It's not cached in the DB -- it's a transient in-memory structure built from the bucket's `membership/` entries. This is intentional: the chain is the bucket's source of truth, and caching it locally would create a consistency problem.

### HLC maintenance

The `Hlc` must be kept in sync with remote clocks. After pulling changesets, call `hlc.update()` with the highest remote timestamp seen. This prevents clock drift when the local clock is behind a remote peer. The `Hlc` uses interior `Mutex`, is thread-safe, and embeds the `device_id` in generated timestamps.

### Display type boundary

Per project conventions:
- `bae-ui` defines `Member`, `SharedReleaseDisplay`, and `MemberRole` (display enum) -- these are pure display types with no bae-core dependency. Names avoid the "FooInfo" anti-pattern by shadowing bae-core types (which bae-ui cannot see) or using descriptive names.
- `bae-desktop` converts `MembershipEntry` + `AttributionMap` -> `Member`, `SharedRelease` -> `SharedReleaseDisplay`
- `bae-ui` components receive only display types as props
