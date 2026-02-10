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
    /// The seq up to which the latest snapshot covers. None if no snapshot
    /// has been created by this device.
    pub snapshot_seq: Option<u64>,
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

    /// Fetch a single changeset by device_id and seq.
    ///
    /// Returns the **decrypted** envelope bytes from `changes/{device_id}/{seq}.enc`.
    /// Implementations must handle downloading the encrypted blob and decrypting
    /// it before returning. Callers receive plaintext ready for `envelope::unpack()`.
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
    /// If `snapshot_seq` is Some, the head records that a snapshot covers
    /// all changesets up to that seq.
    async fn put_head(
        &self,
        device_id: &str,
        seq: u64,
        snapshot_seq: Option<u64>,
    ) -> Result<(), BucketError>;

    /// Upload an encrypted image.
    /// Writes to `images/{id[0..2]}/{id[2..4]}/{id}`.
    async fn upload_image(&self, id: &str, data: Vec<u8>) -> Result<(), BucketError>;

    /// Download a decrypted image by ID.
    /// Reads from `images/{id[0..2]}/{id[2..4]}/{id}`.
    async fn download_image(&self, id: &str) -> Result<Vec<u8>, BucketError>;

    /// Upload an encrypted snapshot.
    /// Writes to `snapshot.db.enc` (overwrites any previous snapshot).
    async fn put_snapshot(&self, data: Vec<u8>) -> Result<(), BucketError>;

    /// Download the encrypted snapshot.
    /// Returns bytes from `snapshot.db.enc`.
    async fn get_snapshot(&self) -> Result<Vec<u8>, BucketError>;

    /// Delete a single changeset from the bucket.
    /// Removes `changes/{device_id}/{seq}.enc`.
    async fn delete_changeset(&self, device_id: &str, seq: u64) -> Result<(), BucketError>;

    /// List all changeset keys for a device.
    /// Returns the sequence numbers that exist in `changes/{device_id}/`.
    async fn list_changesets(&self, device_id: &str) -> Result<Vec<u64>, BucketError>;
}
