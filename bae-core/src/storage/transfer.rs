//! Transfer service — moves releases between storage profiles
//!
//! Orchestrates reading files from the source location, writing them to the
//! destination, updating DB records atomically, and queuing old files for
//! deferred cleanup.

use crate::cloud_storage::CloudStorage;
use crate::db::{DbFile, DbReleaseStorage, DbStorageProfile, EncryptionScheme, StorageLocation};
use crate::encryption::EncryptionService;
use crate::keys::KeyService;
use crate::library::SharedLibraryManager;
use crate::library_dir::LibraryDir;
use crate::storage::{create_storage_reader, ReleaseStorageImpl};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use super::cleanup::PendingDeletion;

/// Progress updates emitted during a transfer
#[derive(Debug, Clone)]
pub enum TransferProgress {
    /// Transfer started
    Started {
        release_id: String,
        total_files: usize,
    },
    /// A file is being transferred
    FileProgress {
        release_id: String,
        file_index: usize,
        total_files: usize,
        filename: String,
        percent: u8,
    },
    /// Transfer completed
    Complete { release_id: String },
    /// Transfer failed
    Failed { release_id: String, error: String },
}

/// Where to transfer a release
pub enum TransferTarget {
    /// Move to a managed storage profile
    Profile(DbStorageProfile),
    /// Eject to a user-chosen local folder (removes from managed storage)
    Eject(PathBuf),
}

/// Transfer service that moves releases between storage profiles
pub struct TransferService {
    library_manager: SharedLibraryManager,
    encryption_service: Option<EncryptionService>,
    library_dir: LibraryDir,
    key_service: KeyService,
    #[cfg(feature = "test-utils")]
    injected_source_cloud: Option<Arc<dyn CloudStorage>>,
    #[cfg(feature = "test-utils")]
    injected_dest_cloud: Option<Arc<dyn CloudStorage>>,
}

impl TransferService {
    pub fn new(
        library_manager: SharedLibraryManager,
        encryption_service: Option<EncryptionService>,
        library_dir: LibraryDir,
        key_service: KeyService,
    ) -> Self {
        Self {
            library_manager,
            encryption_service,
            library_dir,
            key_service,
            #[cfg(feature = "test-utils")]
            injected_source_cloud: None,
            #[cfg(feature = "test-utils")]
            injected_dest_cloud: None,
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn with_injected_clouds(
        mut self,
        source: Option<Arc<dyn CloudStorage>>,
        dest: Option<Arc<dyn CloudStorage>>,
    ) -> Self {
        self.injected_source_cloud = source;
        self.injected_dest_cloud = dest;
        self
    }

    /// Transfer a release to a new storage target.
    ///
    /// Returns a receiver for progress updates.
    pub fn transfer(
        &self,
        release_id: String,
        target: TransferTarget,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();
        let encryption_service = self.encryption_service.clone();
        let library_dir = self.library_dir.clone();
        let key_service = self.key_service.clone();

        #[cfg(feature = "test-utils")]
        let injected_source = self.injected_source_cloud.clone();
        #[cfg(feature = "test-utils")]
        let injected_dest = self.injected_dest_cloud.clone();

        tokio::spawn(async move {
            let result = do_transfer(
                &release_id,
                target,
                &library_manager,
                encryption_service.as_ref(),
                &library_dir,
                &key_service,
                &tx,
                #[cfg(feature = "test-utils")]
                injected_source,
                #[cfg(feature = "test-utils")]
                injected_dest,
            )
            .await;

            if let Err(e) = result {
                error!("Transfer failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        rx
    }
}

async fn do_transfer(
    release_id: &str,
    target: TransferTarget,
    library_manager: &SharedLibraryManager,
    encryption_service: Option<&EncryptionService>,
    library_path: &Path,
    key_service: &KeyService,
    tx: &mpsc::UnboundedSender<TransferProgress>,
    #[cfg(feature = "test-utils")] injected_source: Option<Arc<dyn CloudStorage>>,
    #[cfg(feature = "test-utils")] injected_dest: Option<Arc<dyn CloudStorage>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager.get();

    // 1. Get current files and source profile
    let old_files = mgr.get_files_for_release(release_id).await?;
    let source_profile = mgr.get_storage_profile_for_release(release_id).await?;

    if old_files.is_empty() {
        return Err("Release has no files".into());
    }

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: old_files.len(),
    });

    info!(
        "Starting transfer for release {} ({} files)",
        release_id,
        old_files.len()
    );

    // 2. Read all files from source, decrypting if needed
    let source_reader: Option<Arc<dyn CloudStorage>> = if let Some(ref profile) = source_profile {
        #[cfg(feature = "test-utils")]
        if let Some(ref cloud) = injected_source {
            Some(cloud.clone())
        } else {
            Some(create_storage_reader(profile, key_service).await?)
        }
        #[cfg(not(feature = "test-utils"))]
        Some(create_storage_reader(profile, key_service).await?)
    } else {
        None
    };

    let source_encrypted = source_profile
        .as_ref()
        .map(|p| p.encrypted)
        .unwrap_or(false);

    // Read source files into memory with decryption
    let mut file_data: Vec<(String, Vec<u8>)> = Vec::with_capacity(old_files.len());
    for (i, file) in old_files.iter().enumerate() {
        let _ = tx.send(TransferProgress::FileProgress {
            release_id: release_id.to_string(),
            file_index: i,
            total_files: old_files.len(),
            filename: file.original_filename.clone(),
            percent: 0,
        });

        let raw_data = read_file_data(file, source_reader.as_ref()).await?;

        let data = if source_encrypted {
            let enc =
                encryption_service.ok_or("Encryption service required for encrypted source")?;
            enc.decrypt(&raw_data)?
        } else {
            raw_data
        };

        file_data.push((file.original_filename.clone(), data));

        let _ = tx.send(TransferProgress::FileProgress {
            release_id: release_id.to_string(),
            file_index: i,
            total_files: old_files.len(),
            filename: file.original_filename.clone(),
            percent: 50,
        });
    }

    // 3. Write files to destination, updating existing records in place
    match &target {
        TransferTarget::Profile(dest_profile) => {
            let database = Arc::new(mgr.database().clone());

            #[cfg(feature = "test-utils")]
            let storage = if let Some(ref cloud) = injected_dest {
                ReleaseStorageImpl::with_cloud(
                    dest_profile.clone(),
                    encryption_service.cloned(),
                    cloud.clone(),
                    database,
                )
            } else {
                ReleaseStorageImpl::from_profile(
                    dest_profile.clone(),
                    encryption_service.cloned(),
                    database,
                    key_service,
                )
                .await?
            };

            #[cfg(not(feature = "test-utils"))]
            let storage = ReleaseStorageImpl::from_profile(
                dest_profile.clone(),
                encryption_service.cloned(),
                database,
                key_service,
            )
            .await?;

            // Write all bytes to storage first, collecting new paths
            let mut file_updates: Vec<(String, Option<Vec<u8>>)> =
                Vec::with_capacity(old_files.len());

            for (i, file) in old_files.iter().enumerate() {
                let data = &file_data[i].1;
                let total_files = old_files.len();
                let tx_clone = tx.clone();
                let rid = release_id.to_string();
                let fname = file.original_filename.clone();

                let (new_source_path, nonce) = storage
                    .store_bytes(
                        &file.id,
                        data,
                        Box::new(move |bytes_written, total_bytes| {
                            let percent = if total_bytes > 0 {
                                ((bytes_written as f64 / total_bytes as f64) * 50.0 + 50.0) as u8
                            } else {
                                100
                            };
                            let _ = tx_clone.send(TransferProgress::FileProgress {
                                release_id: rid.clone(),
                                file_index: i,
                                total_files,
                                filename: fname.clone(),
                                percent,
                            });
                        }),
                    )
                    .await?;

                file_updates.push((new_source_path, nonce));
            }

            // Batch-update all file records + release storage link atomically
            let updates: Vec<(&str, &str, Option<&[u8]>, &str)> = old_files
                .iter()
                .zip(file_updates.iter())
                .map(|(file, (path, nonce))| {
                    (
                        file.id.as_str(),
                        path.as_str(),
                        nonce.as_deref(),
                        file.encryption_scheme.as_str(),
                    )
                })
                .collect();
            mgr.batch_update_file_source_paths(&updates).await?;

            let release_storage = DbReleaseStorage::new(release_id, &dest_profile.id);
            mgr.set_release_storage(&release_storage).await?;
        }
        TransferTarget::Eject(target_dir) => {
            tokio::fs::create_dir_all(target_dir).await?;

            // Write all files to target directory first
            let mut dest_paths: Vec<String> = Vec::with_capacity(old_files.len());

            for (i, file) in old_files.iter().enumerate() {
                let data = &file_data[i].1;
                let dest_path = target_dir.join(&file.original_filename);
                tokio::fs::write(&dest_path, data).await?;
                dest_paths.push(dest_path.display().to_string());

                let _ = tx.send(TransferProgress::FileProgress {
                    release_id: release_id.to_string(),
                    file_index: i,
                    total_files: file_data.len(),
                    filename: file.original_filename.clone(),
                    percent: 100,
                });
            }

            // Batch-update all file records atomically
            let scheme = EncryptionScheme::Master.as_str();
            let updates: Vec<(&str, &str, Option<&[u8]>, &str)> = old_files
                .iter()
                .zip(dest_paths.iter())
                .map(|(file, path)| (file.id.as_str(), path.as_str(), None, scheme))
                .collect();
            mgr.batch_update_file_source_paths(&updates).await?;

            mgr.delete_release_storage(release_id).await?;
        }
    }

    // 4. Queue old files for deferred deletion
    let pending: Vec<PendingDeletion> = old_files
        .iter()
        .filter_map(|f| {
            let source_path = f.source_path.as_ref()?;
            if let Some(ref profile) = source_profile {
                if profile.location == StorageLocation::Cloud {
                    Some(PendingDeletion::Cloud {
                        profile_id: profile.id.clone(),
                        key: source_path.clone(),
                    })
                } else {
                    Some(PendingDeletion::Local {
                        path: source_path.clone(),
                    })
                }
            } else {
                // Self-managed files: don't delete originals
                None
            }
        })
        .collect();

    if !pending.is_empty() {
        if let Err(e) = super::cleanup::append_pending_deletions(library_path, &pending).await {
            warn!("Failed to queue deferred deletions: {}", e);
        }
    }

    info!("Transfer complete for release {}", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Read file data from its source location
async fn read_file_data(
    file: &DbFile,
    source_reader: Option<&Arc<dyn CloudStorage>>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let source_path = file
        .source_path
        .as_ref()
        .ok_or_else(|| format!("File {} has no source_path", file.id))?;

    if let Some(reader) = source_reader {
        Ok(reader.download(source_path).await?)
    } else {
        // Self-managed: read from disk directly
        Ok(tokio::fs::read(source_path).await?)
    }
}
