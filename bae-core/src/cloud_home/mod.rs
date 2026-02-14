//! CloudHome: low-level cloud storage abstraction.
//!
//! Each backend (S3, R2, B2, etc.) implements `CloudHome` -- 8 methods for
//! raw bytes in/out. No encryption, no path layout knowledge, no sync
//! semantics. Higher-level concerns live in `CloudHomeSyncBucket` which wraps
//! any `dyn CloudHome`.

pub mod s3;

use async_trait::async_trait;

/// Errors from raw cloud storage operations.
#[derive(Debug, thiserror::Error)]
pub enum CloudHomeError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Information needed to join a cloud home from another device.
pub enum JoinInfo {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
    },
}

/// Low-level cloud storage. Implementations handle a single bucket/container.
///
/// All methods deal in raw bytes. No encryption or path layout logic.
#[async_trait]
pub trait CloudHome: Send + Sync {
    /// Write bytes to a key, creating or overwriting.
    async fn write(&self, key: &str, data: Vec<u8>) -> Result<(), CloudHomeError>;

    /// Read the full contents of a key.
    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError>;

    /// Read a byte range from a key. `start` is inclusive, `end` is exclusive.
    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError>;

    /// List all keys under a prefix.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError>;

    /// Delete a key. Not an error if the key does not exist.
    async fn delete(&self, key: &str) -> Result<(), CloudHomeError>;

    /// Check whether a key exists.
    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError>;

    /// Return connection info that another device can use to access this
    /// cloud home. For S3 this is bucket/region/endpoint.
    fn join_info(&self) -> JoinInfo;

    /// Revoke a previously granted access. No-op for backends where access
    /// is controlled externally (e.g. S3 with pre-shared credentials).
    async fn revoke_access(&self) -> Result<(), CloudHomeError>;
}
