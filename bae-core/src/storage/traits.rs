//! Storage trait and implementation
use crate::content_type::ContentType;
use crate::db::{Database, DbFile};
use crate::encryption::EncryptionService;
use crate::library_dir::LibraryDir;
use crate::storage::storage_path;
use async_trait::async_trait;
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

/// Storage implementation for managed local storage.
///
/// Writes files to `library_dir/storage/ab/cd/{file_id}`, optionally encrypting.
#[derive(Clone)]
pub struct ReleaseStorageImpl {
    library_dir: LibraryDir,
    encryption: Option<EncryptionService>,
    database: Option<Arc<Database>>,
}

impl ReleaseStorageImpl {
    /// Create storage for managed local imports.
    pub fn new_local(
        library_dir: LibraryDir,
        encryption: Option<EncryptionService>,
        database: Arc<Database>,
    ) -> Self {
        Self {
            library_dir,
            encryption,
            database: Some(database),
        }
    }

    /// Write bytes to local storage without creating a DB record.
    ///
    /// Uses the given `file_id` for the hash-based storage path.
    /// Returns the encryption nonce if encryption was applied.
    pub async fn store_bytes(
        &self,
        file_id: &str,
        data: &[u8],
        on_progress: ProgressCallback,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        use tokio::io::AsyncWriteExt;

        let total_bytes = data.len();
        on_progress(0, total_bytes);

        let data_to_store = self.encrypt_if_needed(data)?;

        let nonce = if self.encryption.is_some() && data_to_store.len() >= 24 {
            Some(data_to_store[..24].to_vec())
        } else {
            None
        };

        let rel_path = storage_path(file_id);
        let path = self.library_dir.join(&rel_path);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let batch_size = 1_048_576;
        let file = tokio::fs::File::create(&path).await?;
        let mut writer = tokio::io::BufWriter::new(file);
        let mut bytes_written = 0usize;

        for chunk in data_to_store.chunks(batch_size) {
            writer.write_all(chunk).await?;
            bytes_written += chunk.len();

            let progress_bytes = if data_to_store.len() != data.len() {
                (bytes_written as f64 * data.len() as f64 / data_to_store.len() as f64) as usize
            } else {
                bytes_written
            };
            on_progress(progress_bytes.min(total_bytes), total_bytes);
        }

        writer.flush().await?;

        Ok(nonce)
    }

    /// Encrypt data if encryption is enabled
    fn encrypt_if_needed(&self, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        match &self.encryption {
            Some(encryption) => Ok(encryption.encrypt(data)),
            None => Ok(data.to_vec()),
        }
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

        let nonce = self.store_bytes(&db_file.id, data, on_progress).await?;
        db_file.encryption_nonce = nonce;

        info!("Stored file {} -> {}", filename, storage_path(&db_file.id));

        if let Some(db) = &self.database {
            db.insert_file(&db_file)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        Ok(())
    }
}
