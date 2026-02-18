/// Sync bucket client: reads/writes to the S3 layout used for changeset sync.
///
/// Layout:
/// ```text
/// changes/{device_id}/{seq}.enc          -- encrypted changeset envelopes
/// heads/{device_id}.json.enc             -- encrypted head pointers
/// images/{ab}/{cd}/{id}                  -- encrypted library images
/// snapshot.db.enc                        -- full DB snapshot for bootstrapping
/// snapshot_meta.json.enc                 -- per-device cursors at snapshot time
/// membership/{author_pubkey}/{seq}.enc   -- encrypted membership entries
/// keys/{user_pubkey}.enc                 -- wrapped library keys per member
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
    /// RFC 3339 timestamp of when this head was last updated (i.e., when
    /// the device last synced). None for heads written before this field
    /// was added.
    pub last_sync: Option<String>,
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

impl From<crate::cloud_home::CloudHomeError> for BucketError {
    fn from(e: crate::cloud_home::CloudHomeError) -> Self {
        match e {
            crate::cloud_home::CloudHomeError::NotFound(key) => BucketError::NotFound(key),
            crate::cloud_home::CloudHomeError::Storage(msg) => BucketError::S3(msg),
            crate::cloud_home::CloudHomeError::Io(io_err) => {
                BucketError::S3(format!("I/O error: {io_err}"))
            }
        }
    }
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

    /// Upload a changeset blob (plaintext — the implementation encrypts it).
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
    /// all changesets up to that seq. `timestamp` is the RFC 3339 time of
    /// this sync (used by the sync status UI).
    async fn put_head(
        &self,
        device_id: &str,
        seq: u64,
        snapshot_seq: Option<u64>,
        timestamp: &str,
    ) -> Result<(), BucketError>;

    /// Upload an image (plaintext — the implementation encrypts it).
    /// Writes to `images/{id[0..2]}/{id[2..4]}/{id}`.
    ///
    /// `release_id`: `Some(id)` for cover images (uses per-release key),
    /// `None` for artist images (uses master key).
    async fn upload_image(
        &self,
        id: &str,
        release_id: Option<&str>,
        data: Vec<u8>,
    ) -> Result<(), BucketError>;

    /// Download a decrypted image by ID.
    /// Reads from `images/{id[0..2]}/{id[2..4]}/{id}`.
    ///
    /// `release_id`: `Some(id)` for cover images (uses per-release key),
    /// `None` for artist images (uses master key).
    async fn download_image(
        &self,
        id: &str,
        release_id: Option<&str>,
    ) -> Result<Vec<u8>, BucketError>;

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

    /// Get the minimum schema version required to sync with this bucket.
    ///
    /// Returns `None` if no minimum has been set (backwards compat: any version
    /// can sync). Reads from `min_schema_version.json.enc`.
    async fn get_min_schema_version(&self) -> Result<Option<u32>, BucketError>;

    /// Set the minimum schema version required to sync with this bucket.
    ///
    /// Writes to `min_schema_version.json.enc`. Used when a breaking migration
    /// bumps the schema and all devices must upgrade before syncing.
    async fn set_min_schema_version(&self, version: u32) -> Result<(), BucketError>;

    /// Upload a membership entry.
    /// Writes to `membership/{author_pubkey_hex}/{seq}.enc`.
    async fn put_membership_entry(
        &self,
        author_pubkey: &str,
        seq: u64,
        data: Vec<u8>,
    ) -> Result<(), BucketError>;

    /// Download a membership entry.
    /// Reads from `membership/{author_pubkey_hex}/{seq}.enc`.
    async fn get_membership_entry(
        &self,
        author_pubkey: &str,
        seq: u64,
    ) -> Result<Vec<u8>, BucketError>;

    /// List all membership entry keys.
    /// Returns tuples of (author_pubkey, seq).
    async fn list_membership_entries(&self) -> Result<Vec<(String, u64)>, BucketError>;

    /// Upload a wrapped library key for a member.
    /// Writes to `keys/{user_pubkey_hex}.enc`.
    async fn put_wrapped_key(&self, user_pubkey: &str, data: Vec<u8>) -> Result<(), BucketError>;

    /// Download a wrapped library key for a member.
    /// Reads from `keys/{user_pubkey_hex}.enc`.
    async fn get_wrapped_key(&self, user_pubkey: &str) -> Result<Vec<u8>, BucketError>;

    /// Delete a wrapped library key.
    /// Removes `keys/{user_pubkey_hex}.enc`.
    async fn delete_wrapped_key(&self, user_pubkey: &str) -> Result<(), BucketError>;

    /// Upload snapshot metadata (plaintext -- the implementation encrypts it).
    /// Writes to `snapshot_meta.json.enc`.
    async fn put_snapshot_meta(&self, data: Vec<u8>) -> Result<(), BucketError>;

    /// Download snapshot metadata (decrypted).
    /// Reads from `snapshot_meta.json.enc`. Returns NotFound if no metadata exists.
    async fn get_snapshot_meta(&self) -> Result<Vec<u8>, BucketError>;
}
