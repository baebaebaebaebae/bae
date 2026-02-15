use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// Typed wrapper for a library directory path.
///
/// Centralizes the on-disk layout so callers use methods instead of
/// ad-hoc `path.join("images")` etc.
#[derive(Clone, Debug)]
pub struct LibraryDir {
    path: PathBuf,
}

impl LibraryDir {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn db_path(&self) -> PathBuf {
        self.path.join("library.db")
    }

    pub fn config_path(&self) -> PathBuf {
        self.path.join("config.yaml")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.path.join("images")
    }

    /// Hash-based image path: `images/{ab}/{cd}/{id}`
    pub fn image_path(&self, id: &str) -> PathBuf {
        let hex = id.replace('-', "");
        self.images_dir().join(&hex[..2]).join(&hex[2..4]).join(id)
    }

    pub fn storage_dir(&self) -> PathBuf {
        self.path.join("storage")
    }

    /// Hash-based storage path: `storage/{ab}/{cd}/{file_id}`
    pub fn storage_file_path(&self, file_id: &str) -> PathBuf {
        let hex = file_id.replace('-', "");
        self.storage_dir()
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(file_id)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.path.join("manifest.json")
    }

    pub fn pending_deletions_path(&self) -> PathBuf {
        self.path.join("pending_deletions.json")
    }

    /// All asset directories that should be synced/created.
    pub fn asset_dirs(&self) -> Vec<PathBuf> {
        vec![self.images_dir(), self.storage_dir()]
    }
}

/// Manifest identifying a library.
/// Present at the root of the library directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub library_id: String,
    pub library_name: Option<String>,
    pub encryption_key_fingerprint: Option<String>,
}

impl Deref for LibraryDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for LibraryDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl From<PathBuf> for LibraryDir {
    fn from(path: PathBuf) -> Self {
        Self { path }
    }
}
