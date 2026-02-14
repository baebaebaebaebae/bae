# Phase 3: Consumer cloud backends

Add CloudHome implementations for consumer cloud services. Most users don't have S3 buckets -- consumer clouds are the primary sync path.

## Depends on

Phase 2 (CloudHome trait) -- merged as PR #224. The trait is at `bae-core/src/cloud_home/mod.rs` with `S3CloudHome` in `s3.rs`. `CloudHomeSyncBucket` wraps any `dyn CloudHome` and handles path layout + encryption.

**However**, the Phase 2 implementation diverged from the design notes in several ways that must be fixed before consumer clouds can work. See "Phase 2 fixes" below.

---

## Phase 2 fixes (prerequisite)

The current trait has `join_info()` (passive getter) and `revoke_access()` (no params). The design notes specify `grant_access(member_email_or_id) -> Result<JoinInfo>` and `revoke_access(member_email_or_id) -> Result<()>`. Consumer clouds need the parameterized versions because granting access means sharing a folder with a specific user account.

### Fix 1: CloudHome trait signature -- `bae-core/src/cloud_home/mod.rs`

```rust
// Before:
fn join_info(&self) -> JoinInfo;
async fn revoke_access(&self) -> Result<(), CloudHomeError>;

// After:
async fn grant_access(&self, member_id: &str) -> Result<JoinInfo, CloudHomeError>;
async fn revoke_access(&self, member_id: &str) -> Result<(), CloudHomeError>;
```

For S3, `grant_access` ignores `member_id` and returns bucket/region/endpoint (access is out-of-band). `revoke_access` stays a no-op.

### Fix 2: JoinInfo enum -- `bae-core/src/cloud_home/mod.rs`

Add consumer cloud variants (needed for Phase 4 invite codes too):

```rust
pub enum JoinInfo {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
    },
    GoogleDrive {
        folder_id: String,
    },
    Dropbox {
        shared_folder_id: String,
    },
    OneDrive {
        drive_id: String,
        folder_id: String,
    },
    PCloud {
        folder_id: u64,
    },
    // iCloud is macOS-only and uses folder sharing via NSSharingService,
    // not an invite code. The joiner sees the shared folder appear
    // automatically in their iCloud Drive. So no JoinInfo variant needed
    // for the code-based join flow.
}
```

Derive `Serialize, Deserialize` on JoinInfo (needed for Phase 4 invite code encoding).

### Fix 3: Wire grant_access into invite.rs -- `bae-core/src/sync/invite.rs`

`create_invitation()` currently writes the membership entry and wraps the key, but never calls `grant_access`. Fix:

```rust
pub async fn create_invitation(
    bucket: &dyn SyncBucketClient,
    cloud_home: &dyn CloudHome,   // <-- new parameter
    // ... existing params ...
) -> Result<JoinInfo, InviteError> {
    // 1. Validate chain (existing)
    // 2. Call cloud_home.grant_access(invitee_pubkey_hex) -- NEW
    let join_info = cloud_home.grant_access(invitee_pubkey_hex).await?;
    // 3. Wrap library key to invitee (existing)
    // 4. Write membership entry (existing)
    Ok(join_info)
}
```

Update `revoke_member()` similarly:

```rust
pub async fn revoke_member(
    bucket: &dyn SyncBucketClient,
    cloud_home: &dyn CloudHome,   // <-- new parameter
    // ... existing params ...
) -> Result<(), InviteError> {
    // 1. Validate chain (existing)
    // 2. Call cloud_home.revoke_access(member_pubkey_hex) -- NEW
    cloud_home.revoke_access(member_pubkey_hex).await?;
    // 3. Write removal entry (existing)
    // 4. Rotate encryption key (existing)
    Ok(())
}
```

### Fix 4: Rename config fields -- `bae-core/src/config.rs`

```
sync_s3_bucket   →  cloud_home_s3_bucket
sync_s3_region   →  cloud_home_s3_region
sync_s3_endpoint →  cloud_home_s3_endpoint
```

Update `ConfigYaml`, `Config`, `load_from_bae_dir`, `save_to_config_yaml`, `create_new_library`, `is_sync_configured()`. Also update `bae-desktop/src/main.rs`, `bae-server/src/main.rs` (anywhere that reads these fields).

### Fix 5: Rename keyring entries -- `bae-core/src/keys.rs`

```
sync_s3_access_key  →  cloud_home_access_key
sync_s3_secret_key  →  cloud_home_secret_key
```

Add new entry:
```
cloud_home_oauth_token  -- OAuth refresh token (consumer cloud providers)
```

Update `KeyService` methods:
```rust
// Rename:
set_sync_access_key / get_sync_access_key  →  set_cloud_home_access_key / get_cloud_home_access_key
set_sync_secret_key / get_sync_secret_key  →  set_cloud_home_secret_key / get_cloud_home_secret_key

// Add:
pub fn set_cloud_home_oauth_token(&self, token: &str) -> Result<(), KeyError>;
pub fn get_cloud_home_oauth_token(&self) -> Option<String>;
pub fn delete_cloud_home_oauth_token(&self) -> Result<(), KeyError>;
```

Keyring entry names: `{library_id}_cloud_home_access_key`, `{library_id}_cloud_home_secret_key`, `{library_id}_cloud_home_oauth_token`.

### Fix 6: Rename UI store -- `bae-ui/src/stores/sync.rs`

```rust
// Before:
pub sync_bucket: Option<String>,
pub sync_region: Option<String>,
pub sync_endpoint: Option<String>,
pub sync_configured: bool,

// After:
pub cloud_home_bucket: Option<String>,
pub cloud_home_region: Option<String>,
pub cloud_home_endpoint: Option<String>,
pub cloud_home_configured: bool,
```

Also rename `ShareInfo` fields: `bucket` → `cloud_home_bucket`, etc. Update all references in `bae-ui/src/components/settings/sync.rs` and `bae-desktop/src/ui/components/settings/sync.rs`.

### Fix 7: Update SyncSectionView props -- `bae-ui/src/components/settings/sync.rs`

Rename all `sync_bucket` / `sync_region` / `sync_endpoint` / `sync_configured` props to `cloud_home_*`. Update all internal usage. Update `bae-desktop` wrapper and `bae-mocks` call sites.

### Fix 8: Update callers -- `bae-desktop/src/ui/components/settings/sync.rs`

The desktop `SyncSection` that calls `create_invitation()` and `revoke_member()` needs to pass `cloud_home: &dyn CloudHome` through. It currently only has `&dyn SyncBucketClient`. Update `AppService` (or `SyncHandle`) to expose the `dyn CloudHome` reference so invite/revoke can call `grant_access`/`revoke_access`.

### Verification for Phase 2 fixes

- `cargo clippy -p bae-core -p bae-ui -p bae-desktop -p bae-mocks -p bae-server -- -D warnings`
- `cargo test -p bae-core` -- existing sync tests pass, invite tests updated
- Manual: sync still works (S3 backend, same behavior)
- Manual: invite flow still works (grant_access returns JoinInfo::S3 with bucket/region/endpoint)

---

## Shared infrastructure (before any backend)

### OAuth helper -- `bae-core/src/oauth.rs` (new)

Backends 3b-3e all need OAuth 2.0. Factor out the common flow.

```rust
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>, // None for public clients (PKCE-only)
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_port: u16, // localhost callback port (default 19284)
}

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>, // unix timestamp
}

/// Open browser, listen on localhost, exchange code for tokens.
pub async fn authorize(config: &OAuthConfig) -> Result<OAuthTokens, OAuthError>;

/// Refresh an expired access token.
pub async fn refresh(config: &OAuthConfig, refresh_token: &str) -> Result<OAuthTokens, OAuthError>;
```

Flow: generate PKCE verifier/challenge, open `auth_url` in default browser with `redirect_uri=http://localhost:{port}/callback`, spawn a one-shot axum server on that port, wait for the callback, exchange the authorization code for tokens at `token_url`.

Dependencies: `reqwest` (already in workspace), `open` (already in workspace), `rand` (for PKCE verifier). The one-shot HTTP server reuses `axum` + `tokio` (both in workspace).

### Token storage -- `bae-core/src/keys.rs`

The `cloud_home_oauth_token` keyring entry (added in Fix 5) stores the OAuth refresh token as a JSON string containing `OAuthTokens` fields. Access token + expiry are stored alongside the refresh token so we don't need to refresh on every app launch.

### Config -- `bae-core/src/config.rs`

Add cloud home provider selection:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CloudProvider {
    S3,
    ICloud,
    GoogleDrive,
    Dropbox,
    OneDrive,
    PCloud,
}
```

New fields on `ConfigYaml` / `Config`:

```rust
/// Selected cloud provider for the cloud home. None = not configured.
#[serde(default)]
pub cloud_provider: Option<CloudProvider>,

/// Google Drive folder ID for the cloud home.
#[serde(default)]
pub cloud_home_google_drive_folder_id: Option<String>,

/// Dropbox folder path for the cloud home.
#[serde(default)]
pub cloud_home_dropbox_folder_path: Option<String>,

/// OneDrive drive + folder IDs.
#[serde(default)]
pub cloud_home_onedrive_drive_id: Option<String>,
#[serde(default)]
pub cloud_home_onedrive_folder_id: Option<String>,

/// pCloud folder ID.
#[serde(default)]
pub cloud_home_pcloud_folder_id: Option<u64>,
```

The existing `cloud_home_s3_bucket` / `cloud_home_s3_region` / `cloud_home_s3_endpoint` fields (renamed in Fix 4) stay for the S3 provider.

Update `is_sync_configured()` to check `cloud_provider.is_some()` instead of just S3 fields.

### CloudHome factory -- `bae-core/src/cloud_home/mod.rs`

```rust
/// Construct a CloudHome from config + keyring tokens.
pub async fn create_cloud_home(
    config: &Config,
    key_service: &KeyService,
) -> Result<Box<dyn CloudHome>, CloudHomeError>;
```

Dispatches based on `config.cloud_provider`:
- `S3` → read bucket/region/endpoint from config, access_key/secret_key from keyring → `S3CloudHome::new()`
- `GoogleDrive` → read folder_id from config, OAuth tokens from keyring → `GoogleDriveCloudHome::new()`
- etc.

### UI store -- `bae-ui/src/stores/config.rs`

Add to `ConfigState`:

```rust
pub cloud_provider: Option<CloudProvider>,
pub cloud_account_display: Option<String>, // "user@gmail.com", "iCloud Drive", etc.
```

`CloudProvider` is defined in bae-ui (mirror of bae-core's enum, since bae-ui can't depend on bae-core).

### UI store -- `bae-ui/src/stores/sync.rs`

Add:

```rust
/// Whether iCloud Drive is available on this platform.
pub icloud_available: bool,
/// Whether an OAuth sign-in is currently in progress.
pub signing_in: bool,
/// Error from a sign-in attempt.
pub sign_in_error: Option<String>,
```

### AppService -- `bae-desktop/src/ui/app_service.rs`

New methods:

```rust
/// Start OAuth sign-in for a cloud provider. Opens browser, waits for callback.
pub fn sign_in_cloud_provider(&self, provider: CloudProvider);

/// Disconnect the current cloud provider. Deletes tokens, clears config.
pub fn disconnect_cloud_provider(&self);

/// Enable iCloud Drive as the cloud home backend.
pub fn use_icloud(&self);
```

`sign_in_cloud_provider` spawns an async task that:
1. Calls `oauth::authorize()` with the provider's config
2. Stores tokens in keyring via `key_service.set_cloud_home_oauth_token()`
3. Creates the cloud home folder (Google: `Files.create` folder, Dropbox: `files/create_folder_v2`, etc.)
4. Saves `cloud_provider` + folder ID to config
5. Restarts the sync handle with the new CloudHome backend
6. Updates Store with the connected account display name

---

## 3a: iCloud Drive

The simplest backend. No REST API, no OAuth. Just filesystem operations on an iCloud ubiquity container. macOS only.

### What it does

Register `iCloud.com.bae.bae` ubiquity container. Apple syncs the container across devices automatically. `NSMetadataQuery` notifies when remote changes arrive.

### CloudHome implementation -- `bae-core/src/cloud_home/icloud.rs` (new)

```rust
pub struct ICloudCloudHome {
    /// Root path: container_url / "libraries" / "{library_id}"
    root: PathBuf,
}

impl ICloudCloudHome {
    /// Detect iCloud availability and return the container URL.
    /// Calls NSFileManager.url(forUbiquityContainerIdentifier:).
    pub fn detect() -> Option<PathBuf>;

    pub fn new(library_id: &str) -> Result<Self, CloudHomeError>;
}
```

CloudHome trait methods map to filesystem operations:
- `write` → `fs::write(root.join(key), data)`, create parent dirs
- `read` → `fs::read(root.join(key))`
- `read_range` → `File::open` + `seek` + `read_exact`
- `list` → `walkdir` recursively under `root.join(prefix)`, return relative paths from root
- `delete` → `fs::remove_file`
- `exists` → `Path::exists`
- `grant_access` → trigger `NSSharingService` to share the cloud home folder with the joiner's Apple ID. Apple requires system UI confirmation. `member_id` is the joiner's email (Apple ID). Returns... nothing useful for the invite code (the shared folder appears automatically on the joiner's machine). Could return a placeholder or a custom `JoinInfo::ICloud { container_id }` variant.
- `revoke_access` → remove collaborator via `NSSharingService`

### macOS bridging

`NSFileManager.url(forUbiquityContainerIdentifier:)` needs Objective-C bridging via `objc2`/`objc2-foundation` crates. Feature-gate behind `#[cfg(target_os = "macos")]`.

### Entitlements

The macOS app bundle needs `com.apple.developer.icloud-container-identifiers` entitlement with `iCloud.com.bae.bae`. Build/signing change.

### Change notification

`NSMetadataQuery` monitors the container for remote changes. When detected, trigger a sync pull. Supplements the existing 30-second polling loop with event-based triggers.

### Note on bae-server

bae-server does not support iCloud Drive -- it's a macOS-only filesystem path. macOS users use bae-desktop (which has the built-in Subsonic server for sharing). bae-server works with S3 and API-based consumer clouds.

---

## 3b: Google Drive

### CloudHome implementation -- `bae-core/src/cloud_home/google_drive.rs` (new)

```rust
pub struct GoogleDriveCloudHome {
    client: reqwest::Client,
    folder_id: String,
    tokens: Arc<RwLock<OAuthTokens>>,
    oauth_config: OAuthConfig,
    key_service: KeyService,
}
```

OAuth config:
- `client_id`: registered in Google Cloud Console
- `auth_url`: `https://accounts.google.com/o/oauth2/v2/auth`
- `token_url`: `https://oauth2.googleapis.com/token`
- `scopes`: `https://www.googleapis.com/auth/drive.file` (access only to files created by the app)

### Path mapping

CloudHomeSyncBucket uses paths like `changes/{device_id}/{seq}.enc`. Google Drive is folder-ID based, not path-based.

Two options:
- **Nested folders**: create actual folder hierarchy. Natural, but deep hierarchies are slow on Google Drive.
- **Flat filenames**: encode `/` as `__`. `changes/dev1/42.enc` → `changes__dev1__42.enc`. For `list(prefix)`, query `Files.list` with `name contains '{encoded_prefix}'`.

Go with flat filenames for simplicity and speed.

### API mapping

| CloudHome method | Google Drive API |
|---|---|
| `write` | `Files.create` (new) / `Files.update` (overwrite) |
| `read` | `Files.get` with `alt=media` |
| `read_range` | `Files.get` with `Range` header |
| `list` | `Files.list` with `q='{folder_id}' in parents and name contains '{prefix}'` |
| `delete` | `Files.delete` |
| `exists` | `Files.list` with `q=name='{key}'`, check count > 0 |
| `grant_access` | `Permissions.create` with `type=user, role=writer, emailAddress={member_id}` |
| `revoke_access` | `Permissions.delete` for the member's permission ID |

### Token refresh

Wrap every API call: on 401, call `oauth::refresh()`, store new tokens in keyring, retry once.

---

## 3c: Dropbox

### CloudHome implementation -- `bae-core/src/cloud_home/dropbox.rs` (new)

Dropbox has native path-based access -- paths work directly without encoding.

```rust
pub struct DropboxCloudHome {
    client: reqwest::Client,
    folder_path: String, // "/Apps/bae/{library_name}"
    tokens: Arc<RwLock<OAuthTokens>>,
    oauth_config: OAuthConfig,
    key_service: KeyService,
}
```

OAuth: PKCE flow, no client_secret needed.

### API mapping

| CloudHome method | Dropbox API |
|---|---|
| `write` | `POST /2/files/upload` |
| `read` | `POST /2/files/download` |
| `read_range` | `POST /2/files/download` with `Range` header |
| `list` | `POST /2/files/list_folder` + `/list_folder/continue` |
| `delete` | `POST /2/files/delete_v2` |
| `exists` | `POST /2/files/get_metadata`, check for 409 "not_found" |
| `grant_access` | `POST /2/sharing/add_folder_member` with `member_id` email |
| `revoke_access` | `POST /2/sharing/remove_folder_member` |

---

## 3d: OneDrive

### CloudHome implementation -- `bae-core/src/cloud_home/onedrive.rs` (new)

Uses Microsoft Graph API with path-based addressing (`:/{path}:`) which maps directly to CloudHome keys.

```rust
pub struct OneDriveCloudHome {
    client: reqwest::Client,
    drive_id: String,
    folder_id: String,
    tokens: Arc<RwLock<OAuthTokens>>,
    oauth_config: OAuthConfig,
    key_service: KeyService,
}
```

OAuth via Microsoft identity platform. Scope: `Files.ReadWrite`.

### API mapping

| CloudHome method | Microsoft Graph API |
|---|---|
| `write` | `PUT /drives/{drive_id}/items/{folder_id}:/{path}:/content` |
| `read` | `GET /drives/{drive_id}/items/{folder_id}:/{path}:/content` |
| `read_range` | Same with `Range` header |
| `list` | `GET /drives/{drive_id}/items/{folder_id}/children` (filtered) |
| `delete` | `DELETE /drives/{drive_id}/items/{folder_id}:/{path}:` |
| `exists` | `GET .../items/{folder_id}:/{path}:` check for 404 |
| `grant_access` | `POST .../items/{folder_id}/invite` with email |
| `revoke_access` | `DELETE .../items/{folder_id}/permissions/{id}` |

---

## 3e: pCloud

### CloudHome implementation -- `bae-core/src/cloud_home/pcloud.rs` (new)

pCloud uses folder IDs (u64). Use flat filenames like Google Drive.

```rust
pub struct PCloudCloudHome {
    client: reqwest::Client,
    folder_id: u64,
    api_host: String, // "api.pcloud.com" or "eapi.pcloud.com" (EU)
    tokens: Arc<RwLock<OAuthTokens>>,
    oauth_config: OAuthConfig,
    key_service: KeyService,
}
```

### API mapping

| CloudHome method | pCloud API |
|---|---|
| `write` | `uploadfile` |
| `read` | `downloadfile` |
| `list` | `listfolder` |
| `delete` | `deletefile` |
| `exists` | `stat` (check for error) |
| `grant_access` | `sharefolder` with email |
| `revoke_access` | `sharefolder` cancel |

---

## Settings UI

### Provider picker component -- `bae-ui/src/components/settings/cloud_provider.rs` (new)

A pure, props-based component for selecting and configuring the cloud home backend. Rendered inside the Sync settings tab, replacing the current "Sync Bucket" `SettingsCard`.

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct CloudProviderOption {
    pub provider: CloudProvider,
    pub label: &'static str,
    pub description: &'static str,
    pub available: bool, // false for iCloud on non-macOS
    pub connected_account: Option<String>, // "user@gmail.com", etc.
}

#[component]
pub fn CloudProviderPicker(
    /// Currently selected provider.
    selected: Option<CloudProvider>,
    /// Available provider options.
    options: Vec<CloudProviderOption>,
    /// Whether a sign-in is in progress.
    signing_in: bool,
    /// Sign-in error message.
    sign_in_error: Option<String>,

    // --- S3 edit state (shown when S3 is selected) ---
    s3_is_editing: bool,
    s3_bucket: String,
    s3_region: String,
    s3_endpoint: String,
    s3_access_key: String,
    s3_secret_key: String,

    // --- Callbacks ---
    on_select: EventHandler<CloudProvider>,
    on_sign_in: EventHandler<CloudProvider>,
    on_disconnect: EventHandler<()>,
    on_use_icloud: EventHandler<()>,
    // S3 callbacks
    on_s3_edit_start: EventHandler<()>,
    on_s3_cancel: EventHandler<()>,
    on_s3_save: EventHandler<SyncBucketConfig>,
    on_s3_bucket_change: EventHandler<String>,
    on_s3_region_change: EventHandler<String>,
    on_s3_endpoint_change: EventHandler<String>,
    on_s3_access_key_change: EventHandler<String>,
    on_s3_secret_key_change: EventHandler<String>,
) -> Element;
```

### Provider picker layout

```
┌─────────────────────────────────────────────────────┐
│ Cloud Home                                           │
│                                                      │
│  Where should bae store your library?               │
│                                                      │
│  ○ iCloud Drive              (macOS only)           │
│    Automatic sync, no setup needed                  │
│                                                      │
│  ○ Google Drive                                     │
│    Connected as user@gmail.com  [Disconnect]        │
│                                                      │
│  ○ Dropbox                                          │
│    [Sign in with Dropbox]                           │
│                                                      │
│  ○ OneDrive                                         │
│    [Sign in with OneDrive]                          │
│                                                      │
│  ○ pCloud                                           │
│    [Sign in with pCloud]                            │
│                                                      │
│  ○ S3-compatible                                    │
│    For Backblaze B2, Wasabi, MinIO, AWS, etc.       │
│    [Configure]                                       │
│                                                      │
└─────────────────────────────────────────────────────┘
```

**States per provider row:**
- **Not selected, not connected**: Radio button + label + description
- **Not selected, connected**: Radio button + label + "Connected as {email}" in green
- **Selected, not connected**: Radio filled + "[Sign in with {Provider}]" button
- **Selected, connected**: Radio filled + "Connected as {email}" + "[Disconnect]" button
- **Selected, signing in**: Radio filled + spinner + "Signing in..."
- **S3 selected, not configured**: Radio filled + S3 credential form (bucket, region, endpoint, access key, secret key) using existing `TextInput` components
- **S3 selected, configured**: Radio filled + bucket/region display + "[Edit]" button
- **iCloud selected, available**: Radio filled + "Using iCloud Drive" badge
- **iCloud, not available**: Radio disabled + grayed out + "(macOS only)"

### Integration into SyncSectionView -- `bae-ui/src/components/settings/sync.rs`

The current `SyncSectionView` has a "Sync Bucket" `SettingsCard` with S3-only fields. Replace it with `CloudProviderPicker`:

```rust
// Before (current):
SettingsCard {
    h3 { "Sync Bucket" }
    p { "S3-compatible bucket for syncing your library across devices" }
    // ... S3 fields ...
}

// After:
CloudProviderPicker {
    selected: cloud_provider,
    options: provider_options,
    signing_in: signing_in,
    sign_in_error: sign_in_error,
    // ... S3 edit state props (same as current) ...
    // ... callbacks ...
}
```

The rest of `SyncSectionView` (sync status, other devices, members list, invite form, shared releases) stays exactly as-is. Only the "Sync Bucket" card is replaced.

New props on `SyncSectionView`:
```rust
// Cloud provider props
cloud_provider: Option<CloudProvider>,
cloud_options: Vec<CloudProviderOption>,
signing_in: bool,
sign_in_error: Option<String>,
on_select_provider: EventHandler<CloudProvider>,
on_sign_in: EventHandler<CloudProvider>,
on_disconnect_provider: EventHandler<()>,
on_use_icloud: EventHandler<()>,
```

### Desktop wrapper -- `bae-desktop/src/ui/components/settings/sync.rs`

The `SyncSection` component currently reads S3 config from the store and manages edit state. Extend it to:

1. Read `cloud_provider`, `cloud_account_display`, `icloud_available`, `signing_in`, `sign_in_error` from Store
2. Build `Vec<CloudProviderOption>` from the store state
3. Wire `on_sign_in` to `app.sign_in_cloud_provider(provider)`
4. Wire `on_disconnect_provider` to `app.disconnect_cloud_provider()`
5. Wire `on_use_icloud` to `app.use_icloud()`
6. Keep existing S3 edit state logic, pass through to `CloudProviderPicker`'s S3 fields

### Mocks -- `bae-mocks/`

- `bae-mocks/src/mocks/settings.rs` -- add `CloudProviderPicker` mock with sample data (Google connected, others not)
- `bae-mocks/src/pages/settings.rs` -- pass new cloud provider props in the Sync tab

---

## Order

1. **Phase 2 fixes** first -- trait signature, naming, grant_access wiring. This is a prerequisite.
2. **Shared infrastructure** (OAuth, token storage, config fields, provider picker UI) ships with 3b.
3. **3a (iCloud Drive)** -- simplest, no OAuth, covers the primary platform.
4. **3b (Google Drive)** -- largest cross-platform user base, ships with OAuth infra.
5. **3c (Dropbox)**, **3d (OneDrive)**, **3e (pCloud)** -- in any order after 3b.

## Verification per backend

- `cargo clippy -p bae-core -p bae-ui -p bae-desktop -p bae-mocks -- -D warnings`
- `cargo test -p bae-core` -- existing sync tests unchanged (MockBucket is a SyncBucketClient, not a CloudHome)
- Manual: select provider in settings → sign in → sync starts
- Manual: other device → same provider, same account → sync works
- Manual: disconnect → tokens removed, config cleared, sync stops
- Manual: switch providers → old data stays in old provider, new data in new
