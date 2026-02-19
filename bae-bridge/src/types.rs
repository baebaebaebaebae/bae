#[derive(Debug, uniffi::Record)]
pub struct BridgeLibraryInfo {
    pub id: String,
    pub name: Option<String>,
    pub path: String,
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
