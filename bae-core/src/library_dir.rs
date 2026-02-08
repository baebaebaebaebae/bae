use std::ops::Deref;
use std::path::{Path, PathBuf};

/// Typed wrapper for a library directory path.
///
/// Centralizes the on-disk layout so callers use methods instead of
/// ad-hoc `path.join("covers")` etc.
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

    pub fn covers_dir(&self) -> PathBuf {
        self.path.join("covers")
    }

    pub fn artists_dir(&self) -> PathBuf {
        self.path.join("artists")
    }

    pub fn cover_path(&self, release_id: &str) -> PathBuf {
        self.covers_dir().join(release_id)
    }

    pub fn artist_image_path(&self, artist_id: &str) -> PathBuf {
        self.artists_dir().join(artist_id)
    }

    pub fn pending_deletions_path(&self) -> PathBuf {
        self.path.join("pending_deletions.json")
    }

    /// All asset directories that should be synced/created.
    pub fn asset_dirs(&self) -> Vec<PathBuf> {
        vec![self.covers_dir(), self.artists_dir()]
    }
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
