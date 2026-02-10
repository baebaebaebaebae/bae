//! Storage trait and implementation
use crate::cloud_storage::{s3_config_from_profile, CloudStorage, S3CloudStorage};
use crate::content_type::ContentType;
use crate::db::{Database, DbFile, DbStorageProfile, StorageLocation};
use crate::encryption::EncryptionService;
use crate::keys::KeyService;
use crate::storage::storage_path;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage not configured")]
    NotConfigured,
    #[error("Cloud storage error: {0}")]
    Cloud(String),
    #[error("Database error: {0}")]
    Database(String),
}

/// Progress callback type: (bytes_written, total_bytes)
pub type ProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

/// Trait for writing release files to storage during import
///
/// Abstracts over different storage configurations (local/cloud, encrypted/plain).
/// Implementations apply the appropriate transforms based on the StorageProfile.
#[async_trait]
pub trait ReleaseStorage: Send + Sync {
    /// Write a file to storage with progress reporting.
    ///
    /// Creates the DbFile record first (to generate its UUID), then uses the
    /// file_id for the hash-based storage path: `storage/ab/cd/{file_id}`.
    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        on_progress: ProgressCallback,
    ) -> Result<(), StorageError>;
}

/// Storage implementation that applies transforms based on StorageProfile flags
///
/// Handles combinations of (local/cloud) x (encrypted/plain).
/// Transforms are applied in sequence: encrypt -> store.
#[derive(Clone)]
pub struct ReleaseStorageImpl {
    profile: DbStorageProfile,
    encryption: Option<EncryptionService>,
    cloud: Option<Arc<dyn CloudStorage>>,
    database: Option<Arc<Database>>,
}

impl ReleaseStorageImpl {
    /// Create storage from a profile, reading S3 credentials from the keyring if needed.
    pub async fn from_profile(
        profile: DbStorageProfile,
        encryption: Option<EncryptionService>,
        database: Arc<Database>,
        key_service: &KeyService,
    ) -> Result<Self, StorageError> {
        let cloud: Option<Arc<dyn CloudStorage>> = if profile.location == StorageLocation::Cloud {
            let s3_config = s3_config_from_profile(&profile, key_service)
                .ok_or_else(|| StorageError::Cloud("Missing S3 credentials for profile".into()))?;
            let client = S3CloudStorage::new(s3_config)
                .await
                .map_err(|e| StorageError::Cloud(e.to_string()))?;

            info!("Created S3 client for profile: {}", profile.name);
            Some(Arc::new(client))
        } else {
            None
        };

        Ok(Self {
            profile,
            encryption,
            cloud,
            database: Some(database),
        })
    }

    /// Create storage with an injected cloud storage (for testing).
    #[cfg(feature = "test-utils")]
    pub fn with_cloud(
        profile: DbStorageProfile,
        encryption: Option<EncryptionService>,
        cloud: Arc<dyn CloudStorage>,
        database: Arc<Database>,
    ) -> Self {
        Self {
            profile,
            encryption,
            cloud: Some(cloud),
            database: Some(database),
        }
    }

    /// Encrypt data if encryption is enabled
    fn encrypt_if_needed(&self, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if !self.profile.encrypted {
            return Ok(data.to_vec());
        }
        let encryption = self
            .encryption
            .as_ref()
            .ok_or(StorageError::NotConfigured)?;
        Ok(encryption.encrypt(data))
    }
}

#[async_trait]
impl ReleaseStorage for ReleaseStorageImpl {
    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        on_progress: ProgressCallback,
    ) -> Result<(), StorageError> {
        use tokio::io::AsyncWriteExt;

        let total_bytes = data.len();
        on_progress(0, total_bytes);

        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_lowercase();

        // Create DbFile first to get its UUID for the storage path
        let mut db_file = DbFile::new(
            release_id,
            filename,
            data.len() as i64,
            ContentType::from_extension(&ext),
        );

        let data_to_store = self.encrypt_if_needed(data)?;

        // Extract and store encryption nonce for efficient range requests
        if self.profile.encrypted && data_to_store.len() >= 24 {
            db_file.encryption_nonce = Some(data_to_store[..24].to_vec());
        }

        let rel_path = storage_path(&db_file.id);

        let source_path = match self.profile.location {
            StorageLocation::Local => {
                let path = PathBuf::from(&self.profile.location_path).join(&rel_path);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }

                let batch_size = 1_048_576; // 1MB batches for progress reporting
                let file = tokio::fs::File::create(&path).await?;
                let mut writer = tokio::io::BufWriter::new(file);
                let mut bytes_written = 0usize;

                for chunk in data_to_store.chunks(batch_size) {
                    writer.write_all(chunk).await?;
                    bytes_written += chunk.len();

                    // Adjust progress for encryption overhead
                    let progress_bytes = if data_to_store.len() != data.len() {
                        (bytes_written as f64 * data.len() as f64 / data_to_store.len() as f64)
                            as usize
                    } else {
                        bytes_written
                    };
                    on_progress(progress_bytes.min(total_bytes), total_bytes);
                }

                writer.flush().await?;
                path.display().to_string()
            }
            StorageLocation::Cloud => {
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let storage_location = cloud
                    .upload(&rel_path, &data_to_store)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?;
                on_progress(total_bytes, total_bytes);
                storage_location
            }
        };

        db_file.source_path = Some(source_path);

        if let Some(db) = &self.database {
            db.insert_file(&db_file)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        Ok(())
    }
}
