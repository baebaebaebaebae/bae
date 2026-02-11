use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorageError;
use crate::db::{
    Database, DbAlbum, DbAlbumArtist, DbArtist, DbAudioFormat, DbFile, DbImport, DbLibraryImage,
    DbRelease, DbStorageProfile, DbTorrent, DbTrack, DbTrackArtist, ImportOperationStatus,
    ImportStatus, LibraryImageType, LibrarySearchResults, StorageLocation,
};
use crate::encryption::EncryptionService;
use crate::library::export::ExportService;
use crate::storage::cleanup::{append_pending_deletions, PendingDeletion};
use std::path::Path;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::warn;
#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Import error: {0}")]
    Import(String),
    #[error("Track mapping error: {0}")]
    TrackMapping(String),
    #[error("Cloud storage error: {0}")]
    CloudStorage(#[from] CloudStorageError),
    #[error("Encryption error: {0}")]
    Encryption(#[from] crate::encryption::EncryptionError),
}

/// Events emitted by LibraryManager when data changes
#[derive(Clone, Debug)]
pub enum LibraryEvent {
    /// Albums have changed (added, deleted, or modified)
    AlbumsChanged,
}
/// The main library manager for database operations and entity persistence
///
/// Handles:
/// - Album/track/file persistence
/// - State transitions (importing -> complete/failed)
/// - Query methods for library browsing
/// - Deletion with cloud storage cleanup
pub struct LibraryManager {
    database: Database,
    encryption_service: Option<EncryptionService>,
    event_tx: broadcast::Sender<LibraryEvent>,
}

impl std::fmt::Debug for LibraryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryManager")
            .field("database", &self.database)
            .field("encryption_service", &self.encryption_service)
            .finish_non_exhaustive()
    }
}

impl Clone for LibraryManager {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            encryption_service: self.encryption_service.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}
impl LibraryManager {
    /// Create a new library manager
    pub fn new(database: Database, encryption_service: Option<EncryptionService>) -> Self {
        let (event_tx, _) = broadcast::channel(16);
        LibraryManager {
            database,
            encryption_service,
            event_tx,
        }
    }

    /// Subscribe to library events (albums changed, etc.)
    pub fn subscribe_events(&self) -> broadcast::Receiver<LibraryEvent> {
        self.event_tx.subscribe()
    }

    /// Notify subscribers that albums have changed
    pub fn notify_albums_changed(&self) {
        let _ = self.event_tx.send(LibraryEvent::AlbumsChanged);
    }

    /// Get a reference to the encryption service (if configured)
    pub fn encryption_service(&self) -> Option<&EncryptionService> {
        self.encryption_service.as_ref()
    }

    /// Get a reference to the database
    pub fn database(&self) -> &Database {
        &self.database
    }
    /// Insert album, release, and tracks into database in a transaction
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_album_with_release_and_tracks(album, release, tracks)
            .await?;
        Ok(())
    }
    /// Mark release as importing when pipeline starts processing
    pub async fn mark_release_importing(&self, release_id: &str) -> Result<(), LibraryError> {
        self.database
            .update_release_status(release_id, ImportStatus::Importing)
            .await?;
        Ok(())
    }
    /// Mark track as complete after successful import
    pub async fn mark_track_complete(&self, track_id: &str) -> Result<(), LibraryError> {
        self.database
            .update_track_status(track_id, ImportStatus::Complete)
            .await?;
        Ok(())
    }
    /// Mark track as failed if import errors
    pub async fn mark_track_failed(&self, track_id: &str) -> Result<(), LibraryError> {
        self.database
            .update_track_status(track_id, ImportStatus::Failed)
            .await?;
        Ok(())
    }
    /// Update track duration
    pub async fn update_track_duration(
        &self,
        track_id: &str,
        duration_ms: Option<i64>,
    ) -> Result<(), LibraryError> {
        self.database
            .update_track_duration(track_id, duration_ms)
            .await?;
        Ok(())
    }
    /// Mark release as complete after successful import
    pub async fn mark_release_complete(&self, release_id: &str) -> Result<(), LibraryError> {
        self.database
            .update_release_status(release_id, ImportStatus::Complete)
            .await?;
        Ok(())
    }
    /// Mark release as failed if import errors
    pub async fn mark_release_failed(&self, release_id: &str) -> Result<(), LibraryError> {
        self.database
            .update_release_status(release_id, ImportStatus::Failed)
            .await?;
        Ok(())
    }
    /// Add a file to the library
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

    /// Add audio format for a track
    pub async fn add_audio_format(&self, audio_format: &DbAudioFormat) -> Result<(), LibraryError> {
        self.database.insert_audio_format(audio_format).await?;
        Ok(())
    }
    /// Insert torrent metadata
    pub async fn insert_torrent(&self, torrent: &DbTorrent) -> Result<(), LibraryError> {
        self.database.insert_torrent(torrent).await?;
        Ok(())
    }
    /// Get torrent by release ID
    pub async fn get_torrent_by_release(
        &self,
        release_id: &str,
    ) -> Result<Option<DbTorrent>, LibraryError> {
        Ok(self.database.get_torrent_by_release(release_id).await?)
    }
    /// Insert torrent piece mapping
    pub async fn insert_torrent_piece_mapping(
        &self,
        mapping: &crate::db::DbTorrentPieceMapping,
    ) -> Result<(), LibraryError> {
        self.database.insert_torrent_piece_mapping(mapping).await?;
        Ok(())
    }
    /// Get all torrents that are marked as seeding
    pub async fn get_seeding_torrents(&self) -> Result<Vec<DbTorrent>, LibraryError> {
        Ok(self.database.get_seeding_torrents().await?)
    }
    /// Mark a torrent as seeding
    pub async fn set_torrent_seeding(
        &self,
        torrent_id: &str,
        is_seeding: bool,
    ) -> Result<(), LibraryError> {
        self.database
            .update_torrent_seeding(torrent_id, is_seeding)
            .await?;
        Ok(())
    }
    /// Get all albums in the library
    pub async fn get_albums(&self) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums().await?)
    }
    /// Get album by ID
    pub async fn get_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self.database.get_album_by_id(album_id).await?)
    }
    /// Get all releases for a specific album
    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.database.get_releases_for_album(album_id).await?)
    }
    /// Get tracks for a specific release
    pub async fn get_tracks(&self, release_id: &str) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }
    /// Get a single track by ID
    pub async fn get_track(&self, track_id: &str) -> Result<Option<DbTrack>, LibraryError> {
        Ok(self.database.get_track_by_id(track_id).await?)
    }
    /// Get all files for a specific release
    ///
    /// Files belong to releases (not albums or tracks). This includes both:
    /// - Audio files (linked to tracks via db_track_position)
    /// - Metadata files (cover art, CUE sheets, etc.)
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.database.get_files_for_release(release_id).await?)
    }
    /// Get a specific file by ID
    ///
    /// Used during streaming to retrieve the file record after looking up
    /// the track→file relationship via db_track_position.
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, LibraryError> {
        Ok(self.database.get_file_by_id(file_id).await?)
    }
    /// Get audio format for a track
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, LibraryError> {
        Ok(self.database.get_audio_format_by_track_id(track_id).await?)
    }
    /// Get release ID for a track
    pub async fn get_release_id_for_track(&self, track_id: &str) -> Result<String, LibraryError> {
        let track = self
            .database
            .get_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Track not found".to_string()))?;
        Ok(track.release_id)
    }
    /// Get album ID for a track
    pub async fn get_album_id_for_track(&self, track_id: &str) -> Result<String, LibraryError> {
        let track = self
            .database
            .get_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Track not found".to_string()))?;
        let album_id = self
            .database
            .get_album_id_for_release(&track.release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }
    /// Get album ID for a release
    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id = self
            .database
            .get_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }
    /// Insert an artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }
    /// Get artist by Discogs ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self
            .database
            .get_artist_by_discogs_id(discogs_artist_id)
            .await?)
    }

    /// Get artist by MusicBrainz ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_mb_id(mb_id).await?)
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_name(name).await?)
    }

    /// Fill in NULL external IDs on an existing artist (never overwrites)
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .update_artist_external_ids(id, discogs_id, mb_id, sort_name)
            .await?)
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(
        &self,
        album_artist: &DbAlbumArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_album_artist(album_artist).await?;
        Ok(())
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(
        &self,
        track_artist: &DbTrackArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_track_artist(track_artist).await?;
        Ok(())
    }
    /// Get artists for an album
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }
    /// Get artists for a track
    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }
    /// Get artist by ID
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_id(artist_id).await?)
    }
    /// Search across artists, albums, and tracks
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<LibrarySearchResults, LibraryError> {
        Ok(self.database.search_library(query, limit).await?)
    }

    /// Get albums for an artist
    pub async fn get_albums_for_artist(
        &self,
        artist_id: &str,
    ) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums_for_artist(artist_id).await?)
    }
    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), LibraryError> {
        self.database.upsert_library_image(image).await?;
        Ok(())
    }

    /// Get a library image by ID and type
    pub async fn get_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.get_library_image(id, image_type).await?)
    }

    /// Get a library image by ID (regardless of type)
    pub async fn get_library_image_by_id(
        &self,
        id: &str,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.get_library_image_by_id(id).await?)
    }

    /// Delete a library image by ID and type
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), LibraryError> {
        self.database.delete_library_image(id, image_type).await?;
        Ok(())
    }

    /// Set an album's cover release (which release provides the cover art)
    pub async fn set_album_cover_release(
        &self,
        album_id: &str,
        cover_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .set_album_cover_release(album_id, cover_release_id)
            .await?;
        Ok(())
    }

    /// Queue all files for a release into the pending deletions manifest.
    ///
    /// Only queues files that have a storage profile (bae-managed storage).
    /// Self-managed files (no profile) are left untouched.
    async fn queue_release_files_for_deletion(&self, release_id: &str, library_path: &Path) {
        let profile = match self
            .database
            .get_storage_profile_for_release(release_id)
            .await
        {
            Ok(Some(profile)) => profile,
            _ => return,
        };

        let files = match self.get_files_for_release(release_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!("Failed to get files for release {}: {}", release_id, e);
                return;
            }
        };

        let pending: Vec<PendingDeletion> = files
            .iter()
            .filter_map(|f| {
                let source_path = f.source_path.as_ref()?;
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
            })
            .collect();

        if !pending.is_empty() {
            if let Err(e) = append_pending_deletions(library_path, &pending).await {
                warn!("Failed to queue deferred deletions: {}", e);
            }
        }
    }

    /// Delete a release and its associated data
    ///
    /// This will:
    /// 1. Queue files for deferred deletion via the pending deletions manifest
    /// 2. Delete the release from database (cascades to tracks, files, etc.)
    /// 3. If this was the last release for the album, also delete the album
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
    pub async fn delete_release(
        &self,
        release_id: &str,
        library_path: &Path,
    ) -> Result<(), LibraryError> {
        let album_id = self.get_album_id_for_release(release_id).await?;

        // Queue files for deferred deletion before removing DB records
        self.queue_release_files_for_deletion(release_id, library_path)
            .await;

        self.database.delete_release(release_id).await?;
        let remaining_releases = self.get_releases_for_album(&album_id).await?;
        if remaining_releases.is_empty() {
            self.database.delete_album(&album_id).await?;
        }

        // Notify UI that library has changed
        self.notify_albums_changed();

        Ok(())
    }

    /// Delete an album and all its associated data
    ///
    /// This will:
    /// 1. Get all releases for the album
    /// 2. Queue files for deferred deletion via the pending deletions manifest
    /// 3. Delete the album from database (cascades to releases and all related data)
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
    pub async fn delete_album(
        &self,
        album_id: &str,
        library_path: &Path,
    ) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;
        for release in &releases {
            self.queue_release_files_for_deletion(&release.id, library_path)
                .await;
        }

        self.database.delete_album(album_id).await?;

        // Notify UI that library has changed
        self.notify_albums_changed();

        Ok(())
    }
    /// Export all files for a release to a directory
    ///
    /// Copies files from storage to the target directory.
    /// Files are written with their original filenames.
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
        cache: &CacheManager,
        key_service: &crate::keys::KeyService,
    ) -> Result<(), LibraryError> {
        ExportService::export_release(
            release_id,
            target_dir,
            self,
            cache,
            self.encryption_service.as_ref(),
            key_service,
        )
        .await
        .map_err(LibraryError::Import)
    }
    /// Export a single track as a FLAC file
    ///
    /// For one-file-per-track: extracts the original file.
    /// For CUE/FLAC: extracts and re-encodes as a standalone FLAC.
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        cache: &CacheManager,
        key_service: &crate::keys::KeyService,
    ) -> Result<(), LibraryError> {
        // Get storage profile for track's release
        let track = self
            .get_track(track_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Track not found: {}", track_id)))?;
        let storage_profile = self
            .database
            .get_storage_profile_for_release(&track.release_id)
            .await?
            .ok_or_else(|| LibraryError::Import("No storage profile for release".to_string()))?;
        let storage = crate::storage::create_storage_reader(&storage_profile, key_service)
            .await
            .map_err(LibraryError::CloudStorage)?;

        ExportService::export_track(
            track_id,
            output_path,
            self,
            storage,
            cache,
            self.encryption_service.as_ref(),
        )
        .await
        .map_err(LibraryError::Import)
    }
    /// Check if an album already exists by Discogs IDs
    ///
    /// Used for duplicate detection before import.
    /// Returns the existing album if found, None otherwise.
    pub async fn find_duplicate_by_discogs(
        &self,
        master_id: Option<&str>,
        release_id: Option<&str>,
    ) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self
            .database
            .find_album_by_discogs_ids(master_id, release_id)
            .await?)
    }
    /// Check if an album already exists by MusicBrainz IDs
    ///
    /// Used for duplicate detection before import.
    /// Returns the existing album if found, None otherwise.
    pub async fn find_duplicate_by_musicbrainz(
        &self,
        release_id: Option<&str>,
        release_group_id: Option<&str>,
    ) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self
            .database
            .find_album_by_mb_ids(release_id, release_group_id)
            .await?)
    }
    /// Get all storage profiles
    pub async fn get_all_storage_profiles(&self) -> Result<Vec<DbStorageProfile>, LibraryError> {
        Ok(self.database.get_all_storage_profiles().await?)
    }
    /// Get the default storage profile
    pub async fn get_default_storage_profile(
        &self,
    ) -> Result<Option<DbStorageProfile>, LibraryError> {
        Ok(self.database.get_default_storage_profile().await?)
    }
    /// Insert a new storage profile
    pub async fn insert_storage_profile(
        &self,
        profile: &DbStorageProfile,
    ) -> Result<(), LibraryError> {
        Ok(self.database.insert_storage_profile(profile).await?)
    }
    /// Set a profile as the default
    pub async fn set_default_storage_profile(&self, profile_id: &str) -> Result<(), LibraryError> {
        Ok(self
            .database
            .set_default_storage_profile(profile_id)
            .await?)
    }
    /// Update a storage profile
    pub async fn update_storage_profile(
        &self,
        profile: &DbStorageProfile,
    ) -> Result<(), LibraryError> {
        Ok(self.database.update_storage_profile(profile).await?)
    }
    /// Delete a storage profile
    pub async fn delete_storage_profile(&self, profile_id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_storage_profile(profile_id).await?)
    }
    /// Get the storage profile for a release
    pub async fn get_storage_profile_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<DbStorageProfile>, LibraryError> {
        Ok(self
            .database
            .get_storage_profile_for_release(release_id)
            .await?)
    }
    /// Delete release storage link
    pub async fn delete_release_storage(&self, release_id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_release_storage(release_id).await?)
    }

    /// Delete all file records for a release
    pub async fn delete_files_for_release(&self, release_id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_files_for_release(release_id).await?)
    }

    /// Insert release storage link
    pub async fn insert_release_storage(
        &self,
        release_storage: &crate::db::DbReleaseStorage,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .insert_release_storage(release_storage)
            .await?)
    }

    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), LibraryError> {
        Ok(self.database.insert_import(import).await?)
    }
    /// Update the status of an import operation
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), LibraryError> {
        Ok(self.database.update_import_status(id, status).await?)
    }
    /// Link an import operation to a release (after release is created)
    pub async fn link_import_to_release(
        &self,
        import_id: &str,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .link_import_to_release(import_id, release_id)
            .await?)
    }
    /// Record an error for an import operation
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), LibraryError> {
        Ok(self.database.update_import_error(id, error).await?)
    }
    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, LibraryError> {
        Ok(self.database.get_active_imports().await?)
    }

    /// Delete an import record (used by UI to dismiss stuck imports)
    pub async fn delete_import(&self, id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_import(id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DbAlbum, DbRelease, ImportStatus};
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn setup_test_manager() -> (LibraryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new(db_path.to_str().unwrap()).await.unwrap();
        let encryption_service = EncryptionService::new_with_key(&[0u8; 32]);
        let manager = LibraryManager::new(database, Some(encryption_service));
        (manager, temp_dir)
    }

    fn create_test_album() -> DbAlbum {
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: "Test Album".to_string(),
            year: Some(2024),
            discogs_release: None,
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_release_id: None,
            cover_art_url: None,
            is_compilation: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_release(album_id: &str) -> DbRelease {
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year: Some(2024),
            discogs_release_id: None,
            bandcamp_release_id: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            import_status: ImportStatus::Complete,
            private: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_delete_release_with_single_release_deletes_album() {
        let (manager, temp_dir) = setup_test_manager().await;
        let album = create_test_album();
        let release = create_test_release(&album.id);

        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();

        manager
            .delete_release(&release.id, temp_dir.path())
            .await
            .unwrap();

        let album_result = manager.database.get_album_by_id(&album.id).await.unwrap();
        assert!(album_result.is_none());
        let releases = manager
            .database
            .get_releases_for_album(&album.id)
            .await
            .unwrap();
        assert!(releases.is_empty());
    }

    #[tokio::test]
    async fn test_delete_release_with_multiple_releases_preserves_album() {
        let (manager, temp_dir) = setup_test_manager().await;
        let album = create_test_album();
        let release1 = create_test_release(&album.id);
        let release2 = create_test_release(&album.id);

        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release1).await.unwrap();
        manager.database.insert_release(&release2).await.unwrap();

        manager
            .delete_release(&release1.id, temp_dir.path())
            .await
            .unwrap();

        let album_result = manager.database.get_album_by_id(&album.id).await.unwrap();
        assert!(album_result.is_some());
        let releases = manager
            .database
            .get_releases_for_album(&album.id)
            .await
            .unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].id, release2.id);
    }

    #[tokio::test]
    async fn test_delete_album_deletes_all_releases() {
        let (manager, temp_dir) = setup_test_manager().await;
        let album = create_test_album();
        let release1 = create_test_release(&album.id);
        let release2 = create_test_release(&album.id);

        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release1).await.unwrap();
        manager.database.insert_release(&release2).await.unwrap();

        manager
            .delete_album(&album.id, temp_dir.path())
            .await
            .unwrap();

        let album_result = manager.database.get_album_by_id(&album.id).await.unwrap();
        assert!(album_result.is_none());
        let releases = manager
            .database
            .get_releases_for_album(&album.id)
            .await
            .unwrap();
        assert!(releases.is_empty());
    }
}
