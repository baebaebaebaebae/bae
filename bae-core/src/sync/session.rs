/// Production session management for sync.
///
/// `SyncSession` wraps the low-level FFI `Session` and attaches exactly the
/// 11 synced tables. It provides a clean start/changeset/end lifecycle.
use super::session_ext::{Changeset, Session};

/// The 11 tables that participate in changeset sync.
/// Device-specific tables (storage_profiles, release_storage, torrents,
/// torrent_piece_mappings, imports) are NOT attached.
pub const SYNCED_TABLES: &[&str] = &[
    "artists",
    "albums",
    "album_discogs",
    "album_musicbrainz",
    "album_artists",
    "releases",
    "tracks",
    "track_artists",
    "release_files",
    "audio_formats",
    "library_images",
];

/// A sync session that tracks changes to all synced tables on a single connection.
///
/// Lifecycle:
/// 1. `SyncSession::start(db)` -- creates and attaches
/// 2. App writes normally through the connection
/// 3. `session.changeset()` -- grabs the binary diff (None if no changes)
/// 4. Session is dropped (or explicitly ended by dropping)
///
/// The session must be dropped before applying incoming changesets to avoid
/// contaminating the next outgoing changeset with other devices' changes.
pub struct SyncSession {
    session: Session,
}

impl SyncSession {
    /// Create a new sync session on the given raw sqlite3 connection,
    /// attaching all synced tables.
    ///
    /// # Safety
    /// `db` must be a valid, open sqlite3 connection pointer. The session
    /// must be dropped before the connection is closed.
    pub unsafe fn start(db: *mut libsqlite3_sys::sqlite3) -> Result<Self, SyncError> {
        let session = Session::new(db).map_err(SyncError::SessionCreate)?;

        for table in SYNCED_TABLES {
            session
                .attach(Some(table))
                .map_err(|rc| SyncError::SessionAttach(table.to_string(), rc))?;
        }

        Ok(SyncSession { session })
    }

    /// Grab the binary changeset of all changes since the session started.
    /// Returns `None` if no changes were made (avoids pushing empty changesets).
    pub fn changeset(&self) -> Result<Option<Changeset>, SyncError> {
        let cs = self
            .session
            .changeset()
            .map_err(SyncError::ChangesetExtract)?;

        if cs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(cs))
        }
    }
}

#[derive(Debug)]
pub enum SyncError {
    /// Failed to create a session (sqlite3 error code).
    SessionCreate(i32),
    /// Failed to attach a table (table name, sqlite3 error code).
    SessionAttach(String, i32),
    /// Failed to extract a changeset (sqlite3 error code).
    ChangesetExtract(i32),
    /// Failed to apply a changeset (sqlite3 error code).
    ChangesetApply(i32),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::SessionCreate(rc) => write!(f, "session create failed (rc={rc})"),
            SyncError::SessionAttach(table, rc) => {
                write!(f, "session attach failed for {table} (rc={rc})")
            }
            SyncError::ChangesetExtract(rc) => write!(f, "changeset extract failed (rc={rc})"),
            SyncError::ChangesetApply(rc) => write!(f, "changeset apply failed (rc={rc})"),
        }
    }
}

impl std::error::Error for SyncError {}
