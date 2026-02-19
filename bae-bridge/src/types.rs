#[derive(Debug, uniffi::Record)]
pub struct BridgeLibraryInfo {
    pub id: String,
    pub name: Option<String>,
    pub path: String,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub cover_release_id: Option<String>,
    /// Comma-joined artist names for display
    pub artist_names: String,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeArtist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeRelease {
    pub id: String,
    pub album_id: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub tracks: Vec<BridgeTrack>,
    pub files: Vec<BridgeFile>,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeTrack {
    pub id: String,
    pub title: String,
    pub disc_number: Option<i32>,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Comma-joined artist names; may differ from album artist for compilations.
    pub artist_names: String,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeFile {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub content_type: String,
}

#[derive(Debug, uniffi::Record)]
pub struct BridgeAlbumDetail {
    pub album: BridgeAlbum,
    pub artists: Vec<BridgeArtist>,
    pub releases: Vec<BridgeRelease>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRepeatMode {
    None,
    Track,
    Album,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackState {
    Stopped,
    Loading {
        track_id: String,
    },
    Playing {
        track_id: String,
        track_title: String,
        artist_names: String,
        album_id: String,
        /// The image ID for album art (cover_release_id), if available.
        cover_image_id: Option<String>,
        position_ms: u64,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        track_title: String,
        artist_names: String,
        album_id: String,
        cover_image_id: Option<String>,
        position_ms: u64,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRemoteCover {
    pub url: String,
    pub thumbnail_url: String,
    pub label: String,
    pub source: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverSelection {
    ReleaseImage { file_id: String },
    RemoteCover { url: String, source: String },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidate {
    pub folder_path: String,
    pub artist_name: String,
    pub album_title: String,
    pub track_count: u32,
    pub format: String,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportStatus {
    Importing { progress_percent: u32 },
    Complete,
    Error { message: String },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMetadataResult {
    pub source: String,
    pub release_id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub track_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    pub library_id: String,
    pub library_name: Option<String>,
    pub library_path: String,
    pub has_discogs_token: bool,
    pub subsonic_port: u16,
    pub subsonic_bind_address: String,
    pub subsonic_username: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncStatus {
    pub configured: bool,
    pub syncing: bool,
    pub last_sync_time: Option<String>,
    pub error: Option<String>,
    pub device_count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMember {
    pub pubkey: String,
    /// "owner" or "member"
    pub role: String,
    pub added_by: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncConfig {
    pub cloud_provider: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_key_prefix: Option<String>,
    pub share_base_url: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSaveSyncConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub share_base_url: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSearchResults {
    pub artists: Vec<BridgeArtistSearchResult>,
    pub albums: Vec<BridgeAlbumSearchResult>,
    pub tracks: Vec<BridgeTrackSearchResult>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchResult {
    pub id: String,
    pub name: String,
    pub album_count: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub cover_release_id: Option<String>,
    pub artist_name: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    #[error("Not found: {msg}")]
    NotFound { msg: String },
    #[error("Configuration error: {msg}")]
    Config { msg: String },
    #[error("Database error: {msg}")]
    Database { msg: String },
    #[error("Internal error: {msg}")]
    Internal { msg: String },
    #[error("Import error: {msg}")]
    Import { msg: String },
}
