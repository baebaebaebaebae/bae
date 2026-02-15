//! Centralized file reading with transparent decryption.
//!
//! Given a file_id, resolves the file's location, reads the raw bytes,
//! and decrypts if the file has an encryption_nonce set.

use crate::db::{Database, DbFile, EncryptionScheme};
use crate::encryption::EncryptionService;
use crate::library_dir::LibraryDir;
use std::path::Path;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("File has no readable location: {0}")]
    NoLocation(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Decryption error: {0}")]
    Decryption(#[from] crate::encryption::EncryptionError),
    #[error("Decryption required but no encryption service configured")]
    EncryptionNotConfigured,
    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

/// Read a release file's bytes, decrypting if necessary.
///
/// Looks up the file and its release by ID, resolves the file path from the
/// release's storage flags (managed_locally or unmanaged_path), reads the raw
/// bytes, and decrypts based on the file's encryption_nonce and encryption_scheme.
pub async fn read_file(
    db: &Database,
    file_id: &str,
    library_dir: &LibraryDir,
    encryption_service: Option<&EncryptionService>,
) -> Result<(DbFile, Vec<u8>), FileError> {
    let file = db
        .get_file_by_id(file_id)
        .await?
        .ok_or_else(|| FileError::NotFound(file_id.to_string()))?;

    let release = db
        .get_release_by_id(&file.release_id)
        .await?
        .ok_or_else(|| FileError::NotFound(format!("release for file {}", file_id)))?;

    let source_path = if release.managed_locally {
        file.local_storage_path(library_dir)
    } else if let Some(ref unmanaged_path) = release.unmanaged_path {
        Path::new(unmanaged_path).join(&file.original_filename)
    } else {
        return Err(FileError::NoLocation(file_id.to_string()));
    };

    debug!("Reading file {} from {}", file_id, source_path.display());

    let raw_bytes = tokio::fs::read(&source_path).await?;

    let data = decrypt_if_needed(&file, encryption_service, raw_bytes).await?;

    Ok((file, data))
}

/// Decrypt raw bytes if the file has encryption_nonce set.
///
/// Uses the encryption scheme on the file to determine which key:
/// - Master: decrypts with the master key directly
/// - Derived: derives a per-release key via HKDF then decrypts
pub async fn decrypt_if_needed(
    file: &DbFile,
    encryption_service: Option<&EncryptionService>,
    raw_bytes: Vec<u8>,
) -> Result<Vec<u8>, FileError> {
    if file.encryption_nonce.is_none() {
        return Ok(raw_bytes);
    }

    let enc = encryption_service.ok_or(FileError::EncryptionNotConfigured)?;

    let release_id = file.release_id.clone();
    let scheme = file.encryption_scheme;
    let enc = enc.clone();

    tokio::task::spawn_blocking(move || match scheme {
        EncryptionScheme::Master => enc.decrypt(&raw_bytes).map_err(FileError::Decryption),
        EncryptionScheme::Derived => enc
            .decrypt_for_release(&release_id, &raw_bytes)
            .map_err(FileError::Decryption),
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_type::ContentType;
    use crate::db::DbFile;
    use crate::encryption::EncryptionService;

    fn make_file(encrypted: bool, scheme: EncryptionScheme) -> DbFile {
        let mut file = DbFile::new("release-1", "test.flac", 1000, ContentType::Flac);
        if encrypted {
            file.encryption_nonce = Some(vec![0u8; 24]);
        }
        file.encryption_scheme = scheme;
        file
    }

    #[tokio::test]
    async fn decrypt_if_needed_returns_plaintext_for_unencrypted() {
        let file = make_file(false, EncryptionScheme::Master);
        let data = b"hello world".to_vec();
        let result = decrypt_if_needed(&file, None, data.clone()).await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn decrypt_if_needed_fails_without_encryption_service() {
        let file = make_file(true, EncryptionScheme::Master);
        let data = b"encrypted junk".to_vec();
        let result = decrypt_if_needed(&file, None, data).await;
        assert!(matches!(result, Err(FileError::EncryptionNotConfigured)));
    }

    #[tokio::test]
    async fn decrypt_if_needed_decrypts_master_scheme() {
        let enc = EncryptionService::new_with_key(&[42u8; 32]);
        let plaintext = b"test audio data for master key";
        let encrypted = enc.encrypt(plaintext);

        let mut file = make_file(true, EncryptionScheme::Master);
        // Set the real nonce from the encrypted data
        file.encryption_nonce = Some(encrypted[..24].to_vec());

        let result = decrypt_if_needed(&file, Some(&enc), encrypted)
            .await
            .unwrap();
        assert_eq!(result, plaintext);
    }

    #[tokio::test]
    async fn decrypt_if_needed_decrypts_derived_scheme() {
        let enc = EncryptionService::new_with_key(&[42u8; 32]);
        let release_id = "release-1";
        let plaintext = b"test audio data for derived key";
        let encrypted = enc.encrypt_for_release(release_id, plaintext);

        let mut file = make_file(true, EncryptionScheme::Derived);
        file.release_id = release_id.to_string();
        file.encryption_nonce = Some(encrypted[..24].to_vec());

        let result = decrypt_if_needed(&file, Some(&enc), encrypted)
            .await
            .unwrap();
        assert_eq!(result, plaintext);
    }
}
