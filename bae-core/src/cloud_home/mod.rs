//! CloudHome: low-level cloud storage abstraction.
//!
//! Each backend (S3, R2, B2, etc.) implements `CloudHome` -- 8 methods for
//! raw bytes in/out. No encryption, no path layout knowledge, no sync
//! semantics. Higher-level concerns live in `CloudHomeSyncBucket` which wraps
//! any `dyn CloudHome`.

pub mod dropbox;
pub mod google_drive;
pub mod icloud;
pub mod onedrive;
pub mod s3;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JoinInfo {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
    GoogleDrive {
        folder_id: String,
    },
    Dropbox {
        shared_folder_id: String,
    },
    OneDrive {
        drive_id: String,
        folder_id: String,
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

    /// Grant access to a member and return connection info for the cloud home.
    /// For S3 this ignores `member_id` and returns bucket/region/endpoint
    /// (access is managed externally via IAM/pre-shared credentials).
    /// For consumer clouds this shares the folder with the member's account.
    async fn grant_access(&self, member_id: &str) -> Result<JoinInfo, CloudHomeError>;

    /// Revoke a previously granted access. No-op for backends where access
    /// is controlled externally (e.g. S3 with pre-shared credentials).
    async fn revoke_access(&self, member_id: &str) -> Result<(), CloudHomeError>;
}

/// Extract the OAuth token JSON from cloud home credentials, or return a storage error.
fn require_oauth_token(
    key_service: &crate::keys::KeyService,
    provider_name: &str,
) -> Result<String, CloudHomeError> {
    match key_service.get_cloud_home_credentials() {
        Some(crate::keys::CloudHomeCredentials::OAuth { token_json }) => Ok(token_json),
        _ => Err(CloudHomeError::Storage(format!(
            "{provider_name} OAuth token not in keyring"
        ))),
    }
}

/// Construct a CloudHome from config + keyring tokens.
pub async fn create_cloud_home(
    config: &crate::config::Config,
    key_service: &crate::keys::KeyService,
) -> Result<Box<dyn CloudHome>, CloudHomeError> {
    use crate::config::CloudProvider;

    match config.cloud_provider {
        Some(CloudProvider::S3) | None => {
            let bucket = config
                .cloud_home_s3_bucket
                .clone()
                .ok_or_else(|| CloudHomeError::Storage("S3 bucket not configured".to_string()))?;
            let region = config
                .cloud_home_s3_region
                .clone()
                .ok_or_else(|| CloudHomeError::Storage("S3 region not configured".to_string()))?;
            let endpoint = config.cloud_home_s3_endpoint.clone();

            let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
                Some(crate::keys::CloudHomeCredentials::S3 {
                    access_key,
                    secret_key,
                }) => (access_key, secret_key),
                _ => {
                    return Err(CloudHomeError::Storage(
                        "S3 credentials not in keyring".to_string(),
                    ))
                }
            };

            let s3 = s3::S3CloudHome::new(bucket, region, endpoint, access_key, secret_key).await?;
            Ok(Box::new(s3))
        }
        Some(CloudProvider::GoogleDrive) => {
            let folder_id = config
                .cloud_home_google_drive_folder_id
                .clone()
                .ok_or_else(|| {
                    CloudHomeError::Storage("Google Drive folder ID not configured".to_string())
                })?;

            let token_json = require_oauth_token(key_service, "Google Drive")?;
            let tokens: crate::oauth::OAuthTokens = serde_json::from_str(&token_json)
                .map_err(|e| CloudHomeError::Storage(format!("invalid OAuth token JSON: {e}")))?;

            let gd =
                google_drive::GoogleDriveCloudHome::new(folder_id, tokens, key_service.clone());
            Ok(Box::new(gd))
        }
        Some(CloudProvider::ICloud) => {
            let path = config
                .cloud_home_icloud_container_path
                .as_ref()
                .ok_or_else(|| {
                    CloudHomeError::Storage("iCloud container path not configured".to_string())
                })?;
            Ok(Box::new(icloud::ICloudCloudHome::new(
                std::path::PathBuf::from(path),
            )))
        }
        Some(CloudProvider::Dropbox) => {
            let folder_path = config
                .cloud_home_dropbox_folder_path
                .clone()
                .ok_or_else(|| {
                    CloudHomeError::Storage("Dropbox folder path not configured".to_string())
                })?;

            let token_json = require_oauth_token(key_service, "Dropbox")?;
            let tokens: crate::oauth::OAuthTokens = serde_json::from_str(&token_json)
                .map_err(|e| CloudHomeError::Storage(format!("invalid OAuth token JSON: {e}")))?;

            let db = dropbox::DropboxCloudHome::new(folder_path, tokens, key_service.clone());
            Ok(Box::new(db))
        }
        Some(CloudProvider::OneDrive) => {
            let drive_id = config.cloud_home_onedrive_drive_id.clone().ok_or_else(|| {
                CloudHomeError::Storage("OneDrive drive ID not configured".to_string())
            })?;
            let folder_id = config
                .cloud_home_onedrive_folder_id
                .clone()
                .ok_or_else(|| {
                    CloudHomeError::Storage("OneDrive folder ID not configured".to_string())
                })?;

            let token_json = require_oauth_token(key_service, "OneDrive")?;
            let tokens: crate::oauth::OAuthTokens = serde_json::from_str(&token_json)
                .map_err(|e| CloudHomeError::Storage(format!("invalid OAuth token JSON: {e}")))?;

            let od =
                onedrive::OneDriveCloudHome::new(drive_id, folder_id, tokens, key_service.clone());
            Ok(Box::new(od))
        }
    }
}
