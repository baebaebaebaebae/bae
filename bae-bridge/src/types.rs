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
