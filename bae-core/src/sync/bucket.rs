/// Sync bucket client: reads/writes to the S3 layout used for changeset sync.
///
/// Layout:
/// ```text
/// changes/{device_id}/{seq}.enc   -- encrypted changeset envelopes
/// heads/{device_id}.json.enc      -- encrypted head pointers
/// images/{ab}/{cd}/{id}           -- encrypted library images
/// snapshot.db.enc                 -- full DB snapshot for bootstrapping
/// ```
///
/// All data is encrypted before upload and decrypted after download.
/// The trait is async and mockable for testing.
use async_trait::async_trait;

/// Per-device head: the latest sequence number for a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceHead {
    pub device_id: String,
    pub seq: u64,
}

/// Error type for bucket operations.
#[derive(Debug, thiserror::Error)]
pub enum BucketError {
    #[error("S3 operation failed: {0}")]
    S3(String),
    #[error("object not found: {0}")]
    NotFound(String),
    #[error("decryption failed: {0}")]
    Decryption(String),
}

#[async_trait]
pub trait SyncBucketClient: Send + Sync {
    /// List all device heads (one LIST call to `heads/`).
    async fn list_heads(&self) -> Result<Vec<DeviceHead>, BucketError>;

    /// Fetch a single changeset blob by device_id and seq.
    /// Returns the decrypted bytes from `changes/{device_id}/{seq}.enc`.
    async fn get_changeset(&self, device_id: &str, seq: u64) -> Result<Vec<u8>, BucketError>;

    /// Upload an encrypted changeset blob.
    /// Writes to `changes/{device_id}/{seq}.enc`.
    async fn put_changeset(
        &self,
        device_id: &str,
        seq: u64,
        data: Vec<u8>,
    ) -> Result<(), BucketError>;

    /// Update the head pointer for a device.
    /// Writes to `heads/{device_id}.json.enc`.
    async fn put_head(&self, device_id: &str, seq: u64) -> Result<(), BucketError>;

    /// Upload an encrypted image.
    /// Writes to `images/{id[0..2]}/{id[2..4]}/{id}`.
    async fn upload_image(&self, id: &str, data: Vec<u8>) -> Result<(), BucketError>;
}
