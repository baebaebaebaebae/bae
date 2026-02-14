# Phase 5: Follow

A friend browses your full catalog without touching your cloud home. They connect to your bae-desktop or bae-server via Subsonic. Three sub-phases, each independently shippable.

## 5a: Subsonic authentication

### What exists

- Subsonic API server: 11 endpoints (`ping`, `getArtists`, `getArtist`, `getAlbum`, `getAlbumList2`, `search3`, `stream`, `getCoverArt`, `getRandomSongs`, `getScanStatus`, `startScan`)
- Routes in `bae-core/src/subsonic.rs` — each handler takes `State<SubsonicState>` + `Query<SubsonicQuery>`
- `SubsonicQuery` already has `u: Option<String>` (username) but no `p`/`t`/`s` fields for auth
- bae-server: `--username`/`--password` CLI args don't exist
- bae-desktop: Subsonic settings tab has enable/port only, no auth fields
- No authentication middleware — all endpoints are open

### Changes

**`bae-core/src/subsonic.rs`:**
- Add `p: Option<String>` (password or hex-encoded password), `t: Option<String>` (token), `s: Option<String>` (salt) fields to `SubsonicQuery`
- Add `subsonic_auth_enabled: bool`, `subsonic_username: Option<String>`, `subsonic_password_md5: Option<String>` to `SubsonicState`
- Add `validate_auth()` function: checks `SubsonicState.subsonic_auth_enabled`, validates username match, validates password (plaintext `p=enc:...` or token-salt `t`+`s` per Subsonic spec: `token = md5(password + salt)`)
- Add auth check axum middleware layer on the router (or call `validate_auth` at the start of each handler — middleware is cleaner since it avoids modifying all 11 handlers)
- Return Subsonic XML/JSON error response (status="failed", code=40 "Wrong username or password") on auth failure

**`bae-core/src/config.rs`:**
- Add `subsonic_auth_enabled: bool` (`#[serde(default)]`)
- Add `subsonic_username: Option<String>` (`#[serde(default)]`)
- Map in `load_from_bae_dir`, `save_to_config_yaml`, `create_new_library`

**`bae-core/src/keys.rs`:**
- Add `set_subsonic_password(password: &str)` / `get_subsonic_password()` / `delete_subsonic_password()` — stores MD5 hash in keyring under `{library_id}_subsonic_password`

**`bae-server/src/main.rs`:**
- Add `--subsonic-username` and `--subsonic-password` CLI args (+ env vars `BAE_SUBSONIC_USERNAME`, `BAE_SUBSONIC_PASSWORD`)
- Hash password with MD5, pass to `SubsonicState`

**`bae-desktop/src/main.rs`:**
- Read auth config and keyring password, pass to `SubsonicState` when creating router

**`bae-ui/src/stores/config.rs`:**
- Add `subsonic_auth_enabled: bool`, `subsonic_username: Option<String>` to `ConfigState`

**`bae-ui/src/components/settings/subsonic.rs`:**
- Add authentication card to `SubsonicSectionView`:
  - "Require authentication" toggle
  - Username text input (visible when auth enabled)
  - Password + confirm password inputs (visible when auth enabled)
  - Validation: passwords must match
- New props: `auth_enabled`, `username`, `edit_auth_enabled`, `edit_username`, `edit_password`, `edit_confirm_password`, `passwords_valid`, callbacks

**`bae-desktop/src/ui/components/settings/subsonic.rs`:**
- Wire auth fields to config store
- Save password hash to keyring via `key_service.set_subsonic_password()`
- Handle edit/save/cancel cycle for auth fields

**`bae-desktop/src/ui/app_service.rs`:**
- Sync new config fields in init and `save_config`

**Mocks:**
- `bae-mocks/src/mocks/settings.rs`, `pages/settings.rs` — pass new auth props

### Verification

- `cargo test -p bae-core` — subsonic auth tests (valid/invalid credentials, token-salt flow)
- Manual: enable auth in settings, set username/password → verify Subsonic clients need credentials
- Manual: bae-server with `--subsonic-username`/`--subsonic-password` → verify auth required

---

## 5b: Subsonic client

### What exists

Nothing. bae only serves Subsonic, never consumes.

### Changes

**`bae-core/src/subsonic_client.rs` (new):**

```rust
pub struct SubsonicClient {
    server_url: String,
    username: String,
    password: String,
    http: reqwest::Client,
}

impl SubsonicClient {
    pub fn new(server_url: String, username: String, password: String) -> Self;

    /// Build URL with Subsonic auth params (token-salt method)
    fn build_url(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> String;

    pub async fn ping(&self) -> Result<(), SubsonicClientError>;
    pub async fn get_artists(&self) -> Result<Vec<SubsonicArtist>, SubsonicClientError>;
    pub async fn get_artist(&self, id: &str) -> Result<SubsonicArtistDetail, SubsonicClientError>;
    pub async fn get_album(&self, id: &str) -> Result<SubsonicAlbum, SubsonicClientError>;
    pub async fn get_album_list(&self, list_type: &str, size: u32, offset: u32) -> Result<Vec<SubsonicAlbum>, SubsonicClientError>;
    pub async fn stream(&self, id: &str) -> Result<bytes::Bytes, SubsonicClientError>;
    pub async fn get_cover_art(&self, id: &str, size: Option<u32>) -> Result<bytes::Bytes, SubsonicClientError>;
    pub async fn search(&self, query: &str) -> Result<SubsonicSearchResult, SubsonicClientError>;
}
```

Response types: `SubsonicArtist`, `SubsonicAlbum`, `SubsonicSong`, `SubsonicSearchResult` — modeled after the Subsonic API JSON response format. These live in `subsonic_client.rs`.

Error type:
```rust
#[derive(Debug, thiserror::Error)]
pub enum SubsonicClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("parse error: {0}")]
    Parse(String),
}
```

Register in `bae-core/src/lib.rs`.

### Verification

- Unit tests with mock HTTP server (or integration test against bae's own Subsonic server)
- Test: ping, get_artists, get_album, stream returns bytes

---

## 5c: Followed libraries + library switcher

### What exists

- Library switcher in settings: discover, switch, create, rename, remove — local libraries only
- Library page reads from local DB via `LibraryManager` / `SharedLibraryManager`
- bae-ui display types: `Album`, `Artist`, `Track` (pure display structs in bae-ui)
- bae-desktop converts bae-core DB types to bae-ui display types in wrapper components

### Design

A "followed library" is a remote Subsonic server. The user configures a server URL + credentials. When they select it in the library switcher, the UI fetches data from the Subsonic API instead of the local DB. Playback streams through Subsonic.

### Changes

**Config — `bae-core/src/config.rs`:**
- Add `followed_libraries: Vec<FollowedLibrary>` to `ConfigYaml`/`Config`

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct FollowedLibrary {
    pub id: String,          // local UUID
    pub name: String,        // display name
    pub server_url: String,
    pub username: String,
    // password stored in keyring: {library_id}_followed_{id}_password
}
```

**Keyring — `bae-core/src/keys.rs`:**
- `set_followed_password(followed_id: &str, password: &str)` / `get_followed_password(followed_id: &str)`

**UI store — `bae-ui/src/stores/config.rs`:**
- Add `FollowedLibraryInfo` display type
- Add `followed_libraries: Vec<FollowedLibraryInfo>` to `ConfigState`

**Library data source — `bae-ui/src/stores/library_source.rs` (new in bae-ui):**

Enum to represent where library data comes from:
```rust
pub enum LibrarySource {
    Local,
    Followed(String),  // followed library ID
}
```

Add `active_source: LibrarySource` to AppState. When `Local`, UI reads from local store as today. When `Followed(id)`, UI uses data fetched from Subsonic client.

**bae-desktop data fetching — `bae-desktop/src/ui/components/followed_library.rs` (new):**

When a followed library is selected:
1. Create `SubsonicClient` from config + keyring password
2. Fetch `get_artists()`, `get_album_list()` from remote server
3. Convert `SubsonicArtist`/`SubsonicAlbum` responses to bae-ui display types
4. Store in AppState (separate from local library data)

**Library switcher — `bae-ui/src/components/settings/library.rs`:**
- Show followed libraries in the library list, tagged "Following"
- "Add Followed Library" button → form: name, server URL, username, password, "Test Connection" button
- Selecting a followed library sets `active_source` to `Followed(id)`

**bae-desktop settings wrapper — `bae-desktop/src/ui/components/settings/library.rs`:**
- Wire follow form to config store
- Test connection: create SubsonicClient, call ping()
- Save/remove followed libraries

**Library page — `bae-desktop/src/ui/components/library.rs` (or wherever the main library view is):**
- Check `active_source` — if `Followed`, use fetched Subsonic data instead of local LibraryManager data
- Hide write operations (import, edit, delete) for followed libraries
- Album detail page: fetch from Subsonic client on navigation

**Playback — `bae-desktop/src/ui/components/album_detail/page.rs`:**
- For followed libraries: stream through Subsonic `stream` endpoint instead of local file playback
- Cover art: fetch through Subsonic `getCoverArt` instead of local image server

**Mocks:**
- Update library switcher mock with followed library entries
- Add mock followed library view

### Verification

- `cargo clippy -p bae-core -p bae-ui -p bae-desktop -- -D warnings`
- `cargo test -p bae-core` — SubsonicClient tests
- Manual: add a followed library pointing to another bae instance → browse albums, play tracks
- Manual: switch between local and followed library in switcher
- Manual: verify write operations hidden for followed libraries
