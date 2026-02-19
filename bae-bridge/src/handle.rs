use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bae_core::cloud_home::CloudHome;
use bae_core::config::Config;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::image_server::{self, ImageServerHandle};
use bae_core::import::{ImportProgress, ImportService, ImportServiceHandle, ScanEvent};
use bae_core::keys::KeyService;
use bae_core::library::SharedLibraryManager;
use bae_core::playback::{PlaybackHandle, PlaybackProgress, PlaybackService, PlaybackState};
use tracing::{info, warn};

use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeArtist, BridgeConfig, BridgeCoverSelection, BridgeError,
    BridgeFile, BridgeImportCandidate, BridgeImportStatus, BridgeLibraryInfo, BridgeMetadataResult,
    BridgePlaybackState, BridgeRelease, BridgeRemoteCover, BridgeRepeatMode, BridgeTrack,
};

/// Discover all libraries in ~/.bae/libraries/.
#[uniffi::export]
pub fn discover_libraries() -> Result<Vec<BridgeLibraryInfo>, BridgeError> {
    let libraries = Config::discover_libraries();
    Ok(libraries
        .into_iter()
        .map(|lib| BridgeLibraryInfo {
            id: lib.id,
            name: lib.name,
            path: lib.path.to_string_lossy().to_string(),
        })
        .collect())
}

/// Create a new library with an optional name.
/// Returns the new library's info. The library is set as the active library.
#[uniffi::export]
pub fn create_library(name: Option<String>) -> Result<BridgeLibraryInfo, BridgeError> {
    let dev_mode = Config::is_dev_mode();
    let mut config = Config::create_new_library(dev_mode).map_err(|e| BridgeError::Config {
        msg: format!("{e}"),
    })?;

    if let Some(ref n) = name {
        config.library_name = Some(n.clone());
        config.save().map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })?;
    }

    config
        .save_active_library()
        .map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })?;

    Ok(BridgeLibraryInfo {
        id: config.library_id,
        name: config.library_name,
        path: config.library_dir.to_string_lossy().to_string(),
    })
}

/// Unlock a library by providing the encryption key hex.
/// Validates the key against the stored fingerprint, then saves it to the keyring.
#[uniffi::export]
pub fn unlock_library(library_id: String, key_hex: String) -> Result<(), BridgeError> {
    // Validate hex encoding: must be 64 hex chars = 32 bytes
    if key_hex.len() != 64 {
        return Err(BridgeError::Config {
            msg: "Encryption key must be 64 hex characters (32 bytes)".to_string(),
        });
    }
    if hex::decode(&key_hex).is_err() {
        return Err(BridgeError::Config {
            msg: "Invalid hex encoding".to_string(),
        });
    }

    // Compute fingerprint and compare to stored one
    let fingerprint = bae_core::encryption::compute_key_fingerprint(&key_hex).ok_or_else(|| {
        BridgeError::Config {
            msg: "Failed to compute key fingerprint".to_string(),
        }
    })?;

    // Load config for this library to get the stored fingerprint
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| BridgeError::NotFound {
            msg: format!("Library '{library_id}' not found"),
        })?;

    // Read and parse config.yaml to get the stored fingerprint
    let config_path = lib_info.path.join("config.yaml");
    let config_str = std::fs::read_to_string(&config_path).map_err(|e| BridgeError::Config {
        msg: format!("Failed to read config: {e}"),
    })?;
    let yaml_config: bae_core::config::ConfigYaml =
        serde_yaml::from_str(&config_str).map_err(|e| BridgeError::Config {
            msg: format!("Failed to parse config: {e}"),
        })?;

    if let Some(ref stored_fp) = yaml_config.encryption_key_fingerprint {
        if *stored_fp != fingerprint {
            return Err(BridgeError::Config {
                msg: "Encryption key fingerprint mismatch".to_string(),
            });
        }
    }

    // Save key to keyring
    let dev_mode = Config::is_dev_mode();
    let key_service = KeyService::new(dev_mode, library_id);
    key_service
        .set_encryption_key(&key_hex)
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to save encryption key: {e}"),
        })?;

    Ok(())
}

/// Callback interface for playback and import events. Implemented by Swift.
#[uniffi::export(callback_interface)]
pub trait AppEventHandler: Send + Sync {
    fn on_playback_state_changed(&self, state: BridgePlaybackState);
    fn on_playback_progress(&self, position_ms: u64, duration_ms: u64, track_id: String);
    fn on_queue_updated(&self, track_ids: Vec<String>);
    fn on_scan_result(&self, candidate: BridgeImportCandidate);
    fn on_import_progress(&self, folder_path: String, status: BridgeImportStatus);
    fn on_library_changed(&self);
    fn on_error(&self, message: String);
}

/// The central handle to the bae backend. Owns the tokio runtime and all services.
#[derive(uniffi::Object)]
pub struct AppHandle {
    runtime: tokio::runtime::Runtime,
    config: Config,
    library_manager: SharedLibraryManager,
    key_service: KeyService,
    encryption_service: Option<EncryptionService>,
    cloud_home: Option<Arc<dyn CloudHome>>,
    image_server: ImageServerHandle,
    playback_handle: PlaybackHandle,
    import_handle: ImportServiceHandle,
    event_handler: Mutex<Option<Arc<dyn AppEventHandler>>>,
    subsonic_server_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[uniffi::export]
impl AppHandle {
    /// Get the library ID.
    pub fn library_id(&self) -> String {
        self.config.library_id.clone()
    }

    /// Get the library name.
    pub fn library_name(&self) -> Option<String> {
        self.config.library_name.clone()
    }

    /// Get the library path.
    pub fn library_path(&self) -> String {
        self.config.library_dir.to_string_lossy().to_string()
    }

    /// Whether this library has encryption configured.
    pub fn is_encrypted(&self) -> bool {
        self.config.encryption_key_stored
    }

    /// Get the encryption key fingerprint, if available.
    pub fn get_encryption_fingerprint(&self) -> Option<String> {
        self.config.encryption_key_fingerprint.clone()
    }

    /// Check if the encryption key is available in the keyring.
    /// Returns false if the key is configured but not present (needs unlock).
    pub fn check_encryption_key_available(&self) -> bool {
        self.key_service.get_encryption_key().is_some()
    }

    /// Get all albums with their artist names.
    pub fn get_albums(&self) -> Result<Vec<BridgeAlbum>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let albums = lm.get_albums().await.map_err(|e| BridgeError::Database {
                msg: format!("{e}"),
            })?;

            let mut result = Vec::with_capacity(albums.len());
            for album in &albums {
                let artist_names = match lm.get_artists_for_album(&album.id).await {
                    Ok(artists) => artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(_) => String::new(),
                };
                result.push(BridgeAlbum {
                    id: album.id.clone(),
                    title: album.title.clone(),
                    year: album.year,
                    is_compilation: album.is_compilation,
                    cover_release_id: album.cover_release_id.clone(),
                    artist_names,
                });
            }

            Ok(result)
        })
    }

    /// Get all artists that have at least one album.
    pub fn get_artists(&self) -> Result<Vec<BridgeArtist>, BridgeError> {
        self.runtime.block_on(async {
            let artists = self
                .library_manager
                .get()
                .get_artists_with_albums()
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;

            Ok(artists
                .into_iter()
                .map(|a| BridgeArtist {
                    id: a.id,
                    name: a.name,
                })
                .collect())
        })
    }

    /// Get albums for a specific artist, sorted by year descending.
    pub fn get_artist_albums(&self, artist_id: String) -> Result<Vec<BridgeAlbum>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let albums =
                lm.get_albums_for_artist(&artist_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let mut result = Vec::with_capacity(albums.len());
            for album in &albums {
                let artist_names = match lm.get_artists_for_album(&album.id).await {
                    Ok(artists) => artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(_) => String::new(),
                };
                result.push(BridgeAlbum {
                    id: album.id.clone(),
                    title: album.title.clone(),
                    year: album.year,
                    is_compilation: album.is_compilation,
                    cover_release_id: album.cover_release_id.clone(),
                    artist_names,
                });
            }

            Ok(result)
        })
    }

    /// Get the image URL for an image, if the image file exists on disk.
    /// Returns nil if no image file is present.
    pub fn get_image_url(&self, image_id: String) -> Option<String> {
        self.image_server.image_url_if_exists(&image_id)
    }

    /// Get full album detail including releases, tracks, files, and artists.
    pub fn get_album_detail(&self, album_id: String) -> Result<BridgeAlbumDetail, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();

            let album = lm
                .get_album_by_id(&album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Album '{album_id}' not found"),
                })?;

            let artists =
                lm.get_artists_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let album_artist_names = artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            let db_releases =
                lm.get_releases_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let mut releases = Vec::with_capacity(db_releases.len());
            for rel in &db_releases {
                let db_tracks =
                    lm.get_tracks(&rel.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;

                let mut tracks = Vec::with_capacity(db_tracks.len());
                for t in &db_tracks {
                    let track_artists = lm.get_artists_for_track(&t.id).await.map_err(|e| {
                        BridgeError::Database {
                            msg: format!("{e}"),
                        }
                    })?;

                    let artist_names = if track_artists.is_empty() {
                        album_artist_names.clone()
                    } else {
                        track_artists
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };

                    tracks.push(BridgeTrack {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        disc_number: t.disc_number,
                        track_number: t.track_number,
                        duration_ms: t.duration_ms,
                        artist_names,
                    });
                }

                let db_files =
                    lm.get_files_for_release(&rel.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;

                let files = db_files
                    .into_iter()
                    .map(|f| BridgeFile {
                        id: f.id,
                        original_filename: f.original_filename,
                        file_size: f.file_size,
                        content_type: f.content_type.to_string(),
                    })
                    .collect();

                releases.push(BridgeRelease {
                    id: rel.id.clone(),
                    album_id: rel.album_id.clone(),
                    release_name: rel.release_name.clone(),
                    year: rel.year,
                    format: rel.format.clone(),
                    label: rel.label.clone(),
                    catalog_number: rel.catalog_number.clone(),
                    country: rel.country.clone(),
                    tracks,
                    files,
                });
            }

            let bridge_album = BridgeAlbum {
                id: album.id,
                title: album.title,
                year: album.year,
                is_compilation: album.is_compilation,
                cover_release_id: album.cover_release_id,
                artist_names: album_artist_names,
            };

            let bridge_artists = artists
                .into_iter()
                .map(|a| BridgeArtist {
                    id: a.id,
                    name: a.name,
                })
                .collect();

            Ok(BridgeAlbumDetail {
                album: bridge_album,
                artists: bridge_artists,
                releases,
            })
        })
    }

    /// Play an entire album, optionally starting from a specific track index.
    pub fn play_album(
        &self,
        album_id: String,
        start_track_index: Option<u32>,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let releases =
                lm.get_releases_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            // Collect all tracks across releases, sorted by disc then track number
            let mut all_tracks = Vec::new();
            for rel in &releases {
                let tracks = lm
                    .get_tracks(&rel.id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;
                all_tracks.extend(tracks);
            }

            all_tracks.sort_by(|a, b| {
                let disc_a = a.disc_number.unwrap_or(1);
                let disc_b = b.disc_number.unwrap_or(1);
                disc_a.cmp(&disc_b).then_with(|| {
                    a.track_number
                        .unwrap_or(0)
                        .cmp(&b.track_number.unwrap_or(0))
                })
            });

            let mut track_ids: Vec<String> = all_tracks.into_iter().map(|t| t.id).collect();
            if track_ids.is_empty() {
                return Err(BridgeError::NotFound {
                    msg: format!("No tracks found for album '{album_id}'"),
                });
            }

            // Rotate so the start track is first
            if let Some(idx) = start_track_index {
                let idx = idx as usize;
                if idx < track_ids.len() {
                    track_ids.rotate_left(idx);
                }
            }

            self.playback_handle.play_album(track_ids);
            Ok(())
        })
    }

    /// Play a list of tracks in order.
    pub fn play_tracks(&self, track_ids: Vec<String>) {
        self.playback_handle.play_album(track_ids);
    }

    pub fn pause(&self) {
        self.playback_handle.pause();
    }

    pub fn resume(&self) {
        self.playback_handle.resume();
    }

    pub fn stop(&self) {
        self.playback_handle.stop();
    }

    pub fn next_track(&self) {
        self.playback_handle.next();
    }

    pub fn previous_track(&self) {
        self.playback_handle.previous();
    }

    /// Seek to a position in the current track, in milliseconds.
    pub fn seek(&self, position_ms: u64) {
        self.playback_handle
            .seek(Duration::from_millis(position_ms));
    }

    pub fn set_volume(&self, volume: f32) {
        self.playback_handle.set_volume(volume);
    }

    pub fn set_repeat_mode(&self, mode: BridgeRepeatMode) {
        let core_mode = match mode {
            BridgeRepeatMode::None => bae_core::playback::RepeatMode::None,
            BridgeRepeatMode::Track => bae_core::playback::RepeatMode::Track,
            BridgeRepeatMode::Album => bae_core::playback::RepeatMode::Album,
        };
        self.playback_handle.set_repeat_mode(core_mode);
    }

    // MARK: - Import

    /// Scan a folder for importable music. Results arrive via on_scan_result callbacks.
    pub fn scan_folder(&self, path: String) -> Result<(), BridgeError> {
        self.import_handle
            .enqueue_folder_scan(PathBuf::from(path))
            .map_err(|e| BridgeError::Import { msg: e })
    }

    /// Search MusicBrainz for metadata matching a candidate.
    pub fn search_musicbrainz(
        &self,
        artist: String,
        album: String,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        self.runtime.block_on(async {
            let params = bae_core::musicbrainz::ReleaseSearchParams {
                artist: Some(artist),
                album: Some(album),
                ..Default::default()
            };
            let releases = bae_core::musicbrainz::search_releases_with_params(&params)
                .await
                .map_err(|e| BridgeError::Import {
                    msg: format!("MusicBrainz search failed: {e}"),
                })?;

            Ok(releases
                .into_iter()
                .map(|r| {
                    let year = r
                        .date
                        .as_ref()
                        .and_then(|d| d.split('-').next())
                        .and_then(|y| y.parse::<i32>().ok());
                    BridgeMetadataResult {
                        source: "musicbrainz".to_string(),
                        release_id: r.release_id,
                        title: r.title,
                        artist: r.artist,
                        year,
                        format: r.format,
                        label: r.label,
                        track_count: 0,
                    }
                })
                .collect())
        })
    }

    /// Search Discogs for metadata matching a candidate.
    pub fn search_discogs(
        &self,
        artist: String,
        album: String,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        self.runtime.block_on(async {
            let api_key =
                self.key_service
                    .get_discogs_key()
                    .ok_or_else(|| BridgeError::Import {
                        msg: "Discogs API key not configured".to_string(),
                    })?;
            let client = bae_core::discogs::DiscogsClient::new(api_key);
            let params = bae_core::discogs::client::DiscogsSearchParams {
                artist: Some(artist),
                release_title: Some(album),
                ..Default::default()
            };
            let results =
                client
                    .search_with_params(&params)
                    .await
                    .map_err(|e| BridgeError::Import {
                        msg: format!("Discogs search failed: {e}"),
                    })?;

            Ok(results
                .into_iter()
                .map(|r| {
                    let year = r.year.as_ref().and_then(|y| y.parse::<i32>().ok());
                    let format = r.format.as_ref().map(|f| f.join(", "));
                    let label = r.label.as_ref().and_then(|l| l.first().cloned());
                    BridgeMetadataResult {
                        source: "discogs".to_string(),
                        release_id: r.id.to_string(),
                        title: r.title,
                        artist: String::new(),
                        year,
                        format,
                        label,
                        track_count: 0,
                    }
                })
                .collect())
        })
    }

    /// Start importing a folder with the selected metadata release.
    pub fn commit_import(
        &self,
        folder_path: String,
        release_id: String,
        source: String,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let import_id = uuid::Uuid::new_v4().to_string();
            let folder = PathBuf::from(&folder_path);

            let request = if source == "musicbrainz" {
                bae_core::import::ImportRequest::Folder {
                    import_id,
                    discogs_release: None,
                    mb_release: Some(bae_core::musicbrainz::MbRelease {
                        release_id,
                        release_group_id: String::new(),
                        title: String::new(),
                        artist: String::new(),
                        date: None,
                        first_release_date: None,
                        format: None,
                        country: None,
                        label: None,
                        catalog_number: None,
                        barcode: None,
                        is_compilation: false,
                    }),
                    folder,
                    master_year: 0,
                    managed: true,
                    selected_cover: None,
                }
            } else {
                // Discogs: need to fetch the full release first
                let api_key =
                    self.key_service
                        .get_discogs_key()
                        .ok_or_else(|| BridgeError::Import {
                            msg: "Discogs API key not configured".to_string(),
                        })?;
                let client = bae_core::discogs::DiscogsClient::new(api_key);
                let discogs_release =
                    client
                        .get_release(&release_id)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Failed to fetch Discogs release: {e}"),
                        })?;
                bae_core::import::ImportRequest::Folder {
                    import_id,
                    discogs_release: Some(discogs_release),
                    mb_release: None,
                    folder,
                    master_year: 0,
                    managed: true,
                    selected_cover: None,
                }
            };

            self.import_handle
                .send_request(request)
                .await
                .map_err(|e| BridgeError::Import { msg: e })?;
            Ok(())
        })
    }

    // MARK: - Settings

    /// Get current configuration.
    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig {
            library_id: self.config.library_id.clone(),
            library_name: self.config.library_name.clone(),
            library_path: self.config.library_dir.to_string_lossy().to_string(),
            has_discogs_token: self.key_service.get_discogs_key().is_some(),
            subsonic_port: self.config.server_port,
            subsonic_bind_address: self.config.server_bind_address.clone(),
            subsonic_username: self.config.server_username.clone(),
        }
    }

    /// Rename the library.
    pub fn rename_library(&self, name: String) -> Result<(), BridgeError> {
        Config::rename_library(&self.config.library_dir, &name).map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })
    }

    /// Save Discogs API token to the OS keyring.
    pub fn save_discogs_token(&self, token: String) -> Result<(), BridgeError> {
        self.key_service
            .set_discogs_key(&token)
            .map_err(|e| BridgeError::Config {
                msg: format!("{e}"),
            })
    }

    /// Check if a Discogs API token is configured.
    pub fn has_discogs_token(&self) -> bool {
        self.key_service.get_discogs_key().is_some()
    }

    /// Get the Discogs API token (for display in settings).
    pub fn get_discogs_token(&self) -> Option<String> {
        self.key_service.get_discogs_key()
    }

    /// Remove the Discogs API token from the OS keyring.
    pub fn remove_discogs_token(&self) -> Result<(), BridgeError> {
        self.key_service
            .delete_discogs_key()
            .map_err(|e| BridgeError::Config {
                msg: format!("{e}"),
            })
    }

    // MARK: - Events

    /// Register a callback handler for playback and import events.
    /// Spawns background tasks that forward events to the handler.
    pub fn set_event_handler(&self, handler: Box<dyn AppEventHandler>) {
        let handler: Arc<dyn AppEventHandler> = Arc::from(handler);
        let prev = self.event_handler.lock().unwrap().replace(handler.clone());
        drop(prev);

        // Playback events
        let rx = self.playback_handle.subscribe_progress();
        let lm = self.library_manager.clone();
        self.runtime
            .spawn(dispatch_playback_events(rx, handler.clone(), lm));

        // Scan events
        let scan_rx = self.import_handle.subscribe_folder_scan_events();
        self.runtime
            .spawn(dispatch_scan_events(scan_rx, handler.clone()));

        // Import progress events
        let import_rx = self.import_handle.subscribe_all_imports();
        self.runtime
            .spawn(dispatch_import_events(import_rx, handler));
    }

    /// Fetch available remote cover art options for a release.
    /// Checks MusicBrainz Cover Art Archive and Discogs.
    pub fn fetch_remote_covers(
        &self,
        release_id: String,
    ) -> Result<Vec<BridgeRemoteCover>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();

            // Get the release to find its album
            let release = lm
                .database()
                .get_release_by_id(&release_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Release '{release_id}' not found"),
                })?;

            // Get the album to access MusicBrainz/Discogs IDs
            let album = lm
                .get_album_by_id(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Album '{}' not found", release.album_id),
                })?;

            // Check current cover source to skip duplicates
            let current_source = lm
                .get_library_image(&release_id, &bae_core::db::LibraryImageType::Cover)
                .await
                .ok()
                .flatten()
                .map(|img| img.source);

            let mut covers = Vec::new();

            // Try MusicBrainz Cover Art Archive
            if let Some(ref mb) = album.musicbrainz_release {
                if current_source.as_deref() != Some("musicbrainz") {
                    if let Some(url) =
                        bae_core::import::cover_art::fetch_cover_art_from_archive(&mb.release_id)
                            .await
                    {
                        covers.push(BridgeRemoteCover {
                            url: url.clone(),
                            thumbnail_url: url,
                            label: "MusicBrainz".to_string(),
                            source: "musicbrainz".to_string(),
                        });
                    }
                }
            }

            // Try Discogs
            if let Some(ref discogs_id) = release.discogs_release_id {
                if current_source.as_deref() != Some("discogs") {
                    if let Some(token) = self.key_service.get_discogs_key() {
                        let client = bae_core::discogs::client::DiscogsClient::new(token);
                        if let Ok(discogs_release) = client.get_release(discogs_id).await {
                            if let Some(cover_url) = discogs_release
                                .cover_image
                                .or(discogs_release.thumb.clone())
                            {
                                let thumb =
                                    discogs_release.thumb.unwrap_or_else(|| cover_url.clone());
                                covers.push(BridgeRemoteCover {
                                    url: cover_url,
                                    thumbnail_url: thumb,
                                    label: "Discogs".to_string(),
                                    source: "discogs".to_string(),
                                });
                            }
                        }
                    }
                }
            }

            Ok(covers)
        })
    }

    /// Change the cover art for an album's release.
    pub fn change_cover(
        &self,
        album_id: String,
        release_id: String,
        selection: BridgeCoverSelection,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let library_dir = &self.config.library_dir;

            match selection {
                BridgeCoverSelection::ReleaseImage { file_id } => {
                    let file = lm
                        .get_file_by_id(&file_id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?
                        .ok_or_else(|| BridgeError::NotFound {
                            msg: format!("File '{file_id}' not found"),
                        })?;

                    let release = lm
                        .database()
                        .get_release_by_id(&file.release_id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?
                        .ok_or_else(|| BridgeError::NotFound {
                            msg: format!("Release '{}' not found", file.release_id),
                        })?;

                    let source_path = if release.managed_locally {
                        file.local_storage_path(library_dir)
                    } else if let Some(ref unmanaged) = release.unmanaged_path {
                        std::path::PathBuf::from(unmanaged).join(&file.original_filename)
                    } else {
                        return Err(BridgeError::Internal {
                            msg: "Release has no local file storage".to_string(),
                        });
                    };

                    let bytes = std::fs::read(&source_path).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to read file: {e}"),
                    })?;

                    let content_type = file.content_type.clone();
                    write_cover_and_update_db(
                        lm,
                        library_dir,
                        &album_id,
                        &release_id,
                        &bytes,
                        content_type,
                        "local",
                        Some(format!("release://{}", file.original_filename)),
                    )
                    .await?;
                }
                BridgeCoverSelection::RemoteCover { url, source } => {
                    let (bytes, content_type) =
                        bae_core::import::cover_art::download_cover_art_bytes(&url)
                            .await
                            .map_err(|e| BridgeError::Internal {
                                msg: format!("Failed to download cover: {e}"),
                            })?;

                    write_cover_and_update_db(
                        lm,
                        library_dir,
                        &album_id,
                        &release_id,
                        &bytes,
                        content_type,
                        &source,
                        Some(url),
                    )
                    .await?;
                }
            }

            Ok(())
        })
    }

    /// Get the configured Subsonic server port.
    pub fn server_port(&self) -> u16 {
        self.config.server_port
    }

    /// Get the configured Subsonic server bind address.
    pub fn server_bind_address(&self) -> String {
        self.config.server_bind_address.clone()
    }

    /// Whether the Subsonic server is currently running.
    pub fn is_subsonic_running(&self) -> bool {
        let guard = self.subsonic_server_handle.lock().unwrap();
        match &*guard {
            Some(handle) => !handle.is_finished(),
            None => false,
        }
    }

    /// Start the Subsonic server. No-op if already running.
    pub fn start_subsonic_server(&self) -> Result<(), BridgeError> {
        {
            let guard = self.subsonic_server_handle.lock().unwrap();
            if let Some(handle) = &*guard {
                if !handle.is_finished() {
                    return Ok(());
                }
            }
        }

        let auth = if self.config.server_auth_enabled {
            let password = self.key_service.get_server_password();
            bae_core::subsonic::SubsonicAuth {
                enabled: self.config.server_username.is_some() && password.is_some(),
                username: self.config.server_username.clone(),
                password,
            }
        } else {
            bae_core::subsonic::SubsonicAuth {
                enabled: false,
                username: None,
                password: None,
            }
        };

        let mut app = bae_core::subsonic::create_router(
            self.library_manager.clone(),
            self.encryption_service.clone(),
            self.config.library_dir.clone(),
            self.key_service.clone(),
            auth,
        );

        if let Some(ref ch) = self.cloud_home {
            let cloud_state = Arc::new(bae_core::cloud_routes::CloudRouteState {
                cloud_home: ch.clone(),
            });
            let cloud_router = bae_core::cloud_routes::create_cloud_router(cloud_state);
            app = app.merge(cloud_router);
        }

        let addr = format!(
            "{}:{}",
            self.config.server_bind_address, self.config.server_port
        );
        let handle = self.runtime.spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => {
                    info!("Subsonic server listening on http://{}", addr);
                    l
                }
                Err(e) => {
                    warn!("Failed to bind Subsonic server to {}: {}", addr, e);
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app).await {
                warn!("Subsonic server error: {}", e);
            }
        });

        let mut guard = self.subsonic_server_handle.lock().unwrap();
        *guard = Some(handle);

        Ok(())
    }

    /// Stop the Subsonic server if running.
    pub fn stop_subsonic_server(&self) {
        let mut guard = self.subsonic_server_handle.lock().unwrap();
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    /// Create a share link for a release. Requires cloud home and encryption.
    /// Returns the share URL on success.
    pub fn create_share_link(&self, release_id: String) -> Result<String, BridgeError> {
        self.runtime.block_on(async {
            let cloud_home = self
                .cloud_home
                .as_ref()
                .ok_or_else(|| BridgeError::Config {
                    msg: "Cloud storage not configured".to_string(),
                })?;
            let encryption =
                self.encryption_service
                    .as_ref()
                    .ok_or_else(|| BridgeError::Config {
                        msg: "Encryption not configured".to_string(),
                    })?;
            let base_url =
                self.config
                    .share_base_url
                    .as_deref()
                    .ok_or_else(|| BridgeError::Config {
                        msg: "Share base URL not configured".to_string(),
                    })?;

            let db = self.library_manager.get().database().clone();

            // Verify release exists and is in the cloud
            let release = db
                .get_release_by_id(&release_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: "Release not found".to_string(),
                })?;
            if !release.managed_in_cloud {
                return Err(BridgeError::Config {
                    msg: "Release must be managed in the cloud to share".to_string(),
                });
            }

            // Get album metadata
            let album = db
                .get_album_by_id(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: "Album not found".to_string(),
                })?;
            let artists = db
                .get_artists_for_album(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;
            let artist_name = artists
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string());

            // Get tracks and files
            let tracks = db.get_tracks_for_release(&release_id).await.map_err(|e| {
                BridgeError::Database {
                    msg: format!("{e}"),
                }
            })?;
            let files =
                db.get_files_for_release(&release_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            // Build track list with file keys
            use bae_core::sync::share_format;

            let mut share_tracks = Vec::new();
            let mut manifest_files = Vec::new();

            for track in &tracks {
                let audio_format =
                    db.get_audio_format_by_track_id(&track.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;
                if let Some(af) = audio_format {
                    let file = files
                        .iter()
                        .find(|f| af.file_id.as_deref() == Some(f.id.as_str()));
                    if let Some(file) = file {
                        let file_key = bae_core::storage::storage_path(&file.id);
                        let format = share_format::format_for_content_type(&af.content_type);
                        share_tracks.push(share_format::ShareMetaTrack {
                            number: track.track_number,
                            title: track.title.clone(),
                            duration_secs: track.duration_ms.map(|ms| ms / 1000),
                            file_key: file_key.clone(),
                            format: format.to_string(),
                        });
                        manifest_files.push(file_key);
                    }
                }
            }

            // Cover image key
            let cover_release_id = album.cover_release_id.as_deref().unwrap_or(&release_id);
            let cover_image_key = find_cover_image_key(&db, cover_release_id).await;
            if let Some(ref key) = cover_image_key {
                manifest_files.push(key.clone());
            }

            // Per-release encryption key
            use base64::Engine;

            let release_enc = encryption.derive_release_encryption(&release_id);
            let release_key_b64 =
                base64::engine::general_purpose::STANDARD.encode(release_enc.key_bytes());

            // Build ShareMeta
            let meta = share_format::ShareMeta {
                album_name: album.title,
                artist: artist_name,
                year: album.year,
                cover_image_key,
                tracks: share_tracks,
                release_key_b64,
            };
            let meta_json = serde_json::to_vec(&meta).map_err(|e| BridgeError::Internal {
                msg: format!("Serialize error: {e}"),
            })?;

            // Generate per-share key and encrypt
            let per_share_key = bae_core::encryption::generate_random_key();
            let per_share_enc = EncryptionService::from_key(per_share_key);
            let meta_encrypted = per_share_enc.encrypt_chunked(&meta_json);

            // Build manifest
            let manifest = share_format::ShareManifest {
                files: manifest_files,
            };
            let manifest_json =
                serde_json::to_vec(&manifest).map_err(|e| BridgeError::Internal {
                    msg: format!("Serialize error: {e}"),
                })?;

            // Upload to cloud home
            let share_id = uuid::Uuid::new_v4().to_string();
            cloud_home
                .write(&format!("shares/{share_id}/meta.enc"), meta_encrypted)
                .await
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Upload error: {e}"),
                })?;
            cloud_home
                .write(&format!("shares/{share_id}/manifest.json"), manifest_json)
                .await
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Upload error: {e}"),
                })?;

            // Build URL
            let key_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(per_share_key);
            Ok(format!("{base_url}/share/{share_id}#{key_b64}"))
        })
    }
}

/// Background task that reads PlaybackProgress events and dispatches them
/// to the Swift event handler callback.
async fn dispatch_playback_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    handler: Arc<dyn AppEventHandler>,
    library_manager: SharedLibraryManager,
) {
    while let Some(event) = rx.recv().await {
        match event {
            PlaybackProgress::StateChanged { state } => {
                let bridge_state = convert_playback_state(&state, &library_manager).await;
                handler.on_playback_state_changed(bridge_state);
            }
            PlaybackProgress::PositionUpdate {
                position, track_id, ..
            } => {
                handler.on_playback_progress(
                    position.as_millis() as u64,
                    0, // duration comes from state changes, not position updates
                    track_id,
                );
            }
            PlaybackProgress::QueueUpdated { tracks } => {
                handler.on_queue_updated(tracks);
            }
            PlaybackProgress::PlaybackError { message } => {
                handler.on_error(message);
            }
            // Other events (Seeked, SeekError, TrackCompleted, etc.) don't need
            // separate handling -- state changes cover the UI updates.
            _ => {}
        }
    }
}

/// Background task that reads folder scan events and forwards candidates to Swift.
async fn dispatch_scan_events(
    mut rx: tokio::sync::broadcast::Receiver<ScanEvent>,
    handler: Arc<dyn AppEventHandler>,
) {
    use bae_core::import::folder_scanner::AudioContent;

    while let Ok(event) = rx.recv().await {
        match event {
            ScanEvent::Candidate(candidate) => {
                let (track_count, format) = match &candidate.files.audio {
                    AudioContent::CueFlacPairs(pairs) => {
                        let count: usize = pairs.iter().map(|p| p.track_count).sum();
                        (count as u32, "CUE+FLAC".to_string())
                    }
                    AudioContent::TrackFiles(tracks) => (tracks.len() as u32, "FLAC".to_string()),
                };

                let total_size_bytes = match &candidate.files.audio {
                    AudioContent::CueFlacPairs(pairs) => pairs
                        .iter()
                        .map(|p| p.audio_file.size + p.cue_file.size)
                        .sum(),
                    AudioContent::TrackFiles(tracks) => tracks.iter().map(|t| t.size).sum(),
                };

                handler.on_scan_result(BridgeImportCandidate {
                    folder_path: candidate.path.to_string_lossy().to_string(),
                    artist_name: String::new(),
                    album_title: candidate.name,
                    track_count,
                    format,
                    total_size_bytes,
                });
            }
            ScanEvent::Error(msg) => {
                handler.on_error(msg);
            }
            ScanEvent::Finished => {
                // No specific callback for scan finished; UI can track this
                // via the absence of new candidates.
            }
        }
    }
}

/// Background task that reads import progress events and forwards them to Swift.
async fn dispatch_import_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ImportProgress>,
    handler: Arc<dyn AppEventHandler>,
) {
    while let Some(event) = rx.recv().await {
        match event {
            ImportProgress::Preparing {
                import_id,
                step,
                album_title,
                ..
            } => {
                handler.on_import_progress(
                    import_id,
                    BridgeImportStatus::Importing {
                        progress_percent: 0,
                    },
                );

                info!(
                    "Import preparing: {} - {}",
                    album_title,
                    step.display_text()
                );
            }
            ImportProgress::Started { id, .. } => {
                handler.on_import_progress(
                    id,
                    BridgeImportStatus::Importing {
                        progress_percent: 0,
                    },
                );
            }
            ImportProgress::Progress {
                id,
                percent,
                import_id: Some(iid),
                ..
            } => {
                // Only forward release-level progress, not per-track
                if id != iid {
                    handler.on_import_progress(
                        iid,
                        BridgeImportStatus::Importing {
                            progress_percent: percent as u32,
                        },
                    );
                }
            }
            ImportProgress::Complete {
                release_id: None,
                import_id: Some(iid),
                ..
            } => {
                handler.on_import_progress(iid, BridgeImportStatus::Complete);
                handler.on_library_changed();
            }
            ImportProgress::Failed {
                error,
                import_id: Some(iid),
                ..
            } => {
                handler.on_import_progress(iid, BridgeImportStatus::Error { message: error });
            }
            _ => {}
        }
    }
}

/// Convert a core PlaybackState into a BridgePlaybackState, looking up
/// track metadata (title, artist names, album_id) from the database.
async fn convert_playback_state(
    state: &PlaybackState,
    lm: &SharedLibraryManager,
) -> BridgePlaybackState {
    match state {
        PlaybackState::Stopped => BridgePlaybackState::Stopped,
        PlaybackState::Loading { track_id } => BridgePlaybackState::Loading {
            track_id: track_id.clone(),
        },
        PlaybackState::Playing {
            track,
            position,
            duration,
            decoded_duration,
            ..
        } => {
            let (artist_names, album_id, cover_image_id) = resolve_track_info(lm, &track.id).await;
            let dur = duration.unwrap_or(*decoded_duration).as_millis() as u64;
            BridgePlaybackState::Playing {
                track_id: track.id.clone(),
                track_title: track.title.clone(),
                artist_names,
                album_id,
                cover_image_id,
                position_ms: position.as_millis() as u64,
                duration_ms: dur,
            }
        }
        PlaybackState::Paused {
            track,
            position,
            duration,
            decoded_duration,
            ..
        } => {
            let (artist_names, album_id, cover_image_id) = resolve_track_info(lm, &track.id).await;
            let dur = duration.unwrap_or(*decoded_duration).as_millis() as u64;
            BridgePlaybackState::Paused {
                track_id: track.id.clone(),
                track_title: track.title.clone(),
                artist_names,
                album_id,
                cover_image_id,
                position_ms: position.as_millis() as u64,
                duration_ms: dur,
            }
        }
    }
}

/// Look up artist names, album_id, and cover_image_id for a track.
async fn resolve_track_info(
    lm: &SharedLibraryManager,
    track_id: &str,
) -> (String, String, Option<String>) {
    let mgr = lm.get();

    let artist_names = match mgr.get_artists_for_track(track_id).await {
        Ok(artists) => artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        Err(e) => {
            warn!("Failed to get artists for track {track_id}: {e}");
            String::new()
        }
    };

    let (album_id, cover_image_id) = match mgr.get_album_id_for_track(track_id).await {
        Ok(id) => {
            let cover = match mgr.get_album_by_id(&id).await {
                Ok(Some(album)) => album.cover_release_id,
                _ => None,
            };
            (id, cover)
        }
        Err(e) => {
            warn!("Failed to get album_id for track {track_id}: {e}");
            (String::new(), None)
        }
    };

    (artist_names, album_id, cover_image_id)
}

/// Write cover image bytes to disk and update the database.
#[allow(clippy::too_many_arguments)]
async fn write_cover_and_update_db(
    lm: &bae_core::library::LibraryManager,
    library_dir: &bae_core::library_dir::LibraryDir,
    album_id: &str,
    release_id: &str,
    bytes: &[u8],
    content_type: bae_core::content_type::ContentType,
    source: &str,
    source_url: Option<String>,
) -> Result<(), BridgeError> {
    let cover_path = library_dir.image_path(release_id);
    if let Some(parent) = cover_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BridgeError::Internal {
            msg: format!("Failed to create images dir: {e}"),
        })?;
    }
    std::fs::write(&cover_path, bytes).map_err(|e| BridgeError::Internal {
        msg: format!("Failed to write cover: {e}"),
    })?;

    let library_image = bae_core::db::DbLibraryImage {
        id: release_id.to_string(),
        image_type: bae_core::db::LibraryImageType::Cover,
        content_type,
        file_size: bytes.len() as i64,
        width: None,
        height: None,
        source: source.to_string(),
        source_url,
        updated_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
    };
    lm.upsert_library_image(&library_image)
        .await
        .map_err(|e| BridgeError::Database {
            msg: format!("{e}"),
        })?;

    lm.set_album_cover_release(album_id, release_id)
        .await
        .map_err(|e| BridgeError::Database {
            msg: format!("{e}"),
        })?;

    Ok(())
}

/// Find the S3 key for a release's cover image.
async fn find_cover_image_key(db: &Database, release_id: &str) -> Option<String> {
    let image = db
        .get_library_image(release_id, &bae_core::db::LibraryImageType::Cover)
        .await
        .ok()??;
    let hex = image.id.replace('-', "");
    Some(format!("images/{}/{}/{}", &hex[..2], &hex[2..4], &image.id))
}

/// Initialize the app with a specific library.
/// Call discover_libraries() first to find available libraries.
#[uniffi::export]
pub fn init_app(library_id: String) -> Result<Arc<AppHandle>, BridgeError> {
    // Find the library among discovered libraries
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| BridgeError::NotFound {
            msg: format!("Library '{library_id}' not found"),
        })?;

    // Load the full config for this library.
    // Write the active-library pointer so Config::load() finds it.
    let home_dir = dirs::home_dir().ok_or_else(|| BridgeError::Config {
        msg: "Failed to get home directory".to_string(),
    })?;
    let bae_dir = home_dir.join(".bae");
    std::fs::create_dir_all(&bae_dir).map_err(|e| BridgeError::Config {
        msg: format!("Failed to create .bae directory: {e}"),
    })?;
    std::fs::write(bae_dir.join("active-library"), &lib_info.id).map_err(|e| {
        BridgeError::Config {
            msg: format!("Failed to write active-library pointer: {e}"),
        }
    })?;

    let config = Config::load();

    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| BridgeError::Internal {
        msg: format!("Failed to create runtime: {e}"),
    })?;

    // Initialize FFmpeg
    bae_core::audio_codec::init();

    // Initialize keyring
    bae_core::config::init_keyring();

    // Create database
    let db_path = config.library_dir.db_path();
    let database = runtime
        .block_on(Database::new(db_path.to_str().unwrap()))
        .map_err(|e| BridgeError::Database {
            msg: format!("Failed to open database: {e}"),
        })?;

    // Create key service
    let dev_mode = Config::is_dev_mode();
    let key_service = KeyService::new(dev_mode, config.library_id.clone());

    // Create encryption service if configured
    let encryption_service = if config.encryption_key_stored {
        key_service
            .get_encryption_key()
            .and_then(|key| bae_core::encryption::EncryptionService::new(&key).ok())
    } else {
        None
    };

    // Create library manager
    let library_manager =
        bae_core::library::LibraryManager::new(database.clone(), encryption_service.clone());
    let shared_library = SharedLibraryManager::new(library_manager);

    // Start import service
    let import_handle = ImportService::start(
        runtime.handle().clone(),
        shared_library.clone(),
        encryption_service.clone(),
        Arc::new(database),
        key_service.clone(),
        config.library_dir.clone(),
    );

    // Start playback service (needs an owned LibraryManager clone)
    let playback_handle = PlaybackService::start(
        shared_library.get().clone(),
        encryption_service.clone(),
        config.library_dir.clone(),
        runtime.handle().clone(),
    );

    // Start image server
    let image_server = runtime.block_on(image_server::start_image_server(
        shared_library.clone(),
        config.library_dir.clone(),
        encryption_service.clone(),
        "127.0.0.1",
    ));

    // Try to create cloud home (non-fatal if not configured)
    let cloud_home: Option<Arc<dyn CloudHome>> = match runtime.block_on(
        bae_core::cloud_home::create_cloud_home(&config, &key_service),
    ) {
        Ok(ch) => Some(Arc::from(ch)),
        Err(e) => {
            info!("Cloud home not available: {e}");
            None
        }
    };

    info!("AppHandle initialized for library '{library_id}'");

    Ok(Arc::new(AppHandle {
        runtime,
        config,
        library_manager: shared_library,
        key_service,
        encryption_service,
        cloud_home,
        image_server,
        playback_handle,
        import_handle,
        event_handler: Mutex::new(None),
        subsonic_server_handle: Mutex::new(None),
    }))
}
