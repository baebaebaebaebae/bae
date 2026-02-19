use std::sync::{Arc, Mutex};
use std::time::Duration;

use bae_core::config::Config;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::image_server::{self, ImageServerHandle};
use bae_core::keys::KeyService;
use bae_core::library::SharedLibraryManager;
use bae_core::playback::{PlaybackHandle, PlaybackProgress, PlaybackService, PlaybackState};
use bae_core::sync::bucket::SyncBucketClient;
use tracing::{info, warn};

use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeArtist, BridgeError, BridgeFile, BridgeLibraryInfo,
    BridgeMember, BridgePlaybackState, BridgeRelease, BridgeRepeatMode, BridgeSaveSyncConfig,
    BridgeSyncConfig, BridgeSyncStatus, BridgeTrack,
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

/// Callback interface for playback events. Implemented by Swift.
#[uniffi::export(callback_interface)]
pub trait AppEventHandler: Send + Sync {
    fn on_playback_state_changed(&self, state: BridgePlaybackState);
    fn on_playback_progress(&self, position_ms: u64, duration_ms: u64, track_id: String);
    fn on_queue_updated(&self, track_ids: Vec<String>);
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
    image_server: ImageServerHandle,
    playback_handle: PlaybackHandle,
    event_handler: Mutex<Option<Arc<dyn AppEventHandler>>>,
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

    /// Register a callback handler for playback events.
    /// Spawns a background task that forwards PlaybackProgress events to the handler.
    pub fn set_event_handler(&self, handler: Box<dyn AppEventHandler>) {
        let handler: Arc<dyn AppEventHandler> = Arc::from(handler);
        let prev = self.event_handler.lock().unwrap().replace(handler.clone());
        drop(prev);

        let rx = self.playback_handle.subscribe_progress();
        let lm = self.library_manager.clone();

        self.runtime.spawn(dispatch_events(rx, handler, lm));
    }

    // =========================================================================
    // Sync / membership
    // =========================================================================

    /// Get the current sync configuration status.
    pub fn get_sync_status(&self) -> BridgeSyncStatus {
        let configured = self.config.sync_enabled(&self.key_service);
        BridgeSyncStatus {
            configured,
            syncing: false,
            last_sync_time: None,
            error: None,
            device_count: if configured { 1 } else { 0 },
        }
    }

    /// Get the current sync bucket configuration.
    pub fn get_sync_config(&self) -> BridgeSyncConfig {
        let cloud_provider = self.config.cloud_provider.as_ref().map(|p| match p {
            bae_core::config::CloudProvider::S3 => "s3".to_string(),
            bae_core::config::CloudProvider::ICloud => "icloud".to_string(),
            bae_core::config::CloudProvider::GoogleDrive => "google_drive".to_string(),
            bae_core::config::CloudProvider::Dropbox => "dropbox".to_string(),
            bae_core::config::CloudProvider::OneDrive => "onedrive".to_string(),
            bae_core::config::CloudProvider::BaeCloud => "bae_cloud".to_string(),
        });
        BridgeSyncConfig {
            cloud_provider,
            s3_bucket: self.config.cloud_home_s3_bucket.clone(),
            s3_region: self.config.cloud_home_s3_region.clone(),
            s3_endpoint: self.config.cloud_home_s3_endpoint.clone(),
            s3_key_prefix: self.config.cloud_home_s3_key_prefix.clone(),
            share_base_url: self.config.share_base_url.clone(),
        }
    }

    /// Save S3 sync configuration. Sets cloud_provider to S3 and persists
    /// bucket/region/endpoint/key_prefix to config.yaml and credentials to keyring.
    pub fn save_sync_config(&self, config_data: BridgeSaveSyncConfig) -> Result<(), BridgeError> {
        // Save credentials to keyring
        let creds = bae_core::keys::CloudHomeCredentials::S3 {
            access_key: config_data.access_key,
            secret_key: config_data.secret_key,
        };
        self.key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to save credentials: {e}"),
            })?;

        // Update config and persist
        let mut config = self.config.clone();
        config.cloud_provider = Some(bae_core::config::CloudProvider::S3);
        config.cloud_home_s3_bucket = Some(config_data.bucket);
        config.cloud_home_s3_region = Some(config_data.region);
        config.cloud_home_s3_endpoint = config_data.endpoint.filter(|s| !s.is_empty());
        config.cloud_home_s3_key_prefix = config_data.key_prefix.filter(|s| !s.is_empty());
        config.share_base_url = config_data.share_base_url.filter(|s| !s.is_empty());
        config.save().map_err(|e| BridgeError::Config {
            msg: format!("Failed to save config: {e}"),
        })?;

        info!("Saved S3 sync configuration");
        Ok(())
    }

    /// Get the user's Ed25519 public key (hex-encoded), or nil if no keypair exists.
    pub fn get_user_pubkey(&self) -> Option<String> {
        self.key_service.get_user_public_key().map(hex::encode)
    }

    /// Generate a follow code for this library.
    /// Requires a share_base_url and encryption key to be configured.
    pub fn generate_follow_code(&self) -> Result<String, BridgeError> {
        let proxy_url = self
            .config
            .share_base_url
            .as_ref()
            .ok_or_else(|| BridgeError::Config {
                msg: "No share base URL configured. Set it in sync settings.".to_string(),
            })?;

        let encryption_key_hex =
            self.key_service
                .get_encryption_key()
                .ok_or_else(|| BridgeError::Config {
                    msg: "No encryption key found".to_string(),
                })?;

        let encryption_key =
            hex::decode(&encryption_key_hex).map_err(|e| BridgeError::Internal {
                msg: format!("Invalid encryption key: {e}"),
            })?;

        let library_name = self.config.library_name.as_deref();
        Ok(bae_core::follow_code::encode(
            proxy_url,
            &encryption_key,
            library_name,
        ))
    }

    /// Read the membership chain from the sync bucket and return the current members.
    /// Returns an empty list if sync is not configured or no membership chain exists.
    pub fn get_members(&self) -> Result<Vec<BridgeMember>, BridgeError> {
        // Membership requires a sync bucket. Without one, there are no members to show.
        if !self.config.sync_enabled(&self.key_service) {
            return Ok(Vec::new());
        }

        self.runtime.block_on(async {
            let bucket =
                create_bucket_client(&self.config, &self.key_service, &self.encryption_service)
                    .await
                    .map_err(|e| BridgeError::Config {
                        msg: format!("Failed to create bucket client: {e}"),
                    })?;

            let entry_keys =
                bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to list membership entries: {e}"),
                    })?;

            if entry_keys.is_empty() {
                return Ok(Vec::new());
            }

            let mut raw_entries = Vec::new();
            for (author, seq) in &entry_keys {
                let data = bucket
                    .get_membership_entry(author, *seq)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to get membership entry {author}/{seq}: {e}"),
                    })?;

                let entry: bae_core::sync::membership::MembershipEntry =
                    serde_json::from_slice(&data).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to parse membership entry: {e}"),
                    })?;
                raw_entries.push(entry);
            }

            let chain = bae_core::sync::membership::MembershipChain::from_entries(raw_entries)
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Invalid membership chain: {e}"),
                })?;

            let user_pubkey = self.key_service.get_user_public_key().map(hex::encode);

            let current = chain.current_members();
            let members = current
                .into_iter()
                .map(|(pubkey, role)| {
                    let role_str = match role {
                        bae_core::sync::membership::MemberRole::Owner => "owner".to_string(),
                        bae_core::sync::membership::MemberRole::Member => "member".to_string(),
                    };
                    let name = if user_pubkey.as_deref() == Some(&pubkey) {
                        Some("You".to_string())
                    } else {
                        None
                    };
                    BridgeMember {
                        pubkey,
                        role: role_str,
                        added_by: None,
                        name,
                    }
                })
                .collect();

            Ok(members)
        })
    }
}

/// Background task that reads PlaybackProgress events and dispatches them
/// to the Swift event handler callback.
async fn dispatch_events(
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
        bae_core::library::LibraryManager::new(database, encryption_service.clone());
    let shared_library = SharedLibraryManager::new(library_manager);

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

    info!("AppHandle initialized for library '{library_id}'");

    Ok(Arc::new(AppHandle {
        runtime,
        config,
        library_manager: shared_library,
        key_service,
        encryption_service,
        image_server,
        playback_handle,
        event_handler: Mutex::new(None),
    }))
}

/// Create a temporary bucket client for reading membership data.
///
/// This constructs a CloudHomeSyncBucket from the current config and keyring
/// credentials. Used by get_members() which only needs read access.
async fn create_bucket_client(
    config: &Config,
    key_service: &KeyService,
    encryption_service: &Option<EncryptionService>,
) -> Result<impl SyncBucketClient, String> {
    use bae_core::cloud_home::s3::S3CloudHome;
    use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;

    let bucket = config
        .cloud_home_s3_bucket
        .as_ref()
        .ok_or("No S3 bucket configured")?;
    let region = config
        .cloud_home_s3_region
        .as_ref()
        .ok_or("No S3 region configured")?;
    let endpoint = config.cloud_home_s3_endpoint.clone();

    let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
        Some(bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        }) => (access_key, secret_key),
        _ => return Err("No S3 credentials found in keyring".to_string()),
    };

    let encryption = match encryption_service {
        Some(enc) => enc.clone(),
        None => {
            let key = key_service
                .get_encryption_key()
                .ok_or("No encryption key found")?;
            EncryptionService::new(&key)
                .map_err(|e| format!("Failed to create encryption service: {e}"))?
        }
    };

    let key_prefix = config.cloud_home_s3_key_prefix.clone();
    let cloud_home = S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint,
        access_key,
        secret_key,
        key_prefix,
    )
    .await
    .map_err(|e| format!("Failed to create S3 cloud home: {e}"))?;

    Ok(CloudHomeSyncBucket::new(Box::new(cloud_home), encryption))
}
