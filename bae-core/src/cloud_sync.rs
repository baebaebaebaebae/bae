//! Cloud sync: upload/download encrypted library DB and covers to S3.
//!
//! Separate from the `CloudStorage` trait used for audio file storage.
//! The sync service needs exact control over S3 keys (no hash-based partitioning).

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{debug, error, info};

use crate::encryption::EncryptionService;

#[derive(Error, Debug)]
pub enum CloudSyncError {
    #[error("S3 error: {0}")]
    S3(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] crate::encryption::EncryptionError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Fingerprint mismatch: expected {expected}, got {actual}")]
    FingerprintMismatch { expected: String, actual: String },
    #[error("No cloud sync metadata found in S3")]
    NoMetadata,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Metadata stored alongside the encrypted DB in S3.
/// Uploaded unencrypted so we can validate the key before downloading the DB.
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncMeta {
    pub fingerprint: String,
    pub uploaded_at: String,
}

/// Cloud sync service for uploading/downloading the encrypted library DB and covers.
pub struct CloudSyncService {
    client: Client,
    bucket: String,
    library_id: String,
    encryption_service: EncryptionService,
}

impl CloudSyncService {
    /// Create a new cloud sync service.
    pub async fn new(
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
        library_id: String,
        encryption_service: EncryptionService,
    ) -> Result<Self, CloudSyncError> {
        let credentials = Credentials::new(access_key, secret_key, None, None, "bae-cloud-sync");

        let mut builder = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region))
            .credentials_provider(credentials);

        if let Some(ref ep) = endpoint {
            let normalized = ep.trim_end_matches('/').to_string();
            builder = builder.endpoint_url(normalized);
        }

        let aws_config = builder.load().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket,
            library_id,
            encryption_service,
        })
    }

    /// Upload the library DB (via VACUUM INTO snapshot) and meta.json to S3.
    /// Returns the upload timestamp.
    pub async fn upload_db(&self, db_path: &Path) -> Result<String, CloudSyncError> {
        let snapshot_path = db_path.with_extension("db.snapshot");

        // Create DB snapshot using VACUUM INTO (called by the caller before this)
        // Read snapshot, encrypt, upload
        let data = tokio::fs::read(&snapshot_path).await?;

        info!(
            "Encrypting DB snapshot ({} bytes) for cloud sync",
            data.len()
        );

        let encrypted = self.encryption_service.encrypt(&data);

        let db_key = format!("bae/{}/library.db.enc", self.library_id);
        self.put_object(&db_key, &encrypted).await?;

        info!("Uploaded encrypted DB ({} bytes)", encrypted.len());

        // Upload meta.json
        let now = chrono::Utc::now().to_rfc3339();
        let meta = SyncMeta {
            fingerprint: self.encryption_service.fingerprint(),
            uploaded_at: now.clone(),
        };
        let meta_json = serde_json::to_vec_pretty(&meta)?;
        let meta_key = format!("bae/{}/meta.json", self.library_id);
        self.put_object(&meta_key, &meta_json).await?;

        // Clean up snapshot
        if let Err(e) = tokio::fs::remove_file(&snapshot_path).await {
            error!("Failed to clean up DB snapshot: {}", e);
        }

        info!("Cloud sync upload complete");
        Ok(now)
    }

    /// Download meta.json from S3.
    pub async fn download_meta(&self) -> Result<SyncMeta, CloudSyncError> {
        let key = format!("bae/{}/meta.json", self.library_id);
        let data = self
            .get_object(&key)
            .await
            .map_err(|_| CloudSyncError::NoMetadata)?;
        let meta: SyncMeta = serde_json::from_slice(&data)?;
        Ok(meta)
    }

    /// Validate that the local encryption key matches the one used for the cloud DB.
    pub async fn validate_key(&self) -> Result<(), CloudSyncError> {
        let meta = self.download_meta().await?;
        let local_fingerprint = self.encryption_service.fingerprint();

        if meta.fingerprint != local_fingerprint {
            return Err(CloudSyncError::FingerprintMismatch {
                expected: meta.fingerprint,
                actual: local_fingerprint,
            });
        }

        Ok(())
    }

    /// Download and decrypt the library DB from S3.
    pub async fn download_db(&self, target_path: &Path) -> Result<(), CloudSyncError> {
        let key = format!("bae/{}/library.db.enc", self.library_id);
        let encrypted = self.get_object(&key).await?;

        info!(
            "Downloaded encrypted DB ({} bytes), decrypting...",
            encrypted.len()
        );

        let decrypted = self.encryption_service.decrypt(&encrypted)?;

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(target_path, &decrypted).await?;

        info!(
            "Restored DB to {} ({} bytes)",
            target_path.display(),
            decrypted.len()
        );

        Ok(())
    }

    /// Upload all cover images (encrypted) to S3.
    pub async fn upload_covers(&self, covers_dir: &Path) -> Result<(), CloudSyncError> {
        if !covers_dir.exists() {
            debug!("No covers directory, skipping cover upload");
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(covers_dir).await?;
        let mut count = 0u32;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let data = tokio::fs::read(&path).await?;
            let encrypted = self.encryption_service.encrypt(&data);
            let key = format!("bae/{}/covers/{}", self.library_id, filename);
            self.put_object(&key, &encrypted).await?;
            count += 1;
        }

        info!("Uploaded {} cover images", count);
        Ok(())
    }

    /// Download and decrypt all cover images from S3.
    pub async fn download_covers(&self, covers_dir: &Path) -> Result<(), CloudSyncError> {
        tokio::fs::create_dir_all(covers_dir).await?;

        let prefix = format!("bae/{}/covers/", self.library_id);
        let keys = self.list_objects(&prefix).await?;

        let mut count = 0u32;
        for key in &keys {
            let filename = match key.strip_prefix(&prefix) {
                Some(f) if !f.is_empty() => f,
                _ => continue,
            };

            let encrypted = self.get_object(key).await?;
            let decrypted = self.encryption_service.decrypt(&encrypted)?;
            let target = covers_dir.join(filename);
            tokio::fs::write(&target, &decrypted).await?;
            count += 1;
        }

        info!("Downloaded {} cover images", count);
        Ok(())
    }

    async fn put_object(&self, key: &str, data: &[u8]) -> Result<(), CloudSyncError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.to_vec().into())
            .content_type("application/octet-stream")
            .send()
            .await
            .map_err(|e| CloudSyncError::S3(format!("Put object failed for {}: {}", key, e)))?;
        Ok(())
    }

    async fn get_object(&self, key: &str) -> Result<Vec<u8>, CloudSyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| CloudSyncError::S3(format!("Get object failed for {}: {}", key, e)))?;

        let data = response
            .body
            .collect()
            .await
            .map_err(|e| CloudSyncError::S3(format!("Failed to read body for {}: {}", key, e)))?
            .into_bytes()
            .to_vec();

        Ok(data)
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, CloudSyncError> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);

            if let Some(ref token) = continuation_token {
                request = request.continuation_token(token);
            }

            let response = request.send().await.map_err(|e| {
                CloudSyncError::S3(format!("List objects failed for {}: {}", prefix, e))
            })?;

            for obj in response.contents() {
                if let Some(key) = obj.key() {
                    keys.push(key.to_string());
                }
            }

            if response.is_truncated() == Some(true) {
                continuation_token = response.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(keys)
    }
}
