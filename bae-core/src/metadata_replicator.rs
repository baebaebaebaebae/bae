//! Metadata replication: sync DB snapshot, images, and manifest to all non-home profiles.
//!
//! Replaces CloudSyncService. Each profile gets a full copy of the library metadata.

use crate::cloud_storage::{s3_config_from_profile, CloudStorage, S3CloudStorage};
use crate::db::{DbStorageProfile, StorageLocation};
use crate::encryption::EncryptionService;
use crate::keys::KeyService;
use crate::library::SharedLibraryManager;
use crate::library_dir::{LibraryDir, Manifest};
use std::path::Path;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Error, Debug)]
pub enum ReplicationError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] crate::encryption::EncryptionError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No S3 credentials for profile {0}")]
    MissingCredentials(String),
}

pub struct MetadataReplicator {
    library_manager: SharedLibraryManager,
    library_dir: LibraryDir,
    key_service: KeyService,
    encryption_service: Option<EncryptionService>,
    library_id: String,
    library_name: Option<String>,
}

impl MetadataReplicator {
    pub fn new(
        library_manager: SharedLibraryManager,
        library_dir: LibraryDir,
        key_service: KeyService,
        encryption_service: Option<EncryptionService>,
        library_id: String,
        library_name: Option<String>,
    ) -> Self {
        Self {
            library_manager,
            library_dir,
            key_service,
            encryption_service,
            library_id,
            library_name,
        }
    }

    /// Sync metadata (DB, images, manifest) to all non-home profiles.
    /// Errors on individual profiles are logged and skipped.
    pub async fn sync_all(&self) -> Result<(), ReplicationError> {
        let profiles = self
            .library_manager
            .database()
            .get_replica_profiles()
            .await?;

        if profiles.is_empty() {
            info!("No replica profiles to sync");
            return Ok(());
        }

        // Create DB snapshot
        let snapshot_path = self.library_dir.db_path().with_extension("db.snapshot");
        self.library_manager
            .database()
            .vacuum_into(snapshot_path.to_str().unwrap())
            .await?;

        info!("Syncing metadata to {} replica profile(s)", profiles.len());

        let now = chrono::Utc::now().to_rfc3339();

        for profile in &profiles {
            if let Err(e) = self.sync_profile(profile, &snapshot_path, &now).await {
                error!(
                    "Failed to sync to profile '{}' ({}): {}",
                    profile.name, profile.id, e
                );
            }
        }

        // Clean up snapshot
        if let Err(e) = tokio::fs::remove_file(&snapshot_path).await {
            warn!("Failed to clean up DB snapshot: {}", e);
        }

        // Update home manifest with replicated_at timestamp
        self.write_home_manifest(Some(&now)).await?;

        info!("Metadata replication complete");
        Ok(())
    }

    async fn sync_profile(
        &self,
        profile: &DbStorageProfile,
        snapshot_path: &Path,
        now: &str,
    ) -> Result<(), ReplicationError> {
        match profile.location {
            StorageLocation::Local => self.sync_local_profile(profile, snapshot_path, now).await,
            StorageLocation::Cloud => self.sync_cloud_profile(profile, snapshot_path, now).await,
        }
    }

    async fn sync_local_profile(
        &self,
        profile: &DbStorageProfile,
        snapshot_path: &Path,
        now: &str,
    ) -> Result<(), ReplicationError> {
        let target_dir = Path::new(&profile.location_path);
        tokio::fs::create_dir_all(target_dir).await?;

        // Copy DB snapshot
        let target_db = target_dir.join("library.db");
        tokio::fs::copy(snapshot_path, &target_db).await?;

        info!("Copied DB snapshot to {}", target_db.display());

        // Copy images directory (recursive)
        let source_images = self.library_dir.images_dir();
        if source_images.exists() {
            let target_images = target_dir.join("images");
            copy_dir_recursive(&source_images, &target_images).await?;

            info!("Copied images to {}", target_images.display());
        }

        // Write manifest
        let manifest = self.build_manifest(profile, Some(now));
        let manifest_path = target_dir.join("manifest.json");
        let json = serde_json::to_string_pretty(&manifest)?;
        tokio::fs::write(&manifest_path, json).await?;

        info!("Synced local profile '{}'", profile.name);
        Ok(())
    }

    async fn sync_cloud_profile(
        &self,
        profile: &DbStorageProfile,
        snapshot_path: &Path,
        now: &str,
    ) -> Result<(), ReplicationError> {
        let encryption = match &self.encryption_service {
            Some(enc) => enc,
            None => {
                warn!(
                    "Skipping cloud profile '{}': no encryption service",
                    profile.name
                );
                return Ok(());
            }
        };

        let s3_config = s3_config_from_profile(profile, &self.key_service)
            .ok_or_else(|| ReplicationError::MissingCredentials(profile.name.clone()))?;

        let storage = S3CloudStorage::new_with_bucket_creation(s3_config, false)
            .await
            .map_err(|e| ReplicationError::S3(e.to_string()))?;

        // Encrypt and upload DB snapshot
        let snapshot_data = tokio::fs::read(snapshot_path).await?;
        let encrypted_db = encryption.encrypt(&snapshot_data);
        storage
            .upload("library.db.enc", &encrypted_db)
            .await
            .map_err(|e| ReplicationError::S3(e.to_string()))?;

        info!(
            "Uploaded encrypted DB ({} bytes) to '{}'",
            encrypted_db.len(),
            profile.name
        );

        // Encrypt and upload images
        let images_dir = self.library_dir.images_dir();
        if images_dir.exists() {
            upload_images_encrypted(&storage, encryption, &images_dir).await?;
        }

        // Encrypt and upload manifest
        let manifest = self.build_manifest(profile, Some(now));
        let manifest_json = serde_json::to_vec_pretty(&manifest)?;
        let encrypted_manifest = encryption.encrypt(&manifest_json);
        storage
            .upload("manifest.json.enc", &encrypted_manifest)
            .await
            .map_err(|e| ReplicationError::S3(e.to_string()))?;

        info!("Synced cloud profile '{}'", profile.name);
        Ok(())
    }

    fn build_manifest(&self, profile: &DbStorageProfile, replicated_at: Option<&str>) -> Manifest {
        let encryption_key_fingerprint = self.encryption_service.as_ref().map(|e| e.fingerprint());

        Manifest {
            library_id: self.library_id.clone(),
            library_name: self.library_name.clone(),
            encryption_key_fingerprint,
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            replicated_at: replicated_at.map(|s| s.to_string()),
        }
    }

    /// Write manifest.json at the library home directory.
    pub async fn write_home_manifest(
        &self,
        replicated_at: Option<&str>,
    ) -> Result<(), ReplicationError> {
        let profiles = self
            .library_manager
            .database()
            .get_all_storage_profiles()
            .await?;

        let home_profile = profiles.iter().find(|p| p.is_home);
        let encryption_key_fingerprint = self.encryption_service.as_ref().map(|e| e.fingerprint());

        let manifest = Manifest {
            library_id: self.library_id.clone(),
            library_name: self.library_name.clone(),
            encryption_key_fingerprint,
            profile_id: home_profile.map(|p| p.id.clone()).unwrap_or_default(),
            profile_name: home_profile
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Local".to_string()),
            replicated_at: replicated_at.map(|s| s.to_string()),
        };

        let json = serde_json::to_string_pretty(&manifest)?;
        tokio::fs::write(self.library_dir.manifest_path(), json).await?;

        Ok(())
    }
}

/// Recursively copy a directory tree.
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    tokio::fs::create_dir_all(dst).await?;

    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            if let Some(parent) = dst_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

/// Upload all images from a directory tree, encrypting each one.
async fn upload_images_encrypted(
    storage: &S3CloudStorage,
    encryption: &EncryptionService,
    images_dir: &Path,
) -> Result<(), ReplicationError> {
    let mut count = 0u32;
    let mut stack = vec![images_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(images_dir)
                    .map_err(std::io::Error::other)?;

                let data = tokio::fs::read(&path).await?;
                let encrypted = encryption.encrypt(&data);
                let key = format!("images/{}", rel.to_string_lossy());
                storage
                    .upload(&key, &encrypted)
                    .await
                    .map_err(|e| ReplicationError::S3(e.to_string()))?;
                count += 1;
            }
        }
    }

    info!("Uploaded {} encrypted images", count);
    Ok(())
}
