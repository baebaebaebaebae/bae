//! Transfer service — moves releases between storage modes
//!
//! Orchestrates reading files from the source location, writing them to the
//! destination, updating DB records, and queuing old files for deferred cleanup.

use crate::db::DbFile;
use crate::encryption::EncryptionService;
use crate::library::SharedLibraryManager;
use crate::library_dir::LibraryDir;
use crate::storage::ReleaseStorageImpl;
use std::path::{Path, PathBuf};
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
    /// Move files into managed local storage
    ManagedLocal,
    /// Eject to a user-chosen local folder (removes from managed storage)
    Eject(PathBuf),
}

/// Transfer service that moves releases between storage modes
pub struct TransferService {
    library_manager: SharedLibraryManager,
    encryption_service: Option<EncryptionService>,
    library_dir: LibraryDir,
}

impl TransferService {
    pub fn new(
        library_manager: SharedLibraryManager,
        encryption_service: Option<EncryptionService>,
        library_dir: LibraryDir,
    ) -> Self {
        Self {
            library_manager,
            encryption_service,
            library_dir,
        }
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

        tokio::spawn(async move {
            let result = do_transfer(
                &release_id,
                target,
                &library_manager,
                encryption_service.as_ref(),
                &library_dir,
                &tx,
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
    library_dir: &LibraryDir,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager.get();

    let release = mgr
        .database()
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;

    let old_files = mgr.get_files_for_release(release_id).await?;

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

    // Read all files from source, decrypting if needed
    let source_encrypted = release.managed_locally && encryption_service.is_some();

    let mut file_data: Vec<(String, Vec<u8>)> = Vec::with_capacity(old_files.len());
    for (i, file) in old_files.iter().enumerate() {
        let _ = tx.send(TransferProgress::FileProgress {
            release_id: release_id.to_string(),
            file_index: i,
            total_files: old_files.len(),
            filename: file.original_filename.clone(),
            percent: 0,
        });

        let raw_data = read_file_data(file, &release, library_dir).await?;

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

    // Queue old managed-local files for deferred deletion
    if release.managed_locally {
        let pending: Vec<PendingDeletion> = old_files
            .iter()
            .map(|f| PendingDeletion::Local {
                path: f.local_storage_path(library_dir).display().to_string(),
            })
            .collect();

        if !pending.is_empty() {
            if let Err(e) =
                super::cleanup::append_pending_deletions(library_dir.as_ref(), &pending).await
            {
                warn!("Failed to queue deferred deletions: {}", e);
            }
        }
    }

    // Write files to destination
    match &target {
        TransferTarget::ManagedLocal => {
            let database = std::sync::Arc::new(mgr.database().clone());
            let storage = ReleaseStorageImpl::new_local(
                library_dir.clone(),
                encryption_service.cloned(),
                database,
            );

            for (i, file) in old_files.iter().enumerate() {
                let data = &file_data[i].1;
                let total_files = old_files.len();
                let tx_clone = tx.clone();
                let rid = release_id.to_string();
                let fname = file.original_filename.clone();

                let nonce = storage
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

                // Update encryption nonce if newly encrypted
                if let Some(nonce) = nonce {
                    mgr.database()
                        .update_file_encryption_nonce(&file.id, &nonce)
                        .await?;
                }
            }

            mgr.database()
                .set_release_managed_locally(release_id, true)
                .await?;

            // Clear unmanaged_path since files are now managed
            // (set_release_managed_locally already handles this via SQL)
        }
        TransferTarget::Eject(target_dir) => {
            tokio::fs::create_dir_all(target_dir).await?;

            for (i, file) in old_files.iter().enumerate() {
                let data = &file_data[i].1;
                let dest_path = target_dir.join(&file.original_filename);
                tokio::fs::write(&dest_path, data).await?;

                let _ = tx.send(TransferProgress::FileProgress {
                    release_id: release_id.to_string(),
                    file_index: i,
                    total_files: file_data.len(),
                    filename: file.original_filename.clone(),
                    percent: 100,
                });
            }

            let unmanaged_path = target_dir
                .to_str()
                .ok_or("Cannot convert target dir to string")?;
            mgr.database()
                .set_release_unmanaged(release_id, unmanaged_path)
                .await?;
            mgr.database()
                .set_release_managed_locally(release_id, false)
                .await?;
        }
    }

    info!("Transfer complete for release {}", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Read file data from its source location based on release storage flags
async fn read_file_data(
    file: &DbFile,
    release: &crate::db::DbRelease,
    library_dir: &LibraryDir,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    if release.managed_locally {
        // Read from managed local storage path
        let path = file.local_storage_path(library_dir);
        Ok(tokio::fs::read(&path).await?)
    } else if let Some(ref unmanaged_path) = release.unmanaged_path {
        // Read from unmanaged location
        let path = Path::new(unmanaged_path).join(&file.original_filename);
        Ok(tokio::fs::read(&path).await?)
    } else {
        Err(format!("File {} has no readable location", file.id).into())
    }
}
