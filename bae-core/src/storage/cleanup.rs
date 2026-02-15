//! Deferred file cleanup after storage operations
//!
//! After a transfer or delete, old file locations are queued for
//! deletion via a manifest file. Cleanup runs on app startup and after
//! a delay post-operation, giving in-flight playback seeks and Subsonic
//! streams time to complete.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

/// Delay before cleaning up old files after a transfer.
/// Long enough for in-flight seeks/streams to complete.
const CLEANUP_DELAY: Duration = Duration::from_secs(30);

const MANIFEST_FILENAME: &str = "pending_deletions.json";

/// A file queued for deferred deletion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "location")]
pub enum PendingDeletion {
    #[serde(rename = "local")]
    Local { path: String },
}

/// Append pending deletions to the manifest file
pub async fn append_pending_deletions(
    library_path: &Path,
    deletions: &[PendingDeletion],
) -> Result<(), std::io::Error> {
    let manifest_path = library_path.join(MANIFEST_FILENAME);

    let mut existing = read_manifest(&manifest_path).await;
    existing.extend(deletions.iter().cloned());

    let json = serde_json::to_string_pretty(&existing).map_err(std::io::Error::other)?;
    tokio::fs::write(&manifest_path, json).await?;

    info!(
        "Queued {} deletions (total pending: {})",
        deletions.len(),
        existing.len()
    );

    Ok(())
}

/// Process all pending deletions from the manifest.
///
/// Called on app startup and after a delay post-transfer.
pub async fn process_pending_deletions(library_path: &Path) {
    let manifest_path = library_path.join(MANIFEST_FILENAME);
    let pending = read_manifest(&manifest_path).await;

    if pending.is_empty() {
        return;
    }

    info!("Processing {} pending file deletions", pending.len());

    let mut remaining = Vec::new();

    for deletion in pending {
        match &deletion {
            PendingDeletion::Local { path } => {
                match tokio::fs::remove_file(path).await {
                    Ok(_) => info!("Deleted local file: {}", path),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // Already gone, that's fine
                    }
                    Err(e) => {
                        warn!("Failed to delete {}: {}, will retry", path, e);
                        remaining.push(deletion);
                    }
                }
            }
        }
    }

    // Write back any that failed
    if remaining.is_empty() {
        let _ = tokio::fs::remove_file(&manifest_path).await;
    } else if let Ok(json) = serde_json::to_string_pretty(&remaining) {
        let _ = tokio::fs::write(&manifest_path, json).await;
    }
}

/// Schedule deferred cleanup after a transfer completes
pub fn schedule_cleanup(library_path: &Path) {
    let library_path = library_path.to_path_buf();
    tokio::spawn(async move {
        sleep(CLEANUP_DELAY).await;
        process_pending_deletions(&library_path).await;
    });
}

async fn read_manifest(manifest_path: &Path) -> Vec<PendingDeletion> {
    match tokio::fs::read_to_string(manifest_path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_append_creates_manifest() {
        let temp = TempDir::new().unwrap();
        let library_path = temp.path();

        let deletions = vec![PendingDeletion::Local {
            path: "/old/file1.flac".to_string(),
        }];

        append_pending_deletions(library_path, &deletions)
            .await
            .unwrap();

        let manifest_path = library_path.join(MANIFEST_FILENAME);
        assert!(manifest_path.exists());

        let contents = tokio::fs::read_to_string(&manifest_path).await.unwrap();
        let parsed: Vec<PendingDeletion> = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[tokio::test]
    async fn test_append_accumulates() {
        let temp = TempDir::new().unwrap();
        let library_path = temp.path();

        append_pending_deletions(
            library_path,
            &[PendingDeletion::Local {
                path: "/old/a.flac".to_string(),
            }],
        )
        .await
        .unwrap();

        append_pending_deletions(
            library_path,
            &[PendingDeletion::Local {
                path: "/old/b.flac".to_string(),
            }],
        )
        .await
        .unwrap();

        let manifest_path = library_path.join(MANIFEST_FILENAME);
        let contents = tokio::fs::read_to_string(&manifest_path).await.unwrap();
        let parsed: Vec<PendingDeletion> = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[tokio::test]
    async fn test_process_deletes_local_files() {
        let temp = TempDir::new().unwrap();
        let library_path = temp.path();

        // Create files to delete
        let file1 = temp.path().join("file1.flac");
        let file2 = temp.path().join("file2.flac");
        tokio::fs::write(&file1, b"data1").await.unwrap();
        tokio::fs::write(&file2, b"data2").await.unwrap();

        append_pending_deletions(
            library_path,
            &[
                PendingDeletion::Local {
                    path: file1.display().to_string(),
                },
                PendingDeletion::Local {
                    path: file2.display().to_string(),
                },
            ],
        )
        .await
        .unwrap();

        process_pending_deletions(library_path).await;

        assert!(!file1.exists());
        assert!(!file2.exists());
        // Manifest should be cleaned up
        assert!(!library_path.join(MANIFEST_FILENAME).exists());
    }

    #[tokio::test]
    async fn test_process_not_found_files_are_silently_removed() {
        let temp = TempDir::new().unwrap();
        let library_path = temp.path();

        // Queue a file that doesn't exist
        append_pending_deletions(
            library_path,
            &[PendingDeletion::Local {
                path: "/nonexistent/file.flac".to_string(),
            }],
        )
        .await
        .unwrap();

        process_pending_deletions(library_path).await;

        // Manifest should be cleaned up (not-found is not a retry)
        assert!(!library_path.join(MANIFEST_FILENAME).exists());
    }

    #[tokio::test]
    async fn test_process_with_no_manifest_is_noop() {
        let temp = TempDir::new().unwrap();
        // No manifest file exists -- should not panic
        process_pending_deletions(temp.path()).await;
    }

    #[tokio::test]
    async fn test_serde_roundtrip() {
        let deletions = vec![PendingDeletion::Local {
            path: "/some/path.flac".to_string(),
        }];

        let json = serde_json::to_string_pretty(&deletions).unwrap();
        let parsed: Vec<PendingDeletion> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 1);
        match &parsed[0] {
            PendingDeletion::Local { path } => assert_eq!(path, "/some/path.flac"),
        }
    }
}
