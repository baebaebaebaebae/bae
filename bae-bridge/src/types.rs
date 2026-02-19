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
}
