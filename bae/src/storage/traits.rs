//! Storage trait and implementation

use async_trait::async_trait;
use std::path::PathBuf;
use thiserror::Error;

use crate::db::{DbStorageProfile, StorageLocation};

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Storage not configured")]
    NotConfigured,

    #[error("Operation not supported for this storage configuration")]
    NotSupported(String),
}

/// Trait for reading/writing release files to storage
///
/// Abstracts over different storage configurations (local/cloud, encrypted/plain, chunked/raw).
/// Implementations apply the appropriate transforms based on the StorageProfile.
#[async_trait]
pub trait ReleaseStorage: Send + Sync {
    /// Read a file from storage
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError>;

    /// Write a file to storage
    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<(), StorageError>;

    /// List all files for a release
    async fn list_files(&self, release_id: &str) -> Result<Vec<String>, StorageError>;

    /// Check if a file exists
    async fn file_exists(&self, release_id: &str, filename: &str) -> Result<bool, StorageError>;

    /// Delete a file from storage
    async fn delete_file(&self, release_id: &str, filename: &str) -> Result<(), StorageError>;
}

/// Storage implementation that applies transforms based on StorageProfile flags
///
/// Handles all 8 combinations of (local/cloud) × (encrypted/plain) × (chunked/raw).
/// Transforms are applied in sequence: chunk → encrypt → store.
#[derive(Clone)]
pub struct ReleaseStorageImpl {
    profile: DbStorageProfile,
}

impl ReleaseStorageImpl {
    pub fn new(profile: DbStorageProfile) -> Self {
        Self { profile }
    }

    /// Get the local path for a release's files
    fn release_path(&self, release_id: &str) -> PathBuf {
        PathBuf::from(&self.profile.location_path).join(release_id)
    }

    /// Get the full path for a specific file
    fn file_path(&self, release_id: &str, filename: &str) -> PathBuf {
        self.release_path(release_id).join(filename)
    }
}

#[async_trait]
impl ReleaseStorage for ReleaseStorageImpl {
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError> {
        // For now, only implement local raw (no encryption, no chunking)
        if self.profile.location != StorageLocation::Local {
            return Err(StorageError::NotSupported(
                "Cloud storage not yet implemented".to_string(),
            ));
        }

        if self.profile.encrypted {
            return Err(StorageError::NotSupported(
                "Encryption not yet implemented".to_string(),
            ));
        }

        if self.profile.chunked {
            return Err(StorageError::NotSupported(
                "Chunked storage not yet implemented".to_string(),
            ));
        }

        // Local raw: just read the file
        let path = self.file_path(release_id, filename);
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(path.display().to_string())
            } else {
                StorageError::Io(e)
            }
        })
    }

    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<(), StorageError> {
        if self.profile.location != StorageLocation::Local {
            return Err(StorageError::NotSupported(
                "Cloud storage not yet implemented".to_string(),
            ));
        }

        if self.profile.encrypted {
            return Err(StorageError::NotSupported(
                "Encryption not yet implemented".to_string(),
            ));
        }

        if self.profile.chunked {
            return Err(StorageError::NotSupported(
                "Chunked storage not yet implemented".to_string(),
            ));
        }

        // Local raw: ensure directory exists and write file
        let path = self.file_path(release_id, filename);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn list_files(&self, release_id: &str) -> Result<Vec<String>, StorageError> {
        if self.profile.location != StorageLocation::Local {
            return Err(StorageError::NotSupported(
                "Cloud storage not yet implemented".to_string(),
            ));
        }

        let release_path = self.release_path(release_id);
        if !release_path.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(&release_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(name.to_string());
                }
            }
        }

        Ok(files)
    }

    async fn file_exists(&self, release_id: &str, filename: &str) -> Result<bool, StorageError> {
        if self.profile.location != StorageLocation::Local {
            return Err(StorageError::NotSupported(
                "Cloud storage not yet implemented".to_string(),
            ));
        }

        let path = self.file_path(release_id, filename);
        Ok(path.exists())
    }

    async fn delete_file(&self, release_id: &str, filename: &str) -> Result<(), StorageError> {
        if self.profile.location != StorageLocation::Local {
            return Err(StorageError::NotSupported(
                "Cloud storage not yet implemented".to_string(),
            ));
        }

        let path = self.file_path(release_id, filename);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_profile(temp_dir: &TempDir) -> DbStorageProfile {
        DbStorageProfile::new(
            "Test Local Raw",
            StorageLocation::Local,
            temp_dir.path().to_str().unwrap(),
            false, // not encrypted
            false, // not chunked
        )
    }

    #[tokio::test]
    async fn test_write_and_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new(test_profile(&temp_dir));

        let release_id = "test-release-123";
        let filename = "cover.jpg";
        let data = b"fake image data";

        // Write
        storage
            .write_file(release_id, filename, data)
            .await
            .unwrap();

        // Read back
        let read_data = storage.read_file(release_id, filename).await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new(test_profile(&temp_dir));

        let release_id = "test-release-456";

        // Doesn't exist yet
        assert!(!storage.file_exists(release_id, "nope.txt").await.unwrap());

        // Write it
        storage
            .write_file(release_id, "yes.txt", b"hello")
            .await
            .unwrap();

        // Now exists
        assert!(storage.file_exists(release_id, "yes.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_files() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new(test_profile(&temp_dir));

        let release_id = "test-release-789";

        // Empty initially
        let files = storage.list_files(release_id).await.unwrap();
        assert!(files.is_empty());

        // Add some files
        storage
            .write_file(release_id, "track01.flac", b"audio1")
            .await
            .unwrap();
        storage
            .write_file(release_id, "track02.flac", b"audio2")
            .await
            .unwrap();
        storage
            .write_file(release_id, "cover.jpg", b"image")
            .await
            .unwrap();

        // List them
        let mut files = storage.list_files(release_id).await.unwrap();
        files.sort();
        assert_eq!(files, vec!["cover.jpg", "track01.flac", "track02.flac"]);
    }

    #[tokio::test]
    async fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new(test_profile(&temp_dir));

        let release_id = "test-release-del";

        // Write and verify
        storage
            .write_file(release_id, "delete-me.txt", b"bye")
            .await
            .unwrap();
        assert!(storage
            .file_exists(release_id, "delete-me.txt")
            .await
            .unwrap());

        // Delete
        storage
            .delete_file(release_id, "delete-me.txt")
            .await
            .unwrap();
        assert!(!storage
            .file_exists(release_id, "delete-me.txt")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_read_nonexistent_returns_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new(test_profile(&temp_dir));

        let result = storage.read_file("no-release", "no-file.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
