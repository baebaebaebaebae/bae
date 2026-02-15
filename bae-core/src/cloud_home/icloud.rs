//! iCloud Drive-backed `CloudHome` implementation.
//!
//! Unlike other backends that use REST APIs, iCloud Drive is a local directory
//! that macOS syncs automatically. All operations are standard filesystem I/O.
//!
//! The container path is detected in bae-desktop via `NSFileManager` and passed
//! here as a `PathBuf`. This module has no macOS-specific dependencies.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{CloudHome, CloudHomeError, JoinInfo};

/// iCloud Drive-backed cloud home.
///
/// Wraps a local directory inside the app's ubiquity container. macOS handles
/// syncing to/from iCloud transparently. Keys like `changes/dev1/42.enc` map
/// directly to filesystem paths with real directories.
///
/// NOTE: The app needs `com.apple.developer.icloud-container-identifiers` in its
/// entitlements for the ubiquity container to be available.
pub struct ICloudCloudHome {
    root: PathBuf,
}

impl ICloudCloudHome {
    /// Create a new iCloud cloud home rooted at the given directory.
    ///
    /// The path should be an already-detected ubiquity container path
    /// (e.g. from `NSFileManager.URLForUbiquityContainerIdentifier`).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve a key to a full filesystem path.
    fn path_for_key(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

#[async_trait]
impl CloudHome for ICloudCloudHome {
    async fn write(&self, key: &str, data: Vec<u8>) -> Result<(), CloudHomeError> {
        let path = self.path_for_key(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
        let path = self.path_for_key(key);
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CloudHomeError::NotFound(key.to_string())
            } else {
                CloudHomeError::Io(e)
            }
        })
    }

    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let path = self.path_for_key(key);
        let mut file = tokio::fs::File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CloudHomeError::NotFound(key.to_string())
            } else {
                CloudHomeError::Io(e)
            }
        })?;

        file.seek(std::io::SeekFrom::Start(start)).await?;
        let len = end.saturating_sub(start) as usize;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).await?;
        Ok(buf)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
        let base = self.path_for_key(prefix);
        match tokio::fs::metadata(&base).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(CloudHomeError::Io(e)),
            Ok(m) if !m.is_dir() => return Ok(vec![]),
            Ok(_) => {}
        }

        let mut keys = Vec::new();
        let mut stack = vec![base];

        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    stack.push(entry.path());
                } else if file_type.is_file() {
                    // Return path relative to root, using forward slashes (key format)
                    if let Ok(relative) = entry.path().strip_prefix(&self.root) {
                        let key = relative.to_string_lossy().replace('\\', "/");
                        keys.push(key);
                    }
                }
            }
        }

        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        let path = self.path_for_key(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CloudHomeError::Io(e)),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError> {
        let path = self.path_for_key(key);
        let meta = tokio::fs::metadata(&path).await;
        match meta {
            Ok(m) => Ok(m.is_file()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(CloudHomeError::Io(e)),
        }
    }

    async fn grant_access(&self, _member_id: &str) -> Result<JoinInfo, CloudHomeError> {
        Err(CloudHomeError::Storage(
            "iCloud sharing is managed through macOS System Settings".to_string(),
        ))
    }

    async fn revoke_access(&self, _member_id: &str) -> Result<(), CloudHomeError> {
        Err(CloudHomeError::Storage(
            "iCloud sharing is managed through macOS System Settings".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_cloud_home(tmp: &TempDir) -> ICloudCloudHome {
        ICloudCloudHome::new(tmp.path().to_path_buf())
    }

    #[tokio::test]
    async fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        ch.write("test.bin", b"hello world".to_vec()).await.unwrap();
        let data = ch.read("test.bin").await.unwrap();
        assert_eq!(data, b"hello world");
    }

    #[tokio::test]
    async fn read_nonexistent_returns_not_found() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        let err = ch.read("nonexistent.bin").await.unwrap_err();
        assert!(matches!(err, CloudHomeError::NotFound(_)));
    }

    #[tokio::test]
    async fn read_range_works() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        ch.write("range.bin", b"0123456789".to_vec()).await.unwrap();
        let data = ch.read_range("range.bin", 3, 7).await.unwrap();
        assert_eq!(data, b"3456");
    }

    #[tokio::test]
    async fn list_with_prefix() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        ch.write("changes/dev1/1.enc", b"a".to_vec()).await.unwrap();
        ch.write("changes/dev1/2.enc", b"b".to_vec()).await.unwrap();
        ch.write("changes/dev2/1.enc", b"c".to_vec()).await.unwrap();
        ch.write("snapshot.db", b"d".to_vec()).await.unwrap();

        let all = ch.list("changes").await.unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&"changes/dev1/1.enc".to_string()));
        assert!(all.contains(&"changes/dev1/2.enc".to_string()));
        assert!(all.contains(&"changes/dev2/1.enc".to_string()));

        let dev1 = ch.list("changes/dev1").await.unwrap();
        assert_eq!(dev1.len(), 2);

        // Listing a nonexistent prefix returns empty
        let empty = ch.list("nonexistent").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn delete_idempotent() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        ch.write("to-delete.bin", b"data".to_vec()).await.unwrap();
        ch.delete("to-delete.bin").await.unwrap();
        assert!(!ch.exists("to-delete.bin").await.unwrap());

        // Deleting again is fine
        ch.delete("to-delete.bin").await.unwrap();
    }

    #[tokio::test]
    async fn exists_checks() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        assert!(!ch.exists("nope.bin").await.unwrap());

        ch.write("yep.bin", b"data".to_vec()).await.unwrap();
        assert!(ch.exists("yep.bin").await.unwrap());

        // A directory should not be considered as "exists"
        tokio::fs::create_dir_all(tmp.path().join("a-dir"))
            .await
            .unwrap();
        assert!(!ch.exists("a-dir").await.unwrap());
    }

    #[tokio::test]
    async fn nested_key_creates_directories() {
        let tmp = TempDir::new().unwrap();
        let ch = make_cloud_home(&tmp);

        ch.write("a/b/c/deep.bin", b"deep".to_vec()).await.unwrap();
        let data = ch.read("a/b/c/deep.bin").await.unwrap();
        assert_eq!(data, b"deep");
        assert!(tmp.path().join("a/b/c").is_dir());
    }
}
