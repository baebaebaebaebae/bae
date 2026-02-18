//! AppService - encapsulates reactive state and backend service coordination
//!
//! AppService owns the Store<AppState> and is responsible for:
//! - Subscribing to backend service events
//! - Reducing events and updating the Store
//! - Converting DB types to UI types
//! - Delegating actions to backend services
//!
//! UI components access AppService via `use_app()` and:
//! - Read state reactively from `app.state`
//! - Call action methods like `app.play_album()`

use crate::ui::display_types::{
    album_from_db_ref, artist_from_db_ref, file_from_db_ref, release_from_db_ref, track_from_db_ref,
};
use crate::ui::import_helpers::consume_scan_events;
use bae_core::cache;
use bae_core::config;
use bae_core::db::ImportStatus;
use bae_core::image_server::ImageServerHandle;
use bae_core::import::{self, ImportProgress};
use bae_core::keys::{KeyService, UserKeypair};
use bae_core::library::{LibraryEvent, SharedLibraryManager};
use bae_core::playback::{self, PlaybackProgress};
#[cfg(feature = "torrent")]
use bae_core::torrent;
use bae_ui::display_types::{Album, Artist, File, QueueItem, Release, Track, TrackImportState};
use bae_ui::stores::{
    ActiveImport, ActiveImportsUiStateStoreExt, AlbumDetailStateStoreExt, AppState,
    AppStateStoreExt, ArtistDetailStateStoreExt, ConfigStateStoreExt, DeviceActivityInfo,
    ImportOperationStatus, LibraryStateStoreExt, Member, MemberRole, PlaybackStatus,
    PlaybackUiStateStoreExt, PrepareStep, SyncStateStoreExt,
};
use dioxus::prelude::*;
use std::collections::HashMap;

use super::app_context::{AppServices, SyncHandle};

/// Main application service that encapsulates state and backend coordination.
///
/// Created inside the Dioxus component tree because Store<AppState> is not Send-safe.
/// Access via `use_app()` from any component.
#[derive(Clone)]
pub struct AppService {
    /// Reactive application state (Store for fine-grained reactivity)
    pub state: Store<AppState>,

    /// Library manager for database operations
    pub library_manager: SharedLibraryManager,
    /// Application configuration
    pub config: config::Config,
    /// Import service handle for submitting imports
    pub import_handle: import::ImportServiceHandle,
    /// Playback service handle for audio control
    pub playback_handle: playback::PlaybackHandle,
    /// Cache manager for images/files
    pub cache: cache::CacheManager,
    /// Torrent manager (feature-gated)
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
    /// Key service for secret management
    pub key_service: KeyService,
    /// Image server connection handle
    pub image_server: ImageServerHandle,
    /// User's Ed25519 keypair for signing and key exchange
    pub user_keypair: Option<UserKeypair>,
    /// Sync infrastructure handle (present when sync is configured and encryption is enabled).
    pub sync_handle: Option<SyncHandle>,
}

impl AppService {
    /// Create a new AppService from backend services
    pub fn new(services: &AppServices) -> Self {
        #[cfg(feature = "torrent")]
        {
            Self {
                state: Store::new(AppState::default()),
                library_manager: services.library_manager.clone(),
                config: services.config.clone(),
                import_handle: services.import_handle.clone(),
                playback_handle: services.playback_handle.clone(),
                cache: services.cache.clone(),
                torrent_manager: services.torrent_manager.clone(),
                key_service: services.key_service.clone(),
                image_server: services.image_server.clone(),
                user_keypair: services.user_keypair.clone(),
                sync_handle: services.sync_handle.clone(),
            }
        }
        #[cfg(not(feature = "torrent"))]
        {
            Self {
                state: Store::new(AppState::default()),
                library_manager: services.library_manager.clone(),
                config: services.config.clone(),
                import_handle: services.import_handle.clone(),
                playback_handle: services.playback_handle.clone(),
                cache: services.cache.clone(),
                key_service: services.key_service.clone(),
                image_server: services.image_server.clone(),
                user_keypair: services.user_keypair.clone(),
                sync_handle: services.sync_handle.clone(),
            }
        }
    }

    /// Start all event subscriptions. Call this once after creating AppService.
    pub fn start_subscriptions(&self) {
        self.subscribe_playback_events();

        #[cfg(target_os = "macos")]
        self.subscribe_playback_menu_actions();

        self.subscribe_import_progress();
        self.subscribe_library_events();
        self.subscribe_folder_scan_events();
        self.subscribe_sync_events();
        self.subscribe_url_events();
        self.load_initial_data();
        self.process_pending_deletions();
    }

    // =========================================================================
    // Event Subscriptions
    // =========================================================================

    /// Subscribe to playback state changes and update Store
    fn subscribe_playback_events(&self) {
        let state = self.state;
        let playback_handle = self.playback_handle.clone();
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();

        spawn(async move {
            let mut progress_rx = playback_handle.subscribe_progress();
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    PlaybackProgress::StateChanged { state: new_state } => {
                        let (
                            status,
                            current_track_id,
                            release_id,
                            db_track,
                            position_ms,
                            duration_ms,
                            pregap_ms,
                        ) = match &new_state {
                            bae_core::playback::PlaybackState::Stopped => {
                                (PlaybackStatus::Stopped, None, None, None, 0, 0, None)
                            }
                            bae_core::playback::PlaybackState::Loading { track_id } => (
                                PlaybackStatus::Loading,
                                Some(track_id.clone()),
                                None,
                                None,
                                0,
                                0,
                                None,
                            ),
                            bae_core::playback::PlaybackState::Playing {
                                track,
                                position,
                                duration,
                                pregap_ms,
                                ..
                            } => (
                                PlaybackStatus::Playing,
                                Some(track.id.clone()),
                                Some(track.release_id.clone()),
                                Some(track.clone()),
                                position.as_millis() as u64,
                                duration.map(|d| d.as_millis() as u64).unwrap_or(0),
                                *pregap_ms,
                            ),
                            bae_core::playback::PlaybackState::Paused {
                                track,
                                position,
                                duration,
                                pregap_ms,
                                ..
                            } => (
                                PlaybackStatus::Paused,
                                Some(track.id.clone()),
                                Some(track.release_id.clone()),
                                Some(track.clone()),
                                position.as_millis() as u64,
                                duration.map(|d| d.as_millis() as u64).unwrap_or(0),
                                *pregap_ms,
                            ),
                        };

                        {
                            let mut pb_lens = state.playback();
                            let mut pb = pb_lens.write();
                            pb.status = status;
                            pb.current_track_id = current_track_id.clone();
                            pb.current_release_id = release_id;
                            pb.position_ms = position_ms;
                            pb.duration_ms = duration_ms;
                            pb.pregap_ms = pregap_ms;
                        }

                        // Load album and artist info for current track
                        let (current_track, artist_name, artist_id, cover_url) =
                            if let Some(track) = db_track {
                                let (album_title, cover, artist_name, artist_id) =
                                    if let Some(ref track_id) = current_track_id {
                                        if let Ok(album_id) = library_manager
                                            .get()
                                            .get_album_id_for_track(track_id)
                                            .await
                                        {
                                            let album_info = if let Ok(Some(album)) =
                                                library_manager
                                                    .get()
                                                    .get_album_by_id(&album_id)
                                                    .await
                                            {
                                                let cover = album
                                                    .cover_release_id
                                                    .as_ref()
                                                    .map(|rid| imgs.image_url(rid));
                                                (album.title, cover)
                                            } else {
                                                ("Unknown Album".to_string(), None)
                                            };

                                            // Get artist name and ID
                                            let (artist_name, artist_id) = if let Ok(artists) =
                                                library_manager
                                                    .get()
                                                    .get_artists_for_album(&album_id)
                                                    .await
                                            {
                                                match artists.first() {
                                                    Some(a) => (a.name.clone(), Some(a.id.clone())),
                                                    None => (String::new(), None),
                                                }
                                            } else {
                                                (String::new(), None)
                                            };

                                            (album_info.0, album_info.1, artist_name, artist_id)
                                        } else {
                                            ("Unknown Album".to_string(), None, String::new(), None)
                                        }
                                    } else {
                                        ("Unknown Album".to_string(), None, String::new(), None)
                                    };

                                (
                                    Some(QueueItem {
                                        track: track_from_db_ref(&track),
                                        album_title,
                                        cover_url: cover.clone(),
                                    }),
                                    artist_name,
                                    artist_id,
                                    cover,
                                )
                            } else {
                                (None, String::new(), None, None)
                            };

                        {
                            let mut pb_lens = state.playback();
                            let mut pb = pb_lens.write();
                            pb.current_track = current_track;
                            pb.artist_name = artist_name;
                            pb.artist_id = artist_id;
                            pb.cover_url = cover_url;
                        }
                    }
                    PlaybackProgress::PositionUpdate { position, .. } => {
                        state
                            .playback()
                            .position_ms()
                            .set(position.as_millis() as u64);
                    }
                    PlaybackProgress::Seeked {
                        position,
                        was_paused,
                        ..
                    } => {
                        state
                            .playback()
                            .position_ms()
                            .set(position.as_millis() as u64);
                        // Update status based on was_paused
                        if was_paused {
                            state.playback().status().set(PlaybackStatus::Paused);
                        } else {
                            state.playback().status().set(PlaybackStatus::Playing);
                        }
                    }
                    PlaybackProgress::PlaybackError { message } => {
                        state.playback().playback_error().set(Some(message.clone()));
                        // Clear error after 5 seconds
                        let state = state;
                        spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            state.playback().playback_error().set(None);
                        });
                    }
                    PlaybackProgress::QueueUpdated { tracks } => {
                        // Load track/album details for queue items before writing store
                        let mut queue_items = Vec::new();
                        for track_id in &tracks {
                            if let Ok(Some(track)) = library_manager.get().get_track(track_id).await
                            {
                                let (album_title, cover_url) = if let Ok(album_id) =
                                    library_manager.get().get_album_id_for_track(track_id).await
                                {
                                    if let Ok(Some(album)) =
                                        library_manager.get().get_album_by_id(&album_id).await
                                    {
                                        let cover = album
                                            .cover_release_id
                                            .as_ref()
                                            .map(|rid| imgs.image_url(rid));
                                        (album.title, cover)
                                    } else {
                                        ("Unknown Album".to_string(), None)
                                    }
                                } else {
                                    ("Unknown Album".to_string(), None)
                                };

                                queue_items.push(QueueItem {
                                    track: track_from_db_ref(&track),
                                    album_title,
                                    cover_url,
                                });
                            }
                        }

                        {
                            let mut pb_lens = state.playback();
                            let mut pb = pb_lens.write();
                            pb.queue = tracks;
                            pb.queue_items = queue_items;
                        }
                    }
                    PlaybackProgress::VolumeChanged { volume } => {
                        state.playback().volume().set(volume);
                    }
                    PlaybackProgress::RepeatModeChanged { mode } => {
                        state.playback().repeat_mode().set(mode);

                        #[cfg(target_os = "macos")]
                        crate::ui::window_activation::set_playback_repeat_mode(mode);
                    }
                    _ => {}
                }
            }
        });
    }

    /// Subscribe to playback menu actions (macOS native menu)
    #[cfg(target_os = "macos")]
    fn subscribe_playback_menu_actions(&self) {
        let state = self.state;
        let playback_handle = self.playback_handle.clone();

        spawn(async move {
            let mut rx = crate::ui::shortcuts::subscribe_playback_actions();
            while let Ok(action) = rx.recv().await {
                match action {
                    crate::ui::shortcuts::PlaybackAction::SetRepeatMode(mode) => {
                        playback_handle.set_repeat_mode(mode);
                    }
                    crate::ui::shortcuts::PlaybackAction::TogglePlayPause => {
                        let status = *state.playback().status().read();
                        match status {
                            PlaybackStatus::Playing => playback_handle.pause(),
                            PlaybackStatus::Paused => playback_handle.resume(),
                            PlaybackStatus::Stopped | PlaybackStatus::Loading => {}
                        }
                    }
                    crate::ui::shortcuts::PlaybackAction::Next => playback_handle.next(),
                    crate::ui::shortcuts::PlaybackAction::Previous => playback_handle.previous(),
                }
            }
        });
    }

    /// Subscribe to import progress and update Store
    fn subscribe_import_progress(&self) {
        let state = self.state;
        let import_handle = self.import_handle.clone();

        spawn(async move {
            let mut progress_rx = import_handle.subscribe_all_imports();
            while let Some(event) = progress_rx.recv().await {
                handle_import_progress(&state, event);
            }
        });
    }

    /// Subscribe to library events and reload when albums change
    fn subscribe_library_events(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();

        spawn(async move {
            let mut rx = library_manager.get().subscribe_events();
            while let Ok(event) = rx.recv().await {
                match event {
                    LibraryEvent::AlbumsChanged => {
                        load_library(&state, &library_manager, &imgs).await;
                    }
                }
            }
        });
    }

    /// Subscribe to folder scan events
    fn subscribe_folder_scan_events(&self) {
        let app_service = self.clone();
        let rx = self.import_handle.subscribe_folder_scan_events();

        spawn(async move {
            consume_scan_events(app_service, rx).await;
        });
    }

    /// Start the background sync loop if sync is configured.
    ///
    /// Runs periodic sync cycles: push local changes, pull remote changes,
    /// update cursors, and refresh the UI. Triggered by a 30-second timer,
    /// manual trigger (Phase 5d), or AlbumsChanged events (debounced).
    fn subscribe_sync_events(&self) {
        let Some(sync_handle) = self.sync_handle.clone() else {
            return;
        };

        let Some(user_keypair) = self.user_keypair.clone() else {
            tracing::warn!("Sync is configured but no user keypair — skipping sync loop");
            return;
        };

        let state = self.state;
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();
        let library_dir = self.config.library_dir.clone();

        // Spawn a debounced AlbumsChanged forwarder that sends on the trigger channel.
        let mut library_events_rx = library_manager.get().subscribe_events();
        let trigger_tx = sync_handle.sync_trigger.clone();

        spawn(async move {
            loop {
                match library_events_rx.recv().await {
                    Ok(LibraryEvent::AlbumsChanged) => {
                        // Debounce: wait 2 seconds, then drain any events that
                        // arrived during the sleep so we fire only once per burst.
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        while library_events_rx.try_recv().is_ok() {}
                        let _ = trigger_tx.try_send(());
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        // Spawn the main sync loop
        spawn(async move {
            let Some(mut trigger_rx) = sync_handle.take_trigger_rx().await else {
                tracing::error!("Sync trigger receiver already taken");
                return;
            };

            run_sync_loop(
                &sync_handle,
                &user_keypair,
                &state,
                &library_manager,
                &imgs,
                &library_dir,
                &mut trigger_rx,
            )
            .await;
        });
    }

    /// Subscribe to incoming `bae://` URLs from Apple Events or CLI arguments.
    fn subscribe_url_events(&self) {
        let mut rx = crate::ui::shortcuts::subscribe_url();

        // Drain any URL that arrived before this subscriber existed (cold launch)
        if let Some(url) = crate::ui::shortcuts::take_buffered_url() {
            tracing::info!("URL received in app (buffered): {url}");
            // TODO: Parse bae://share/{token} and trigger import
        }

        spawn(async move {
            while let Ok(url) = rx.recv().await {
                tracing::info!("URL received in app: {url}");
                // TODO: Parse bae://share/{token} and trigger import
            }
        });
    }

    /// Load initial data from database
    fn load_initial_data(&self) {
        self.state.playback().volume().set(1.0);
        self.load_config();
        self.load_active_imports();
        self.load_library();
    }

    /// Process any pending file deletions from previous transfers
    fn process_pending_deletions(&self) {
        let library_dir = self.config.library_dir.clone();

        spawn(async move {
            bae_core::storage::cleanup::process_pending_deletions(&library_dir).await;
        });
    }

    /// Load config into Store
    fn load_config(&self) {
        // Populate user identity in sync store
        let user_pubkey = self
            .user_keypair
            .as_ref()
            .map(|kp| hex::encode(kp.public_key));
        self.state.sync().user_pubkey().set(user_pubkey);

        self.sync_config_to_store(&self.config);
    }

    /// Sync a Config to the Store (config + sync sub-stores).
    ///
    /// Used by both `load_config()` (initial load) and `save_config()` (after
    /// writing to disk) to avoid duplicating the field mapping.
    fn sync_config_to_store(&self, config: &config::Config) {
        {
            let mut config_lens = self.state.config();
            let mut cs = config_lens.write();
            cs.discogs_key_stored = config.discogs_key_stored;
            cs.encryption_key_stored = config.encryption_key_stored;
            cs.encryption_key_fingerprint = config.encryption_key_fingerprint.clone();
            cs.server_enabled = config.server_enabled;
            cs.server_port = config.server_port;
            cs.server_bind_address = config.server_bind_address.clone();
            cs.server_auth_enabled = config.server_auth_enabled;
            cs.server_username = config.server_username.clone();
            cs.torrent_bind_interface = config.torrent_bind_interface.clone();
            cs.torrent_listen_port = config.torrent_listen_port;
            cs.torrent_enable_upnp = config.torrent_enable_upnp;
            cs.torrent_max_connections = config.torrent_max_connections;
            cs.torrent_max_connections_per_torrent = config.torrent_max_connections_per_torrent;
            cs.torrent_max_uploads = config.torrent_max_uploads;
            cs.torrent_max_uploads_per_torrent = config.torrent_max_uploads_per_torrent;
            cs.share_base_url = config.share_base_url.clone();
            cs.cloud_provider = config.cloud_provider.as_ref().map(|p| match p {
                bae_core::config::CloudProvider::S3 => bae_ui::stores::config::CloudProvider::S3,
                bae_core::config::CloudProvider::ICloud => {
                    bae_ui::stores::config::CloudProvider::ICloud
                }
                bae_core::config::CloudProvider::GoogleDrive => {
                    bae_ui::stores::config::CloudProvider::GoogleDrive
                }
                bae_core::config::CloudProvider::Dropbox => {
                    bae_ui::stores::config::CloudProvider::Dropbox
                }
                bae_core::config::CloudProvider::OneDrive => {
                    bae_ui::stores::config::CloudProvider::OneDrive
                }
                bae_core::config::CloudProvider::BaeCloud => {
                    bae_ui::stores::config::CloudProvider::BaeCloud
                }
            });
            cs.cloud_account_display = if matches!(
                config.cloud_provider,
                Some(bae_core::config::CloudProvider::ICloud)
            ) {
                Some("iCloud Drive".to_string())
            } else if matches!(
                config.cloud_provider,
                Some(bae_core::config::CloudProvider::BaeCloud)
            ) {
                config.cloud_home_bae_cloud_username.clone()
            } else {
                cs.cloud_account_display.clone()
            };
            cs.followed_libraries = config
                .followed_libraries
                .iter()
                .map(|fl| bae_ui::stores::config::FollowedLibraryInfo {
                    id: fl.id.clone(),
                    name: fl.name.clone(),
                    server_url: fl.server_url.clone(),
                    username: fl.username.clone(),
                })
                .collect();
        }

        {
            let mut sync_lens = self.state.sync();
            let mut ss = sync_lens.write();
            ss.cloud_home_bucket = config.cloud_home_s3_bucket.clone();
            ss.cloud_home_region = config.cloud_home_s3_region.clone();
            ss.cloud_home_endpoint = config.cloud_home_s3_endpoint.clone();
            ss.cloud_home_configured = config.sync_enabled(&self.key_service);
        }
    }

    /// Load active imports from database
    fn load_active_imports(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            state.active_imports().is_loading().set(true);
            match library_manager.get().get_active_imports().await {
                Ok(db_imports) => {
                    let imports: Vec<ActiveImport> = db_imports
                        .into_iter()
                        .map(|db| ActiveImport {
                            import_id: db.id,
                            album_title: db.album_title,
                            artist_name: db.artist_name,
                            status: convert_import_status(db.status),
                            current_step: None,
                            progress_percent: None,
                            release_id: db.release_id,
                        })
                        .collect();
                    state.active_imports().imports().set(imports);
                }
                Err(e) => {
                    tracing::warn!("Failed to load active imports: {}", e);
                }
            }
            state.active_imports().is_loading().set(false);
        });
    }

    /// Load library albums from database
    pub fn load_library(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();

        spawn(async move {
            load_library(&state, &library_manager, &imgs).await;
        });
    }

    /// Load albums from a followed server into the library state.
    pub fn load_followed_library(&self, followed_id: &str) {
        let state = self.state;
        let key_service = self.key_service.clone();

        // Read connection details from the Store (not the stale boot config)
        let followed = {
            let followed_libs = state.read().config.followed_libraries.clone();
            followed_libs.into_iter().find(|f| f.id == followed_id)
        };

        let Some(followed) = followed else {
            state
                .library()
                .error()
                .set(Some("Followed library not found".to_string()));
            return;
        };

        let password = match key_service.get_followed_password(&followed.id) {
            Some(p) => p,
            None => {
                state
                    .library()
                    .error()
                    .set(Some("No password found for followed library".to_string()));
                return;
            }
        };

        spawn(async move {
            load_followed_library(&state, &followed.server_url, &followed.username, &password)
                .await;
        });
    }

    /// Generate a follow code for an existing followed library.
    pub fn generate_follow_code(&self, followed_id: &str) -> Result<String, String> {
        // Read from the Store (not the stale boot-time config) so newly-added
        // followed libraries are found during the same session.
        let followed_libs = self.state.read().config.followed_libraries.clone();
        let followed = followed_libs
            .iter()
            .find(|f| f.id == followed_id)
            .ok_or_else(|| format!("Followed library '{followed_id}' not found"))?;

        let password = self
            .key_service
            .get_followed_password(followed_id)
            .ok_or_else(|| "No password found for followed library".to_string())?;

        Ok(bae_core::follow_code::encode(
            &followed.server_url,
            &followed.username,
            &password,
            Some(&followed.name),
        ))
    }

    // =========================================================================
    // Album Detail Methods
    // =========================================================================

    /// Load album detail data into Store. Takes pre-read active_source and
    /// followed_libs to avoid calling state.read() (which would subscribe a
    /// use_effect's ReactiveContext to the store and cause infinite re-fires).
    pub fn load_album_detail(
        &self,
        album_id: &str,
        release_id: Option<&str>,
        active_source: &bae_ui::stores::config::LibrarySource,
        followed_libs: &[bae_ui::stores::config::FollowedLibraryInfo],
    ) {
        let state = self.state;

        match active_source {
            bae_ui::stores::config::LibrarySource::Followed(ref followed_id) => {
                let key_service = self.key_service.clone();
                let album_id = album_id.to_string();

                let followed = followed_libs.iter().find(|f| f.id == *followed_id).cloned();

                let Some(followed) = followed else {
                    state
                        .album_detail()
                        .error()
                        .set(Some("Followed library not found".to_string()));
                    return;
                };

                let password = match key_service.get_followed_password(&followed.id) {
                    Some(p) => p,
                    None => {
                        state
                            .album_detail()
                            .error()
                            .set(Some("No password found for followed library".to_string()));
                        return;
                    }
                };

                spawn(async move {
                    load_followed_album_detail(
                        &state,
                        &followed.server_url,
                        &followed.username,
                        &password,
                        &album_id,
                    )
                    .await;
                });
            }
            bae_ui::stores::config::LibrarySource::Local => {
                let library_manager = self.library_manager.clone();
                let album_id = album_id.to_string();
                let release_id = release_id.map(|s| s.to_string());
                let imgs = self.image_server.clone();

                spawn(async move {
                    load_album_detail(
                        &state,
                        &library_manager,
                        &album_id,
                        release_id.as_deref(),
                        &imgs,
                    )
                    .await;
                });
            }
        }
    }

    /// Fetch remote cover options (MusicBrainz + Discogs) for the cover picker
    pub fn fetch_remote_covers(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let key_service = self.key_service.clone();

        // Read IDs from current state
        let releases = state.album_detail().releases().read().clone();
        let selected_release_id = state.album_detail().selected_release_id().read().clone();
        let release =
            selected_release_id.and_then(|rid| releases.iter().find(|r| r.id == rid).cloned());
        let Some(release) = release else { return };
        let release_id = release.id.clone();

        state.album_detail().loading_remote_covers().set(true);
        state.album_detail().remote_covers().set(vec![]);

        spawn(async move {
            let covers = fetch_remote_covers_async(
                &library_manager,
                &key_service,
                &release_id,
                release.musicbrainz_release_id.as_deref(),
                release.discogs_release_id.as_deref(),
            )
            .await;
            state.album_detail().remote_covers().set(covers);
            state.album_detail().loading_remote_covers().set(false);
        });
    }

    /// Change the cover for a release (existing image or remote download)
    pub fn change_cover(
        &self,
        album_id: &str,
        release_id: &str,
        selection: bae_ui::display_types::CoverChange,
    ) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let config = self.config.clone();
        let album_id = album_id.to_string();
        let release_id = release_id.to_string();
        let imgs = self.image_server.clone();

        spawn(async move {
            let result = change_cover_async(
                &library_manager,
                &config.library_dir,
                &album_id,
                &release_id,
                selection,
            )
            .await;

            if let Err(e) = result {
                tracing::error!("Failed to change cover: {}", e);
                return;
            }

            // Reload album detail to reflect updated cover
            load_album_detail(
                &state,
                &library_manager,
                &album_id,
                Some(&release_id),
                &imgs,
            )
            .await;

            // Update album cover_url in library grid with cache-busted URL
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let busted_url = format!("{}&t={}", imgs.image_url(&release_id), timestamp);
            let mut albums_lens = state.library().albums();
            let mut albums = albums_lens.write();
            if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
                album.cover_url = Some(busted_url);
            }
        });
    }

    /// Transfer a release to managed local storage
    pub fn transfer_release_to_managed(&self, release_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let config = self.config.clone();
        let release_id = release_id.to_string();
        let imgs = self.image_server.clone();

        spawn(async move {
            let encryption_service = library_manager.get().encryption_service().cloned();
            let library_dir = config.library_dir.clone();

            let transfer_service = bae_core::storage::transfer::TransferService::new(
                library_manager.clone(),
                encryption_service,
                library_dir.clone(),
            );

            let mut rx = transfer_service.transfer(
                release_id.clone(),
                bae_core::storage::transfer::TransferTarget::ManagedLocal,
            );

            while let Some(progress) = rx.recv().await {
                match progress {
                    bae_core::storage::transfer::TransferProgress::Started { .. } => {
                        state.album_detail().transfer_error().set(None);
                        state.album_detail().transfer_progress().set(Some(
                            bae_ui::stores::album_detail::TransferProgressState {
                                file_index: 0,
                                total_files: 0,
                                filename: String::new(),
                                percent: 0,
                            },
                        ));
                    }
                    bae_core::storage::transfer::TransferProgress::FileProgress {
                        file_index,
                        total_files,
                        filename,
                        percent,
                        ..
                    } => {
                        state.album_detail().transfer_progress().set(Some(
                            bae_ui::stores::album_detail::TransferProgressState {
                                file_index,
                                total_files,
                                filename,
                                percent,
                            },
                        ));
                    }
                    bae_core::storage::transfer::TransferProgress::Complete { .. } => {
                        state.album_detail().transfer_progress().set(None);

                        // Reload album detail to reflect new storage state
                        let album_id = state
                            .album_detail()
                            .album()
                            .read()
                            .as_ref()
                            .map(|a| a.id.clone());
                        if let Some(album_id) = album_id {
                            load_album_detail(
                                &state,
                                &library_manager,
                                &album_id,
                                Some(&release_id),
                                &imgs,
                            )
                            .await;
                        }

                        // Schedule deferred cleanup of old files
                        bae_core::storage::cleanup::schedule_cleanup(&library_dir);
                    }
                    bae_core::storage::transfer::TransferProgress::Failed { error, .. } => {
                        state.album_detail().transfer_progress().set(None);
                        state.album_detail().transfer_error().set(Some(error));
                    }
                }
            }
        });
    }

    /// Eject a release from managed storage to a local folder
    pub fn eject_release_storage(&self, release_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let config = self.config.clone();
        let release_id = release_id.to_string();
        let imgs = self.image_server.clone();

        spawn(async move {
            // Show folder picker
            let folder_handle = match rfd::AsyncFileDialog::new()
                .set_title("Select Eject Directory")
                .pick_folder()
                .await
            {
                Some(handle) => handle,
                None => return, // User cancelled
            };
            let target_dir = folder_handle.path().to_path_buf();

            let encryption_service = library_manager.get().encryption_service().cloned();
            let library_dir = config.library_dir.clone();

            let transfer_service = bae_core::storage::transfer::TransferService::new(
                library_manager.clone(),
                encryption_service,
                library_dir.clone(),
            );

            let mut rx = transfer_service.transfer(
                release_id.clone(),
                bae_core::storage::transfer::TransferTarget::Eject(target_dir),
            );

            while let Some(progress) = rx.recv().await {
                match progress {
                    bae_core::storage::transfer::TransferProgress::Started { .. } => {
                        state.album_detail().transfer_error().set(None);
                        state.album_detail().transfer_progress().set(Some(
                            bae_ui::stores::album_detail::TransferProgressState {
                                file_index: 0,
                                total_files: 0,
                                filename: String::new(),
                                percent: 0,
                            },
                        ));
                    }
                    bae_core::storage::transfer::TransferProgress::FileProgress {
                        file_index,
                        total_files,
                        filename,
                        percent,
                        ..
                    } => {
                        state.album_detail().transfer_progress().set(Some(
                            bae_ui::stores::album_detail::TransferProgressState {
                                file_index,
                                total_files,
                                filename,
                                percent,
                            },
                        ));
                    }
                    bae_core::storage::transfer::TransferProgress::Complete { .. } => {
                        state.album_detail().transfer_progress().set(None);

                        // Reload album detail to reflect new storage state
                        let album_id = state
                            .album_detail()
                            .album()
                            .read()
                            .as_ref()
                            .map(|a| a.id.clone());
                        if let Some(album_id) = album_id {
                            load_album_detail(
                                &state,
                                &library_manager,
                                &album_id,
                                Some(&release_id),
                                &imgs,
                            )
                            .await;
                        }

                        // Schedule deferred cleanup of old files
                        bae_core::storage::cleanup::schedule_cleanup(&library_dir);
                    }
                    bae_core::storage::transfer::TransferProgress::Failed { error, .. } => {
                        state.album_detail().transfer_progress().set(None);
                        state.album_detail().transfer_error().set(Some(error));
                    }
                }
            }
        });
    }

    /// Create a share grant for a release and display the result in the store.
    ///
    /// The grant is a self-contained JSON token the user copies and sends
    /// to the recipient out-of-band.
    pub fn create_share_grant(&self, release_id: &str, recipient_pubkey_hex: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let key_service = self.key_service.clone();
        let config = self.config.clone();
        let user_keypair = self.user_keypair.clone();
        let release_id = release_id.to_string();
        let recipient_pubkey_hex = recipient_pubkey_hex.to_string();

        // Clear previous results
        state.album_detail().share_grant_json().set(None);
        state.album_detail().share_error().set(None);

        spawn(async move {
            let result = create_share_grant_async(
                &library_manager,
                &key_service,
                &config,
                user_keypair.as_ref(),
                &release_id,
                &recipient_pubkey_hex,
            )
            .await;

            match result {
                Ok(json) => {
                    state.album_detail().share_grant_json().set(Some(json));
                }
                Err(e) => {
                    state.album_detail().share_error().set(Some(e));
                }
            }
        });
    }

    /// Create a cloud share link for a release: encrypt metadata, upload to cloud home, copy URL to clipboard.
    pub fn create_share_link(&self, release_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let key_service = self.key_service.clone();
        let config = self.config.clone();
        let release_id = release_id.to_string();

        // Clear previous results
        state.album_detail().share_error().set(None);
        state.album_detail().share_link_copied().set(false);

        spawn(async move {
            match create_share_link_async(&library_manager, &key_service, &config, &release_id)
                .await
            {
                Ok(url) => match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&url)) {
                    Ok(()) => {
                        state.album_detail().share_link_copied().set(true);
                    }
                    Err(e) => {
                        state
                            .album_detail()
                            .share_error()
                            .set(Some(format!("Clipboard: {e}")));
                    }
                },
                Err(e) => {
                    state.album_detail().share_error().set(Some(e));
                }
            }
        });
    }

    // =========================================================================
    // Artist Detail Methods
    // =========================================================================

    /// Load artist detail data into Store (called when navigating to artist page)
    pub fn load_artist_detail(&self, artist_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let artist_id = artist_id.to_string();
        let imgs = self.image_server.clone();

        spawn(async move {
            load_artist_detail(&state, &library_manager, &artist_id, &imgs).await;
        });
    }

    // =========================================================================
    // Config Methods
    // =========================================================================

    /// Update config and save to disk
    pub fn save_config(&self, updater: impl FnOnce(&mut config::Config)) {
        // Clone current config, apply update, save to disk, update Store
        let mut new_config = self.config.clone();
        updater(&mut new_config);

        // Save to disk
        if let Err(e) = new_config.save() {
            tracing::error!("Failed to save config: {}", e);
            return;
        }

        self.sync_config_to_store(&new_config);
    }

    // =========================================================================
    // Sync Config Methods
    // =========================================================================

    pub fn trigger_sync(&self) {
        if let Some(ref sh) = self.sync_handle {
            let _ = sh.sync_trigger.try_send(());
        }
    }

    /// Download membership entries from the sync bucket, build the membership
    /// chain, and update the store with the current members list.
    pub fn load_membership(&self) {
        let Some(sync_handle) = self.sync_handle.clone() else {
            return;
        };

        let user_pubkey = self
            .user_keypair
            .as_ref()
            .map(|kp| hex::encode(kp.public_key));
        let state = self.state;

        spawn(async move {
            let bucket: &dyn bae_core::sync::bucket::SyncBucketClient = &*sync_handle.bucket_client;

            match load_membership_from_bucket(bucket, user_pubkey.as_deref()).await {
                Ok(members) => {
                    state.sync().members().set(members);
                }
                Err(e) => {
                    tracing::warn!("Failed to load membership: {e}");
                }
            }
        });
    }

    /// Invite a new member to the shared library.
    ///
    /// If no membership chain exists yet, bootstraps the founder entry first.
    /// Creates the invitation (membership entry + wrapped key), uploads to the
    /// bucket, and refreshes the member list in the store.
    pub fn invite_member(&self, invitee_pubkey_hex: String, role: MemberRole) {
        let Some(sync_handle) = self.sync_handle.clone() else {
            self.state
                .sync()
                .invite_status()
                .set(Some(bae_ui::stores::InviteStatus::Error(
                    "Sync is not configured".to_string(),
                )));
            return;
        };

        let Some(ref user_keypair) = self.user_keypair else {
            self.state
                .sync()
                .invite_status()
                .set(Some(bae_ui::stores::InviteStatus::Error(
                    "No user keypair available".to_string(),
                )));
            return;
        };

        let encryption_key_hex = match self.key_service.get_encryption_key() {
            Some(k) => k,
            None => {
                self.state
                    .sync()
                    .invite_status()
                    .set(Some(bae_ui::stores::InviteStatus::Error(
                        "Encryption key not configured".to_string(),
                    )));
                return;
            }
        };

        let state = self.state;
        let keypair = user_keypair.clone();
        let user_pubkey_hex = hex::encode(keypair.public_key);

        if invitee_pubkey_hex == user_pubkey_hex {
            state
                .sync()
                .invite_status()
                .set(Some(bae_ui::stores::InviteStatus::Error(
                    "Cannot invite yourself".to_string(),
                )));
            return;
        }

        let hlc = sync_handle.hlc.clone();
        let config = self.config.clone();

        state
            .sync()
            .invite_status()
            .set(Some(bae_ui::stores::InviteStatus::Sending));

        spawn(async move {
            let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;

            let result: Result<bae_core::cloud_home::JoinInfo, String> = async {
                // Parse the encryption key from hex.
                let key_bytes: [u8; 32] = hex::decode(&encryption_key_hex)
                    .map_err(|e| format!("Invalid encryption key hex: {e}"))?
                    .try_into()
                    .map_err(|_| "Encryption key wrong length".to_string())?;

                // Download existing membership entries.
                let entry_keys = bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| format!("Failed to list membership entries: {e}"))?;

                let mut chain = if entry_keys.is_empty() {
                    // No membership chain yet -- bootstrap with a founder entry.
                    let mut founder = MembershipEntry {
                        action: MembershipAction::Add,
                        user_pubkey: user_pubkey_hex.clone(),
                        role: CoreMemberRole::Owner,
                        timestamp: hlc.now().to_string(),
                        author_pubkey: String::new(),
                        signature: String::new(),
                    };

                    sign_membership_entry(&mut founder, &keypair);

                    let mut chain = MembershipChain::new();
                    chain
                        .add_entry(founder.clone())
                        .map_err(|e| format!("Failed to create founder entry: {e}"))?;

                    // Upload the founder entry to the bucket.
                    let founder_bytes = serde_json::to_vec(&founder)
                        .map_err(|e| format!("Failed to serialize founder entry: {e}"))?;
                    bucket
                        .put_membership_entry(&user_pubkey_hex, 1, founder_bytes)
                        .await
                        .map_err(|e| format!("Failed to upload founder entry: {e}"))?;

                    tracing::info!("Bootstrapped membership chain with founder entry");

                    chain
                } else {
                    // Build chain from existing entries.
                    let mut raw_entries = Vec::new();
                    for (author, seq) in &entry_keys {
                        let data =
                            bucket
                                .get_membership_entry(author, *seq)
                                .await
                                .map_err(|e| {
                                    format!("Failed to get membership entry {author}/{seq}: {e}")
                                })?;
                        let entry: MembershipEntry =
                            serde_json::from_slice(&data).map_err(|e| {
                                format!("Failed to parse membership entry {author}/{seq}: {e}")
                            })?;
                        raw_entries.push(entry);
                    }

                    MembershipChain::from_entries(raw_entries)
                        .map_err(|e| format!("Invalid membership chain: {e}"))?
                };

                // Convert UI role to core role.
                let core_role = match role {
                    MemberRole::Owner => CoreMemberRole::Owner,
                    MemberRole::Member => CoreMemberRole::Member,
                };

                // Create the invitation (validates chain, wraps key, uploads).
                let invite_ts = hlc.now().to_string();

                let cloud_home = sync_handle.bucket_client.cloud_home();

                let join_info = bae_core::sync::invite::create_invitation(
                    bucket,
                    cloud_home,
                    &mut chain,
                    &keypair,
                    &invitee_pubkey_hex,
                    core_role,
                    &key_bytes,
                    &invite_ts,
                )
                .await
                .map_err(|e| format!("Failed to create invitation: {e}"))?;

                tracing::info!(
                    "Invited member {}...",
                    &invitee_pubkey_hex[..invitee_pubkey_hex.len().min(16)]
                );

                Ok(join_info)
            }
            .await;

            match result {
                Ok(join_info) => {
                    // Encode invite code for the UI.
                    let invite_code = bae_core::join_code::InviteCode {
                        library_id: config.library_id.clone(),
                        library_name: config.library_name.clone().unwrap_or_default(),
                        join_info,
                        owner_pubkey: user_pubkey_hex.clone(),
                    };
                    let code_string = bae_core::join_code::encode(&invite_code);

                    let invitee_display = if invitee_pubkey_hex.len() > 16 {
                        format!(
                            "{}...{}",
                            &invitee_pubkey_hex[..8],
                            &invitee_pubkey_hex[invitee_pubkey_hex.len() - 8..]
                        )
                    } else {
                        invitee_pubkey_hex.clone()
                    };

                    {
                        let mut sync_lens = state.sync();
                        let mut ss = sync_lens.write();
                        ss.invite_status = Some(bae_ui::stores::InviteStatus::Success);
                        ss.share_info = Some(bae_ui::stores::ShareInfo {
                            invite_code: code_string,
                            invitee_display,
                        });
                    }

                    // Reload the member list (after await, separate write).
                    match load_membership_from_bucket(bucket, Some(&user_pubkey_hex)).await {
                        Ok(members) => state.sync().members().set(members),
                        Err(e) => tracing::warn!("Failed to reload membership after invite: {e}"),
                    }
                }
                Err(e) => {
                    state
                        .sync()
                        .invite_status()
                        .set(Some(bae_ui::stores::InviteStatus::Error(e)));
                }
            }
        });
    }

    /// Remove a member from the shared library.
    ///
    /// Downloads the membership chain, calls `revoke_member()` (which creates a
    /// Remove entry, generates a new encryption key, re-wraps for remaining
    /// members, and deletes the revoked member's wrapped key), persists the new
    /// key to keyring, updates the shared EncryptionService, and reloads the
    /// member list.
    ///
    /// Progress and errors are written to `state.sync().removing_member()` and
    /// `state.sync().remove_member_error()`.
    pub fn remove_member(&self, revokee_pubkey: String) {
        let Some(sync_handle) = self.sync_handle.clone() else {
            self.state
                .sync()
                .remove_member_error()
                .set(Some("Sync is not configured".to_string()));
            return;
        };

        let Some(ref user_keypair) = self.user_keypair else {
            self.state
                .sync()
                .remove_member_error()
                .set(Some("No user keypair available".to_string()));
            return;
        };

        let state = self.state;
        let keypair = user_keypair.clone();
        let user_pubkey_hex = hex::encode(keypair.public_key);
        let hlc = sync_handle.hlc.clone();
        let key_service = self.key_service.clone();
        let config = self.config.clone();

        state.sync().removing_member().set(true);
        state.sync().remove_member_error().set(None);

        spawn(async move {
            let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;

            let result: Result<(), String> = async {
                // Download existing membership entries and build the chain.
                let entry_keys = bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| format!("Failed to list membership entries: {e}"))?;

                if entry_keys.is_empty() {
                    return Err("No membership chain exists".to_string());
                }

                let mut raw_entries = Vec::new();
                for (author, seq) in &entry_keys {
                    let data = bucket
                        .get_membership_entry(author, *seq)
                        .await
                        .map_err(|e| {
                            format!("Failed to get membership entry {author}/{seq}: {e}")
                        })?;
                    let entry: MembershipEntry = serde_json::from_slice(&data).map_err(|e| {
                        format!("Failed to parse membership entry {author}/{seq}: {e}")
                    })?;
                    raw_entries.push(entry);
                }

                let mut chain = MembershipChain::from_entries(raw_entries)
                    .map_err(|e| format!("Invalid membership chain: {e}"))?;

                // Revoke the member (creates Remove entry, rotates key, re-wraps).
                let cloud_home = sync_handle.bucket_client.cloud_home();
                let revoke_ts = hlc.now().to_string();
                let new_key = bae_core::sync::invite::revoke_member(
                    bucket,
                    cloud_home,
                    &mut chain,
                    &keypair,
                    &revokee_pubkey,
                    &revoke_ts,
                )
                .await
                .map_err(|e| format!("Failed to revoke member: {e}"))?;

                // Persist the new encryption key to keyring.
                let new_key_hex = hex::encode(new_key);
                key_service
                    .set_encryption_key(&new_key_hex)
                    .map_err(|e| format!("Failed to persist new encryption key: {e}"))?;

                // Update the shared encryption service (visible to sync loop + bucket client).
                sync_handle.update_encryption_key(new_key);

                // Update config fingerprint and persist so startup won't reject the new key.
                let new_fingerprint = {
                    let enc = sync_handle.encryption.read().unwrap();
                    enc.fingerprint()
                };
                let mut updated_config = config.clone();
                updated_config.encryption_key_fingerprint = Some(new_fingerprint);
                if let Err(e) = updated_config.save_to_config_yaml() {
                    tracing::error!("Failed to save config after key rotation: {e}");
                }

                tracing::info!(
                    "Revoked member {}... and rotated encryption key",
                    &revokee_pubkey[..revokee_pubkey.len().min(16)]
                );

                Ok(())
            }
            .await;

            match result {
                Ok(()) => {
                    // Reload the member list.
                    let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;

                    match load_membership_from_bucket(bucket, Some(&user_pubkey_hex)).await {
                        Ok(members) => state.sync().members().set(members),
                        Err(e) => {
                            tracing::warn!("Failed to reload membership after revocation: {e}")
                        }
                    }
                }
                Err(e) => {
                    state.sync().remove_member_error().set(Some(e));
                }
            }

            state.sync().removing_member().set(false);
        });
    }

    /// Save sync bucket configuration to config.yaml and credentials to keyring.
    /// Sets cloud_provider to S3 and updates the store.
    pub fn save_sync_config(&self, config_data: bae_ui::SyncBucketConfig) -> Result<(), String> {
        let state = self.state;
        let key_service = self.key_service.clone();
        let mut new_config = self.config.clone();

        new_config.cloud_provider = Some(bae_core::config::CloudProvider::S3);
        new_config.cloud_home_s3_bucket = Some(config_data.bucket.clone());
        new_config.cloud_home_s3_region = Some(config_data.region.clone());
        new_config.cloud_home_s3_endpoint = if config_data.endpoint.is_empty() {
            None
        } else {
            Some(config_data.endpoint.clone())
        };

        // Save to disk
        new_config
            .save()
            .map_err(|e| format!("Failed to save config: {}", e))?;

        // Save credentials to keyring
        key_service
            .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::S3 {
                access_key: config_data.access_key.clone(),
                secret_key: config_data.secret_key.clone(),
            })
            .map_err(|e| format!("Failed to save credentials: {}", e))?;

        // Update store
        state
            .config()
            .cloud_provider()
            .set(Some(bae_ui::stores::config::CloudProvider::S3));
        {
            let mut sync_lens = state.sync();
            let mut ss = sync_lens.write();
            ss.cloud_home_bucket = new_config.cloud_home_s3_bucket.clone();
            ss.cloud_home_region = new_config.cloud_home_s3_region.clone();
            ss.cloud_home_endpoint = new_config.cloud_home_s3_endpoint.clone();
            ss.cloud_home_configured = new_config.sync_enabled(&key_service);
        }

        Ok(())
    }

    /// Update the selected cloud provider in the store (does not persist until sign-in/save).
    pub fn select_cloud_provider(&self, provider: bae_ui::stores::config::CloudProvider) {
        self.state.config().cloud_provider().set(Some(provider));
    }

    /// Start OAuth sign-in for a cloud provider. Opens the browser, waits for callback,
    /// stores tokens, and updates the store.
    pub fn sign_in_cloud_provider(&self, provider: bae_ui::stores::config::CloudProvider) {
        let state = self.state;
        let mut config = self.config.clone();
        let key_service = self.key_service.clone();

        state.sync().signing_in().set(true);
        state.sync().sign_in_error().set(None);

        spawn(async move {
            let result: Result<(), String> = async {
                match provider {
                    bae_ui::stores::config::CloudProvider::GoogleDrive => {
                        let oauth_config =
                            bae_core::cloud_home::google_drive::GoogleDriveCloudHome::oauth_config();

                        let tokens = bae_core::oauth::authorize(&oauth_config)
                            .await
                            .map_err(|e| format!("Google Drive authorization failed: {e}"))?;

                        // Create a folder in the user's Drive
                        let lib_name = config
                            .library_name
                            .clone()
                            .unwrap_or_else(|| config.library_id.clone());
                        let folder_name = format!("bae - {}", lib_name);

                        let client = reqwest::Client::new();

                        // Search for an existing folder first to avoid duplicates
                        let search_query = format!(
                            "name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
                            folder_name.replace('\'', "\\'")
                        );
                        let existing_folder_id = async {
                            let resp = client
                                .get("https://www.googleapis.com/drive/v3/files")
                                .bearer_auth(&tokens.access_token)
                                .query(&[("q", &search_query), ("fields", &"files(id)".to_string())])
                                .send()
                                .await
                                .ok()?;
                            let json: serde_json::Value = resp.json().await.ok()?;
                            json["files"][0]["id"].as_str().map(|s| s.to_string())
                        }
                        .await;

                        let folder_id = if let Some(id) = existing_folder_id {
                            id
                        } else {
                            let create_body = serde_json::json!({
                                "name": folder_name,
                                "mimeType": "application/vnd.google-apps.folder",
                            });
                            let resp = client
                                .post("https://www.googleapis.com/drive/v3/files")
                                .bearer_auth(&tokens.access_token)
                                .json(&create_body)
                                .send()
                                .await
                                .map_err(|e| format!("Failed to create Google Drive folder: {e}"))?;

                            if !resp.status().is_success() {
                                let body = resp.text().await.unwrap_or_default();
                                return Err(format!(
                                    "Failed to create Google Drive folder: {body}"
                                ));
                            }

                            let folder_resp: serde_json::Value = resp
                                .json()
                                .await
                                .map_err(|e| format!("Failed to parse folder response: {e}"))?;
                            folder_resp["id"]
                                .as_str()
                                .ok_or("Google Drive folder response missing 'id'")?
                                .to_string()
                        };

                        // Get user email for display
                        let account_resp = client
                            .get("https://www.googleapis.com/drive/v3/about?fields=user")
                            .bearer_auth(&tokens.access_token)
                            .send()
                            .await
                            .ok();
                        let account_display = if let Some(resp) = account_resp {
                            resp.text().await.ok().and_then(|body| {
                                let json: serde_json::Value = serde_json::from_str(&body).ok()?;
                                json["user"]["emailAddress"]
                                    .as_str()
                                    .map(|s| s.to_string())
                            })
                        } else {
                            None
                        };

                        // Save tokens to keyring
                        let token_json = serde_json::to_string(&tokens)
                            .map_err(|e| format!("Failed to serialize tokens: {e}"))?;
                        key_service
                            .set_cloud_home_credentials(
                                &bae_core::keys::CloudHomeCredentials::OAuth { token_json },
                            )
                            .map_err(|e| format!("Failed to save OAuth token: {e}"))?;

                        // Save config
                        config.cloud_provider = Some(config::CloudProvider::GoogleDrive);
                        config.cloud_home_google_drive_folder_id = Some(folder_id);
                        config
                            .save()
                            .map_err(|e| format!("Failed to save config: {e}"))?;

                        // Update store
                        {
                            let mut config_lens = state.config();
                            let mut cs = config_lens.write();
                            cs.cloud_provider =
                                Some(bae_ui::stores::config::CloudProvider::GoogleDrive);
                            cs.cloud_account_display = account_display;
                        }
                        state
                            .sync()
                            .cloud_home_configured()
                            .set(config.sync_enabled(&key_service));

                        Ok(())
                    }
                    bae_ui::stores::config::CloudProvider::Dropbox => {
                        let oauth_config =
                            bae_core::cloud_home::dropbox::DropboxCloudHome::oauth_config();

                        let tokens = bae_core::oauth::authorize(&oauth_config)
                            .await
                            .map_err(|e| format!("Dropbox authorization failed: {e}"))?;

                        // Determine the folder path. Use library_name if set, otherwise library_id.
                        let lib_name = config
                            .library_name
                            .clone()
                            .unwrap_or_else(|| config.library_id.clone());
                        let folder_path = format!("/Apps/bae/{}", lib_name);

                        // Create the folder (ignore error if it already exists)
                        let client = reqwest::Client::new();
                        let create_body = serde_json::json!({
                            "path": folder_path,
                            "autorename": false,
                        });
                        let resp = client
                            .post("https://api.dropboxapi.com/2/files/create_folder_v2")
                            .bearer_auth(&tokens.access_token)
                            .json(&create_body)
                            .send()
                            .await
                            .map_err(|e| format!("Failed to create Dropbox folder: {e}"))?;

                        let status = resp.status();
                        if !status.is_success() {
                            let body = resp.text().await.unwrap_or_default();
                            // 409 with "path/conflict" means the folder already exists -- that's fine
                            if !(status == reqwest::StatusCode::CONFLICT
                                && body.contains("conflict"))
                            {
                                return Err(format!(
                                    "Failed to create Dropbox folder (HTTP {status}): {body}"
                                ));
                            }
                        }

                        // Get account display name
                        let account_resp = client
                            .post("https://api.dropboxapi.com/2/users/get_current_account")
                            .bearer_auth(&tokens.access_token)
                            .header("Content-Type", "application/json")
                            .body("{}")
                            .send()
                            .await
                            .ok();
                        let account_display = if let Some(resp) = account_resp {
                            resp.text().await.ok().and_then(|body| {
                                let json: serde_json::Value = serde_json::from_str(&body).ok()?;
                                json["email"].as_str().map(|s| s.to_string())
                            })
                        } else {
                            None
                        };

                        // Save tokens to keyring
                        let token_json = serde_json::to_string(&tokens)
                            .map_err(|e| format!("Failed to serialize tokens: {e}"))?;
                        key_service
                            .set_cloud_home_credentials(
                                &bae_core::keys::CloudHomeCredentials::OAuth { token_json },
                            )
                            .map_err(|e| format!("Failed to save OAuth token: {e}"))?;

                        // Save config
                        config.cloud_provider = Some(config::CloudProvider::Dropbox);
                        config.cloud_home_dropbox_folder_path = Some(folder_path);
                        config
                            .save()
                            .map_err(|e| format!("Failed to save config: {e}"))?;

                        // Update store
                        {
                            let mut config_lens = state.config();
                            let mut cs = config_lens.write();
                            cs.cloud_provider =
                                Some(bae_ui::stores::config::CloudProvider::Dropbox);
                            cs.cloud_account_display = account_display;
                        }
                        state
                            .sync()
                            .cloud_home_configured()
                            .set(config.sync_enabled(&key_service));

                        Ok(())
                    }
                    bae_ui::stores::config::CloudProvider::OneDrive => {
                        sign_in_onedrive(state, &key_service, &config).await
                    }
                    _ => Err("This provider does not use OAuth sign-in.".to_string()),
                }
            }
            .await;

            if let Err(e) = result {
                state.sync().sign_in_error().set(Some(e));
            }
            state.sync().signing_in().set(false);
        });
    }

    /// Detect and configure iCloud Drive as the cloud home.
    ///
    /// Calls `NSFileManager.URLForUbiquityContainerIdentifier` to find the
    /// ubiquity container, appends a library-specific subdirectory, saves the
    /// path to config, and updates the store.
    #[cfg(target_os = "macos")]
    pub fn use_icloud(&self) {
        let state = self.state;
        let mut config = self.config.clone();
        let key_service = self.key_service.clone();

        match detect_icloud_container() {
            None => {
                state.sync().sign_in_error().set(Some(
                    "iCloud Drive is not available. Sign in to iCloud in System Settings."
                        .to_string(),
                ));
            }
            Some(container) => {
                // Use a library-specific subdirectory inside the container
                let cloud_home_path = container.join(&config.library_id);

                config.cloud_provider = Some(config::CloudProvider::ICloud);
                config.cloud_home_icloud_container_path =
                    Some(cloud_home_path.to_string_lossy().to_string());

                if let Err(e) = config.save() {
                    tracing::error!("Failed to save iCloud config: {e}");
                    state
                        .sync()
                        .sign_in_error()
                        .set(Some(format!("Failed to save config: {e}")));
                    return;
                }

                {
                    let mut config_lens = state.config();
                    let mut cs = config_lens.write();
                    cs.cloud_provider = Some(bae_ui::stores::config::CloudProvider::ICloud);
                    cs.cloud_account_display = Some("iCloud Drive".to_string());
                }
                {
                    let mut sync_lens = state.sync();
                    let mut ss = sync_lens.write();
                    ss.cloud_home_configured = config.sync_enabled(&key_service);
                    ss.sign_in_error = None;
                }
            }
        }
    }

    /// Disconnect the current cloud provider. Clears tokens and config.
    pub fn disconnect_cloud_provider(&self) {
        let state = self.state;
        let key_service = self.key_service.clone();
        let mut new_config = self.config.clone();

        // Best-effort logout for bae cloud (don't block on failure)
        if matches!(
            new_config.cloud_provider,
            Some(config::CloudProvider::BaeCloud)
        ) {
            if let Some(bae_core::keys::CloudHomeCredentials::BaeCloud { session_token }) =
                key_service.get_cloud_home_credentials()
            {
                spawn(async move {
                    if let Err(e) = bae_core::bae_cloud_api::logout(&session_token).await {
                        tracing::warn!("bae cloud logout failed (best-effort): {e}");
                    }
                });
            }
        }

        // Clear all cloud home config fields
        new_config.cloud_provider = None;
        new_config.cloud_home_s3_bucket = None;
        new_config.cloud_home_s3_region = None;
        new_config.cloud_home_s3_endpoint = None;
        new_config.cloud_home_google_drive_folder_id = None;
        new_config.cloud_home_dropbox_folder_path = None;
        new_config.cloud_home_onedrive_drive_id = None;
        new_config.cloud_home_onedrive_folder_id = None;
        new_config.cloud_home_icloud_container_path = None;
        new_config.cloud_home_bae_cloud_url = None;
        new_config.cloud_home_bae_cloud_username = None;

        if let Err(e) = new_config.save() {
            tracing::error!("Failed to save config after disconnect: {e}");
            return;
        }

        // Delete cloud home credentials from keyring
        if let Err(e) = key_service.delete_cloud_home_credentials() {
            tracing::warn!("Failed to delete cloud home credentials: {e}");
        }

        // Update store
        state.config().cloud_provider().set(None);
        state.config().cloud_account_display().set(None);
        {
            let mut sync_lens = state.sync();
            let mut ss = sync_lens.write();
            ss.cloud_home_bucket = None;
            ss.cloud_home_region = None;
            ss.cloud_home_endpoint = None;
            ss.cloud_home_configured = false;
            ss.sign_in_error = None;
        }
    }

    /// Sign up for a new bae cloud account, provision the library, and configure sync.
    pub fn sign_up_bae_cloud(&self, email: String, username: String, password: String) {
        let state = self.state;
        let mut new_config = self.config.clone();
        let key_service = self.key_service.clone();

        state.sync().signing_in().set(true);
        state.sync().sign_in_error().set(None);

        spawn(async move {
            let result: Result<(), String> = async {
                // 1. Call signup API
                let resp = bae_core::bae_cloud_api::signup(&email, &username, &password).await?;

                // 2. Get or create Ed25519 keypair
                let keypair = key_service
                    .get_or_create_user_keypair()
                    .map_err(|e| format!("keypair error: {e}"))?;

                // 3. Sign provision message
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string();
                let message = format!("provision:{}:{timestamp}", resp.library_id);
                let signature = keypair.sign(message.as_bytes());

                let pubkey_hex = hex::encode(keypair.public_key);
                let sig_hex = hex::encode(signature);

                // 4. Call provision API
                bae_core::bae_cloud_api::provision(
                    &resp.session_token,
                    &pubkey_hex,
                    &sig_hex,
                    &timestamp,
                )
                .await?;

                // 5. Save credentials to keyring
                key_service
                    .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::BaeCloud {
                        session_token: resp.session_token,
                    })
                    .map_err(|e| format!("keyring error: {e}"))?;

                // 6. Update config
                new_config.cloud_provider = Some(config::CloudProvider::BaeCloud);
                new_config.cloud_home_bae_cloud_url = Some(resp.library_url);
                new_config.cloud_home_bae_cloud_username = Some(username.clone());
                new_config
                    .save()
                    .map_err(|e| format!("config save error: {e}"))?;

                // 7. Update store
                {
                    let mut config_lens = state.config();
                    let mut cs = config_lens.write();
                    cs.cloud_provider = Some(bae_ui::stores::config::CloudProvider::BaeCloud);
                    cs.cloud_account_display = Some(username);
                }
                state
                    .sync()
                    .cloud_home_configured()
                    .set(new_config.sync_enabled(&key_service));

                Ok(())
            }
            .await;

            if let Err(e) = result {
                state.sync().sign_in_error().set(Some(e));
            }
            state.sync().signing_in().set(false);
        });
    }

    /// Log in to an existing bae cloud account and configure sync.
    pub fn log_in_bae_cloud(&self, email: String, password: String) {
        let state = self.state;
        let mut new_config = self.config.clone();
        let key_service = self.key_service.clone();

        state.sync().signing_in().set(true);
        state.sync().sign_in_error().set(None);

        spawn(async move {
            let result: Result<(), String> = async {
                // 1. Call login API
                let resp = bae_core::bae_cloud_api::login(&email, &password).await?;

                // 2. If not yet provisioned, provision now
                if !resp.provisioned {
                    let keypair = key_service
                        .get_or_create_user_keypair()
                        .map_err(|e| format!("keypair error: {e}"))?;

                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        .to_string();
                    let message = format!("provision:{}:{timestamp}", resp.library_id);
                    let signature = keypair.sign(message.as_bytes());

                    let pubkey_hex = hex::encode(keypair.public_key);
                    let sig_hex = hex::encode(signature);

                    bae_core::bae_cloud_api::provision(
                        &resp.session_token,
                        &pubkey_hex,
                        &sig_hex,
                        &timestamp,
                    )
                    .await?;
                }

                // 3. Save credentials to keyring
                key_service
                    .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::BaeCloud {
                        session_token: resp.session_token,
                    })
                    .map_err(|e| format!("keyring error: {e}"))?;

                // 4. Update config
                new_config.cloud_provider = Some(config::CloudProvider::BaeCloud);
                new_config.cloud_home_bae_cloud_url = Some(resp.library_url);
                // For login we don't have the username from the response, use email prefix
                let display_name = email.split('@').next().unwrap_or(&email).to_string();
                new_config.cloud_home_bae_cloud_username = Some(display_name.clone());
                new_config
                    .save()
                    .map_err(|e| format!("config save error: {e}"))?;

                // 5. Update store
                {
                    let mut config_lens = state.config();
                    let mut cs = config_lens.write();
                    cs.cloud_provider = Some(bae_ui::stores::config::CloudProvider::BaeCloud);
                    cs.cloud_account_display = Some(display_name);
                }
                state
                    .sync()
                    .cloud_home_configured()
                    .set(new_config.sync_enabled(&key_service));

                Ok(())
            }
            .await;

            if let Err(e) = result {
                state.sync().sign_in_error().set(Some(e));
            }
            state.sync().signing_in().set(false);
        });
    }

    // =========================================================================
    // Shared Release Methods
    // =========================================================================

    /// Load accepted share grants from the database and update the store.
    pub fn load_shared_releases(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            match bae_core::sync::shared_release::list_shared_releases(
                library_manager.get().database(),
            )
            .await
            {
                Ok(releases) => {
                    let display: Vec<bae_ui::stores::SharedReleaseDisplay> = releases
                        .into_iter()
                        .map(|r| bae_ui::stores::SharedReleaseDisplay {
                            grant_id: r.grant_id,
                            release_id: r.release_id,
                            from_library_id: r.from_library_id,
                            from_user_pubkey: r.from_user_pubkey,
                            bucket: r.bucket,
                            region: r.region,
                            endpoint: r.endpoint,
                            expires: r.expires,
                        })
                        .collect();
                    state.sync().shared_releases().set(display);
                }
                Err(e) => {
                    tracing::warn!("Failed to load shared releases: {e}");
                }
            }
        });
    }

    /// Remove a shared release grant from the local database.
    pub fn revoke_shared_release(&self, grant_id: String) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            match bae_core::sync::shared_release::revoke_grant(
                library_manager.get().database(),
                &grant_id,
            )
            .await
            {
                Ok(()) => {
                    // Remove from store directly.
                    let mut releases = state.sync().shared_releases().read().clone();
                    releases.retain(|r| r.grant_id != grant_id);
                    state.sync().shared_releases().set(releases);
                }
                Err(e) => {
                    tracing::warn!("Failed to revoke shared release: {e}");
                }
            }
        });
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Load library albums and artists into the Store
async fn load_library(
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    imgs: &ImageServerHandle,
) {
    state.library().loading().set(true);
    state.library().error().set(None);

    match library_manager.get().get_albums().await {
        Ok(album_list) => {
            let mut artists_map = HashMap::new();
            for album in &album_list {
                if let Ok(db_artists) = library_manager.get().get_artists_for_album(&album.id).await
                {
                    let artists = db_artists
                        .iter()
                        .map(|a| artist_from_db_ref(a, imgs))
                        .collect();
                    artists_map.insert(album.id.clone(), artists);
                }
            }
            let display_albums = album_list
                .iter()
                .map(|a| album_from_db_ref(a, imgs))
                .collect();

            let mut lib_lens = state.library();
            let mut lib = lib_lens.write();
            lib.albums = display_albums;
            lib.artists_by_album = artists_map;
            lib.loading = false;
            lib.error = None;
        }
        Err(e) => {
            let mut lib_lens = state.library();
            let mut lib = lib_lens.write();
            lib.error = Some(format!("Failed to load library: {}", e));
            lib.loading = false;
        }
    }
}

/// Load albums from a followed Subsonic server into the Store.
async fn load_followed_library(
    state: &Store<AppState>,
    server_url: &str,
    username: &str,
    password: &str,
) {
    state.library().loading().set(true);
    state.library().error().set(None);

    let client = bae_core::subsonic_client::SubsonicClient::new(
        server_url.to_string(),
        username.to_string(),
        password.to_string(),
    );

    match client.get_album_list("newest", 500, 0).await {
        Ok(albums) => {
            let display_albums: Vec<bae_ui::Album> = albums
                .iter()
                .map(|a| bae_ui::Album {
                    id: a.id.clone(),
                    title: a.name.clone(),
                    year: a.year,
                    cover_url: a
                        .cover_art
                        .as_ref()
                        .map(|id| client.get_cover_art_url(id, Some(300))),
                    is_compilation: false,
                    date_added: chrono::Utc::now(),
                })
                .collect();

            let mut artists_map: HashMap<String, Vec<bae_ui::Artist>> = HashMap::new();
            for a in &albums {
                if let Some(ref artist_name) = a.artist {
                    artists_map.insert(
                        a.id.clone(),
                        vec![bae_ui::Artist {
                            id: a.artist_id.clone().unwrap_or_default(),
                            name: artist_name.clone(),
                            image_url: None,
                        }],
                    );
                }
            }

            let mut lib_lens = state.library();
            let mut lib = lib_lens.write();
            lib.albums = display_albums;
            lib.artists_by_album = artists_map;
            lib.loading = false;
            lib.error = None;
        }
        Err(e) => {
            let mut lib_lens = state.library();
            let mut lib = lib_lens.write();
            lib.error = Some(format!("Failed to load followed library: {}", e));
            lib.loading = false;
        }
    }
}

/// Load album detail from a followed Subsonic server into the Store.
async fn load_followed_album_detail(
    state: &Store<AppState>,
    server_url: &str,
    username: &str,
    password: &str,
    album_id: &str,
) {
    state.album_detail().loading().set(true);
    state.album_detail().error().set(None);

    let client = bae_core::subsonic_client::SubsonicClient::new(
        server_url.to_string(),
        username.to_string(),
        password.to_string(),
    );

    match client.get_album(album_id).await {
        Ok(album) => {
            let cover_url = album
                .cover_art
                .as_ref()
                .map(|id| client.get_cover_art_url(id, Some(600)));

            let display_album = bae_ui::Album {
                id: album.id.clone(),
                title: album.name.clone(),
                year: album.year,
                cover_url,
                is_compilation: false,
                date_added: chrono::Utc::now(),
            };

            let artists = if let Some(ref artist_name) = album.artist {
                vec![bae_ui::Artist {
                    id: album.artist_id.clone().unwrap_or_default(),
                    name: artist_name.clone(),
                    image_url: None,
                }]
            } else {
                vec![]
            };

            let tracks: Vec<bae_ui::Track> = album
                .song
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|s| bae_ui::Track {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    track_number: s.track,
                    disc_number: None,
                    duration_ms: s.duration.map(|d| d as i64 * 1000),
                    is_available: true,
                    import_state: bae_ui::TrackImportState::None,
                })
                .collect();

            let track_count = tracks.len();
            let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
            let track_disc_info: Vec<(Option<i32>, String)> = tracks
                .iter()
                .map(|t| (t.disc_number, t.id.clone()))
                .collect();

            let mut detail_lens = state.album_detail();
            let mut detail = detail_lens.write();
            detail.album = Some(display_album);
            detail.artists = artists;
            detail.tracks = tracks;
            detail.track_count = track_count;
            detail.track_ids = track_ids;
            detail.track_disc_info = track_disc_info;
            detail.releases = vec![];
            detail.files = vec![];
            detail.images = vec![];
            detail.selected_release_id = None;
            detail.managed_locally = false;
            detail.managed_in_cloud = false;
            detail.is_unmanaged = false;
            detail.loading = false;
        }
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load album: {}", e)));
            state.album_detail().loading().set(false);
        }
    }
}

/// All data needed for the album detail view, loaded before touching the store.
struct AlbumDetailData {
    album: Option<Album>,
    artists: Vec<Artist>,
    releases: Vec<Release>,
    selected_release_id: String,
    managed_locally: bool,
    managed_in_cloud: bool,
    is_unmanaged: bool,
    import_progress: Option<u8>,
    tracks: Vec<Track>,
    track_count: usize,
    track_ids: Vec<String>,
    track_disc_info: Vec<(Option<i32>, String)>,
    files: Vec<File>,
    images: Vec<bae_ui::Image>,
}

/// Fetch all album detail data from the database without touching the store.
async fn fetch_album_detail(
    library_manager: &SharedLibraryManager,
    album_id: &str,
    release_id_param: Option<&str>,
    imgs: &ImageServerHandle,
) -> Result<AlbumDetailData, String> {
    let album = library_manager
        .get()
        .get_album_by_id(album_id)
        .await
        .map_err(|e| format!("Failed to load album: {e}"))?
        .map(|ref db_album| album_from_db_ref(db_album, imgs))
        .ok_or_else(|| "Album not found".to_string())?;

    let db_releases = library_manager
        .get()
        .get_releases_for_album(album_id)
        .await
        .map_err(|e| format!("Failed to load releases: {e}"))?;

    if db_releases.is_empty() {
        return Err("Album has no releases".to_string());
    }

    let selected = if let Some(rid) = release_id_param {
        db_releases
            .iter()
            .find(|r| r.id == rid)
            .unwrap_or(&db_releases[0])
    } else {
        &db_releases[0]
    };
    let selected_release_id = selected.id.clone();
    let managed_locally = selected.managed_locally;
    let managed_in_cloud = selected.managed_in_cloud;
    let is_unmanaged = selected.unmanaged_path.is_some();
    let import_progress = if selected.import_status == ImportStatus::Importing
        || selected.import_status == ImportStatus::Queued
    {
        Some(0)
    } else {
        None
    };

    let releases = db_releases.iter().map(release_from_db_ref).collect();

    let artists = library_manager
        .get()
        .get_artists_for_album(album_id)
        .await
        .unwrap_or_default()
        .iter()
        .map(|a| artist_from_db_ref(a, imgs))
        .collect();

    let mut tracks: Vec<Track> = library_manager
        .get()
        .get_tracks(&selected_release_id)
        .await
        .map_err(|e| format!("Failed to load tracks: {e}"))?
        .iter()
        .map(track_from_db_ref)
        .collect();
    tracks.sort_by(|a, b| (a.disc_number, a.track_number).cmp(&(b.disc_number, b.track_number)));

    let track_count = tracks.len();
    let track_ids = tracks.iter().map(|t| t.id.clone()).collect();
    let track_disc_info = tracks
        .iter()
        .map(|t| (t.disc_number, t.id.clone()))
        .collect();

    let db_files = library_manager
        .get()
        .get_files_for_release(&selected_release_id)
        .await
        .unwrap_or_default();

    let files = db_files.iter().map(file_from_db_ref).collect();
    let images = db_files
        .iter()
        .filter(|f| f.content_type.is_image())
        .map(|f| bae_ui::Image {
            id: f.id.clone(),
            filename: f.original_filename.clone(),
            url: imgs.file_url(&f.id),
        })
        .collect();

    Ok(AlbumDetailData {
        album: Some(album),
        artists,
        releases,
        selected_release_id,
        managed_locally,
        managed_in_cloud,
        is_unmanaged,
        import_progress,
        tracks,
        track_count,
        track_ids,
        track_disc_info,
        files,
        images,
    })
}

/// Load album detail data into the Store
async fn load_album_detail(
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    album_id: &str,
    release_id_param: Option<&str>,
    imgs: &ImageServerHandle,
) {
    state.album_detail().loading().set(true);
    state.album_detail().error().set(None);

    match fetch_album_detail(library_manager, album_id, release_id_param, imgs).await {
        Ok(data) => {
            let mut detail_lens = state.album_detail();
            let mut detail = detail_lens.write();
            detail.album = data.album;
            detail.artists = data.artists;
            detail.releases = data.releases;
            detail.selected_release_id = Some(data.selected_release_id);
            detail.managed_locally = data.managed_locally;
            detail.managed_in_cloud = data.managed_in_cloud;
            detail.is_unmanaged = data.is_unmanaged;
            detail.import_progress = data.import_progress;
            detail.tracks = data.tracks;
            detail.track_count = data.track_count;
            detail.track_ids = data.track_ids;
            detail.track_disc_info = data.track_disc_info;
            detail.files = data.files;
            detail.images = data.images;
            detail.transfer_progress = None;
            detail.transfer_error = None;
            detail.remote_covers = vec![];
            detail.loading_remote_covers = false;
            detail.share_grant_json = None;
            detail.share_error = None;
            detail.loading = false;
        }
        Err(msg) => {
            state.album_detail().error().set(Some(msg));
            state.album_detail().loading().set(false);
        }
    }
}

/// Fetch remote cover options from MusicBrainz Cover Art Archive and Discogs
async fn fetch_remote_covers_async(
    library_manager: &SharedLibraryManager,
    key_service: &KeyService,
    release_id: &str,
    mb_release_id: Option<&str>,
    discogs_release_id: Option<&str>,
) -> Vec<bae_ui::display_types::RemoteCoverOption> {
    use crate::ui::import_helpers::get_discogs_client;
    use bae_core::import::cover_art::fetch_cover_art_from_archive;

    let mut covers = Vec::new();

    // Check current cover source to skip duplicates
    let current_source = library_manager
        .get()
        .get_library_image(release_id, &bae_core::db::LibraryImageType::Cover)
        .await
        .ok()
        .flatten()
        .map(|img| img.source);

    // Try MusicBrainz Cover Art Archive
    if let Some(mb_id) = mb_release_id {
        if current_source.as_deref() != Some("musicbrainz") {
            if let Some(url) = fetch_cover_art_from_archive(mb_id).await {
                covers.push(bae_ui::display_types::RemoteCoverOption {
                    url: url.clone(),
                    thumbnail_url: url,
                    label: "MusicBrainz".to_string(),
                    source: "musicbrainz".to_string(),
                });
            }
        }
    }

    // Try Discogs
    if let Some(discogs_id) = discogs_release_id {
        if current_source.as_deref() != Some("discogs") {
            if let Ok(client) = get_discogs_client(key_service) {
                if let Ok(discogs_release) = client.get_release(discogs_id).await {
                    if let Some(cover_url) = discogs_release
                        .cover_image
                        .or(discogs_release.thumb.clone())
                    {
                        let thumb = discogs_release.thumb.unwrap_or_else(|| cover_url.clone());
                        covers.push(bae_ui::display_types::RemoteCoverOption {
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

    covers
}

/// Change the cover for a release
async fn change_cover_async(
    library_manager: &SharedLibraryManager,
    library_dir: &bae_core::library_dir::LibraryDir,
    album_id: &str,
    release_id: &str,
    selection: bae_ui::display_types::CoverChange,
) -> Result<(), String> {
    match selection {
        bae_ui::display_types::CoverChange::ReleaseImage { file_id } => {
            let file = library_manager
                .get()
                .get_file_by_id(&file_id)
                .await
                .map_err(|e| format!("Failed to get file: {}", e))?
                .ok_or_else(|| "File not found".to_string())?;

            // Derive file path from the release's storage flags
            let release = library_manager
                .get()
                .database()
                .get_release_by_id(&file.release_id)
                .await
                .map_err(|e| format!("Failed to get release: {}", e))?
                .ok_or_else(|| "Release not found".to_string())?;

            let source_path = if release.managed_locally {
                file.local_storage_path(library_dir)
            } else if let Some(ref unmanaged) = release.unmanaged_path {
                std::path::PathBuf::from(unmanaged).join(&file.original_filename)
            } else {
                return Err("Release has no local file storage".to_string());
            };
            let bytes =
                std::fs::read(&source_path).map_err(|e| format!("Failed to read file: {}", e))?;

            let content_type = file.content_type.clone();

            // Write to images/{prefix}/{subprefix}/{release_id}
            let cover_path = library_dir.image_path(release_id);
            if let Some(parent) = cover_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create images dir: {}", e))?;
            }
            std::fs::write(&cover_path, &bytes)
                .map_err(|e| format!("Failed to write cover: {}", e))?;

            // Upsert library_images record
            let library_image = bae_core::db::DbLibraryImage {
                id: release_id.to_string(),
                image_type: bae_core::db::LibraryImageType::Cover,
                content_type,
                file_size: bytes.len() as i64,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: Some(format!("release://{}", file.original_filename)),
                updated_at: chrono::Utc::now(),
                created_at: chrono::Utc::now(),
            };
            library_manager
                .get()
                .upsert_library_image(&library_image)
                .await
                .map_err(|e| format!("Failed to upsert library image: {}", e))?;

            // Set album cover release
            library_manager
                .get()
                .set_album_cover_release(album_id, release_id)
                .await
                .map_err(|e| format!("Failed to set album cover release: {}", e))?;
        }
        bae_ui::display_types::CoverChange::RemoteCover { url, source } => {
            use bae_core::import::cover_art::download_cover_art_bytes;

            // Download the image
            let (bytes, content_type) = download_cover_art_bytes(&url)
                .await
                .map_err(|e| format!("Failed to download cover: {}", e))?;

            // Write to images/{prefix}/{subprefix}/{release_id}
            let cover_path = library_dir.image_path(release_id);
            if let Some(parent) = cover_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create images dir: {}", e))?;
            }
            std::fs::write(&cover_path, &bytes)
                .map_err(|e| format!("Failed to write cover: {}", e))?;

            // Upsert library_images record
            let library_image = bae_core::db::DbLibraryImage {
                id: release_id.to_string(),
                image_type: bae_core::db::LibraryImageType::Cover,
                content_type,
                file_size: bytes.len() as i64,
                width: None,
                height: None,
                source,
                source_url: Some(url.clone()),
                updated_at: chrono::Utc::now(),
                created_at: chrono::Utc::now(),
            };
            library_manager
                .get()
                .upsert_library_image(&library_image)
                .await
                .map_err(|e| format!("Failed to upsert library image: {}", e))?;

            // Set album cover release
            library_manager
                .get()
                .set_album_cover_release(album_id, release_id)
                .await
                .map_err(|e| format!("Failed to set album cover release: {}", e))?;
        }
    }

    Ok(())
}

/// All data needed for the artist detail view, loaded before touching the store.
struct ArtistDetailData {
    artist: Artist,
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
}

/// Fetch all artist detail data from the database without touching the store.
async fn fetch_artist_detail(
    library_manager: &SharedLibraryManager,
    artist_id: &str,
    imgs: &ImageServerHandle,
) -> Result<ArtistDetailData, String> {
    let artist = library_manager
        .get()
        .get_artist_by_id(artist_id)
        .await
        .map_err(|e| format!("Failed to load artist: {e}"))?
        .map(|ref db_artist| artist_from_db_ref(db_artist, imgs))
        .ok_or_else(|| "Artist not found".to_string())?;

    let db_albums = library_manager
        .get()
        .get_albums_for_artist(artist_id)
        .await
        .map_err(|e| format!("Failed to load albums: {e}"))?;

    let mut artists_by_album = HashMap::new();
    for album in &db_albums {
        if let Ok(db_artists) = library_manager.get().get_artists_for_album(&album.id).await {
            let artists = db_artists
                .iter()
                .map(|a| artist_from_db_ref(a, imgs))
                .collect();
            artists_by_album.insert(album.id.clone(), artists);
        }
    }

    let albums = db_albums
        .iter()
        .map(|a| album_from_db_ref(a, imgs))
        .collect();

    Ok(ArtistDetailData {
        artist,
        albums,
        artists_by_album,
    })
}

/// Load artist detail data into the Store
async fn load_artist_detail(
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    artist_id: &str,
    imgs: &ImageServerHandle,
) {
    state.artist_detail().loading().set(true);
    state.artist_detail().error().set(None);

    match fetch_artist_detail(library_manager, artist_id, imgs).await {
        Ok(data) => {
            let mut detail_lens = state.artist_detail();
            let mut detail = detail_lens.write();
            detail.artist = Some(data.artist);
            detail.albums = data.albums;
            detail.artists_by_album = data.artists_by_album;
            detail.loading = false;
            detail.error = None;
        }
        Err(msg) => {
            let mut detail_lens = state.artist_detail();
            let mut detail = detail_lens.write();
            detail.error = Some(msg);
            detail.loading = false;
        }
    }
}

/// Convert bae_core ImportOperationStatus to bae_ui ImportOperationStatus
fn convert_import_status(status: bae_core::db::ImportOperationStatus) -> ImportOperationStatus {
    match status {
        bae_core::db::ImportOperationStatus::Preparing => ImportOperationStatus::Preparing,
        bae_core::db::ImportOperationStatus::Importing => ImportOperationStatus::Importing,
        bae_core::db::ImportOperationStatus::Complete => ImportOperationStatus::Complete,
        bae_core::db::ImportOperationStatus::Failed => ImportOperationStatus::Failed,
    }
}

/// Convert bae_core PrepareStep to bae_ui PrepareStep
fn convert_prepare_step(step: bae_core::import::PrepareStep) -> PrepareStep {
    match step {
        bae_core::import::PrepareStep::ParsingMetadata => PrepareStep::ParsingMetadata,
        bae_core::import::PrepareStep::DownloadingCoverArt => PrepareStep::DownloadingCoverArt,
        bae_core::import::PrepareStep::DiscoveringFiles => PrepareStep::DiscoveringFiles,
        bae_core::import::PrepareStep::ValidatingTracks => PrepareStep::ValidatingTracks,
        bae_core::import::PrepareStep::SavingToDatabase => PrepareStep::SavingToDatabase,
        bae_core::import::PrepareStep::ExtractingDurations => PrepareStep::ExtractingDurations,
    }
}

/// Handle import progress events and update Store
fn handle_import_progress(state: &Store<AppState>, event: ImportProgress) {
    match event {
        ImportProgress::Preparing {
            import_id,
            step,
            album_title,
            artist_name,
        } => {
            state.active_imports().imports().with_mut(|list| {
                if let Some(import) = list.iter_mut().find(|i| i.import_id == import_id) {
                    import.current_step = Some(convert_prepare_step(step));
                    import.status = ImportOperationStatus::Preparing;
                } else {
                    list.push(ActiveImport {
                        import_id,
                        album_title,
                        artist_name,
                        status: ImportOperationStatus::Preparing,
                        current_step: Some(convert_prepare_step(step)),
                        progress_percent: None,
                        release_id: None,
                    });
                }
            });
        }
        ImportProgress::Started { id, import_id, .. } => {
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Importing;
                        import.current_step = None;
                        import.progress_percent = Some(0);
                        if import.release_id.is_none() {
                            import.release_id = Some(id.clone());
                        }
                    }
                });
            }
        }
        ImportProgress::Progress {
            id: track_id,
            percent,
            import_id,
            ..
        } => {
            // Update active imports
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.progress_percent = Some(percent);
                    }
                });
            }

            // Update track in album_detail if present
            state.album_detail().tracks().with_mut(|tracks| {
                if let Some(track) = tracks.iter_mut().find(|t| t.id == track_id) {
                    track.import_state = TrackImportState::Importing(percent);
                }
            });

            // Update overall import progress for album detail
            state.album_detail().import_progress().set(Some(percent));
        }
        ImportProgress::Complete {
            id,
            import_id,
            release_id,
            ..
        } => {
            // Update active imports
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Complete;
                        import.progress_percent = Some(100);
                        if release_id.is_some() {
                            import.release_id = release_id.clone();
                        }
                    }
                });
            }

            // Check if this is a track completion (release_id is Some) or release completion
            if release_id.is_some() {
                // Track completed - update track in album_detail
                state.album_detail().tracks().with_mut(|tracks| {
                    if let Some(track) = tracks.iter_mut().find(|t| t.id == id) {
                        track.import_state = TrackImportState::Complete;
                        track.is_available = true;
                    }
                });
            } else {
                // Release completed - clear import progress
                state.album_detail().import_progress().set(None);
                state.album_detail().import_error().set(None);
            }
        }
        ImportProgress::Failed {
            import_id, error, ..
        } => {
            if let Some(ref iid) = import_id {
                state.active_imports().imports().with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Failed;
                    }
                });
            }

            // Update album_detail import error
            state.album_detail().import_progress().set(None);
            state.album_detail().import_error().set(Some(error));
        }
    }
}

/// OneDrive sign-in flow: OAuth authorize, get drive, create app folder, persist config.
async fn sign_in_onedrive(
    state: Store<AppState>,
    key_service: &KeyService,
    config: &config::Config,
) -> Result<(), String> {
    let oauth_config = bae_core::cloud_home::onedrive::OneDriveCloudHome::oauth_config();

    // Step 1: OAuth authorization (opens browser)
    let tokens = bae_core::oauth::authorize(&oauth_config)
        .await
        .map_err(|e| format!("OAuth authorization failed: {e}"))?;

    let client = reqwest::Client::new();

    // Step 2: Get the user's default drive
    let drive_resp = client
        .get("https://graph.microsoft.com/v1.0/me/drive")
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to get drive info: {e}"))?;

    let drive_status = drive_resp.status();
    let drive_body = drive_resp
        .text()
        .await
        .map_err(|e| format!("Failed to read drive response: {e}"))?;

    if !drive_status.is_success() {
        return Err(format!(
            "Failed to get drive info (HTTP {drive_status}): {drive_body}"
        ));
    }

    let drive_json: serde_json::Value = serde_json::from_str(&drive_body)
        .map_err(|e| format!("Failed to parse drive response: {e}"))?;

    let drive_id = drive_json["id"]
        .as_str()
        .ok_or_else(|| "Drive response missing 'id' field".to_string())?
        .to_string();

    let user_display = drive_json["owner"]["user"]["displayName"]
        .as_str()
        .or_else(|| drive_json["owner"]["user"]["email"].as_str())
        .unwrap_or("OneDrive user")
        .to_string();

    // Step 3: Create the app folder in root (or find existing one)
    let folder_name = "bae";
    let create_resp = client
        .post(format!(
            "https://graph.microsoft.com/v1.0/drives/{}/root/children",
            drive_id
        ))
        .bearer_auth(&tokens.access_token)
        .json(&serde_json::json!({
            "name": folder_name,
            "folder": {},
            "@microsoft.graph.conflictBehavior": "useExisting",
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to create app folder: {e}"))?;

    let folder_status = create_resp.status();
    let folder_body = create_resp
        .text()
        .await
        .map_err(|e| format!("Failed to read folder response: {e}"))?;

    if !folder_status.is_success() {
        return Err(format!(
            "Failed to create app folder (HTTP {folder_status}): {folder_body}"
        ));
    }

    let folder_json: serde_json::Value = serde_json::from_str(&folder_body)
        .map_err(|e| format!("Failed to parse folder response: {e}"))?;

    let folder_id = folder_json["id"]
        .as_str()
        .ok_or_else(|| "Folder response missing 'id' field".to_string())?
        .to_string();

    // Step 4: Save tokens to keyring
    let token_json =
        serde_json::to_string(&tokens).map_err(|e| format!("Failed to serialize tokens: {e}"))?;

    key_service
        .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::OAuth { token_json })
        .map_err(|e| format!("Failed to save OAuth token: {e}"))?;

    // Step 5: Save config
    let mut new_config = config.clone();
    new_config.cloud_provider = Some(bae_core::config::CloudProvider::OneDrive);
    new_config.cloud_home_onedrive_drive_id = Some(drive_id);
    new_config.cloud_home_onedrive_folder_id = Some(folder_id);

    new_config
        .save()
        .map_err(|e| format!("Failed to save config: {e}"))?;

    // Step 6: Update store
    state
        .config()
        .cloud_provider()
        .set(Some(bae_ui::stores::config::CloudProvider::OneDrive));
    state
        .config()
        .cloud_account_display()
        .set(Some(user_display));
    state
        .sync()
        .cloud_home_configured()
        .set(new_config.sync_enabled(key_service));

    Ok(())
}

/// Hook to access the AppService from any component
pub fn use_app() -> AppService {
    use_context::<AppService>()
}

// =============================================================================
// Background Sync Loop
// =============================================================================

use bae_core::library_dir::LibraryDir;
use bae_core::sync::attribution::AttributionMap;
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::hlc::Timestamp;
use bae_core::sync::membership::{
    sign_membership_entry, MemberRole as CoreMemberRole, MembershipAction, MembershipChain,
    MembershipEntry,
};
use bae_core::sync::service::SyncService;
use bae_core::sync::session::SyncSession;
use bae_core::sync::status::build_sync_status;

/// Path for staging outgoing changeset bytes that survived a push failure.
fn staging_path(library_dir: &LibraryDir) -> std::path::PathBuf {
    library_dir.join("sync_staging.bin")
}

/// Stage outgoing changeset bytes to disk before pushing, so they can be
/// retried if the push fails.
fn stage_changeset(library_dir: &LibraryDir, packed: &[u8]) {
    if let Err(e) = std::fs::write(staging_path(library_dir), packed) {
        tracing::error!("Failed to stage outgoing changeset: {e}");
    }
}

/// Clear the staged changeset after a successful push.
fn clear_staged_changeset(library_dir: &LibraryDir) {
    let _ = std::fs::remove_file(staging_path(library_dir));
}

/// Read a previously staged changeset (if any) for retry.
fn read_staged_changeset(library_dir: &LibraryDir) -> Option<Vec<u8>> {
    let path = staging_path(library_dir);
    if path.exists() {
        match std::fs::read(&path) {
            Ok(data) if !data.is_empty() => Some(data),
            Ok(_) => {
                clear_staged_changeset(library_dir);
                None
            }
            Err(e) => {
                tracing::warn!("Failed to read staged changeset: {e}");
                clear_staged_changeset(library_dir);
                None
            }
        }
    } else {
        None
    }
}

/// Push a changeset to the sync bucket and update the device head.
async fn push_changeset(
    bucket: &dyn SyncBucketClient,
    device_id: &str,
    seq: u64,
    packed: Vec<u8>,
    snapshot_seq: Option<u64>,
    timestamp: &str,
) -> Result<(), bae_core::sync::bucket::BucketError> {
    bucket.put_changeset(device_id, seq, packed).await?;

    bucket
        .put_head(device_id, seq, snapshot_seq, timestamp)
        .await?;

    Ok(())
}

/// The main sync loop. Runs until the trigger channel closes.
async fn run_sync_loop(
    sync_handle: &SyncHandle,
    user_keypair: &UserKeypair,
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    imgs: &ImageServerHandle,
    library_dir: &LibraryDir,
    trigger_rx: &mut tokio::sync::mpsc::Receiver<()>,
) {
    let db = library_manager.get().database();
    let device_id = &sync_handle.device_id;
    let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;
    let hlc = &sync_handle.hlc;
    let sync_service = SyncService::new(device_id.clone());

    // Load persisted sync state
    let mut local_seq = db
        .get_sync_state("local_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let mut snapshot_seq: Option<u64> = db
        .get_sync_state("snapshot_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    let mut last_snapshot_time: Option<chrono::DateTime<chrono::Utc>> = db
        .get_sync_state("last_snapshot_time")
        .await
        .ok()
        .flatten()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    // If there's a staged changeset from a previous failed push, retry it first
    let mut staged_seq: Option<u64> = db
        .get_sync_state("staged_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    // Run an initial sync after a short delay to avoid racing with app startup
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    loop {
        // Run a sync cycle
        let result = run_sync_cycle(
            sync_handle,
            &sync_service,
            user_keypair,
            state,
            db,
            bucket,
            hlc,
            device_id,
            library_dir,
            &mut local_seq,
            &mut snapshot_seq,
            &mut staged_seq,
        )
        .await;

        match result {
            Ok(sync_outcome) => {
                let now = chrono::Utc::now().to_rfc3339();

                // Update sync status in the store
                let other_devices: Vec<DeviceActivityInfo> = sync_outcome
                    .status
                    .other_devices
                    .iter()
                    .map(|d| DeviceActivityInfo {
                        device_id: d.device_id.clone(),
                        last_seq: d.last_seq,
                        last_sync: d.last_sync.clone(),
                    })
                    .collect();

                {
                    let mut sync_lens = state.sync();
                    let mut ss = sync_lens.write();
                    ss.last_sync_time = Some(now.clone());
                    ss.other_devices = other_devices;
                    ss.syncing = false;
                    ss.error = None;
                }

                // Refresh membership list from bucket
                let user_pubkey_hex = hex::encode(user_keypair.public_key);
                match load_membership_from_bucket(bucket, Some(&user_pubkey_hex)).await {
                    Ok(members) => state.sync().members().set(members),
                    Err(e) => tracing::warn!("Failed to load membership after sync: {e}"),
                }

                // Persist snapshot_seq (local_seq is persisted in run_sync_cycle after push)
                if let Some(ss) = snapshot_seq {
                    let _ = db.set_sync_state("snapshot_seq", &ss.to_string()).await;
                }

                // If remote changes were applied, reload the UI and notify subscribers
                if sync_outcome.changesets_applied > 0 {
                    load_library(state, library_manager, imgs).await;

                    // Refresh album detail if currently viewing one
                    let album_id = state
                        .album_detail()
                        .album()
                        .read()
                        .as_ref()
                        .map(|a| a.id.clone());
                    let release_id = state.album_detail().selected_release_id().read().clone();
                    if let Some(aid) = album_id {
                        load_album_detail(
                            state,
                            library_manager,
                            &aid,
                            release_id.as_deref(),
                            imgs,
                        )
                        .await;
                    }

                    library_manager.get().notify_albums_changed();
                }

                // Check snapshot policy
                let hours_since = last_snapshot_time.map(|t| {
                    let elapsed = chrono::Utc::now().signed_duration_since(t);
                    elapsed.num_hours().max(0) as u64
                });

                if bae_core::sync::snapshot::should_create_snapshot(
                    local_seq,
                    snapshot_seq,
                    hours_since,
                ) {
                    tracing::info!("Snapshot policy triggered, creating snapshot");

                    let temp_dir = std::env::temp_dir();
                    let snapshot_result = {
                        let enc = sync_handle.encryption.read().unwrap();
                        unsafe {
                            bae_core::sync::snapshot::create_snapshot(
                                sync_handle.raw_db(),
                                &temp_dir,
                                &enc,
                            )
                        }
                    };
                    match snapshot_result {
                        Ok(encrypted) => {
                            match bae_core::sync::snapshot::push_snapshot(
                                bucket, encrypted, device_id, local_seq,
                            )
                            .await
                            {
                                Ok(()) => {
                                    snapshot_seq = Some(local_seq);
                                    last_snapshot_time = Some(chrono::Utc::now());

                                    let _ = db
                                        .set_sync_state("snapshot_seq", &local_seq.to_string())
                                        .await;
                                    let _ = db
                                        .set_sync_state(
                                            "last_snapshot_time",
                                            &chrono::Utc::now().to_rfc3339(),
                                        )
                                        .await;

                                    tracing::info!(local_seq, "Snapshot created and pushed");
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to push snapshot: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to create snapshot: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                tracing::warn!("Sync cycle failed: {error_msg}");

                state.sync().syncing().set(false);
                state.sync().error().set(Some(error_msg));
            }
        }

        // Wait for next trigger: 30-second timer or manual trigger
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
            msg = trigger_rx.recv() => {
                if msg.is_none() {
                    // Channel closed, stop the loop
                    tracing::info!("Sync trigger channel closed, stopping sync loop");
                    break;
                }
            }
        }
    }
}

/// Outcome of a successful sync cycle.
struct SyncOutcome {
    status: bae_core::sync::status::SyncStatus,
    changesets_applied: u64,
}

/// Run a single sync cycle: grab changeset, push, pull, restart session.
async fn run_sync_cycle(
    sync_handle: &SyncHandle,
    sync_service: &SyncService,
    user_keypair: &UserKeypair,
    state: &Store<AppState>,
    db: &bae_core::db::Database,
    bucket: &dyn SyncBucketClient,
    hlc: &bae_core::sync::hlc::Hlc,
    device_id: &str,
    library_dir: &LibraryDir,
    local_seq: &mut u64,
    snapshot_seq: &mut Option<u64>,
    staged_seq: &mut Option<u64>,
) -> Result<SyncOutcome, String> {
    state.sync().syncing().set(true);

    // If there's a staged changeset from a previous failed push, retry it first
    if let Some(seq) = *staged_seq {
        if let Some(staged_data) = read_staged_changeset(library_dir) {
            let timestamp = hlc.now().to_string();

            tracing::info!(seq, "Retrying staged changeset push");

            match push_changeset(
                bucket,
                device_id,
                seq,
                staged_data,
                *snapshot_seq,
                &timestamp,
            )
            .await
            {
                Ok(()) => {
                    tracing::info!(seq, "Staged changeset push succeeded");
                    clear_staged_changeset(library_dir);
                    *staged_seq = None;
                    *local_seq = seq;
                    let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                    let _ = db.set_sync_state("staged_seq", "").await;
                }
                Err(e) => {
                    // Push still failing -- leave staged data for next cycle
                    return Err(format!("Staged changeset push failed: {e}"));
                }
            }
        } else {
            // Staging file is gone but staged_seq is set -- clear the stale state
            *staged_seq = None;
            let _ = db.set_sync_state("staged_seq", "").await;
        }
    }

    // Load current cursors from DB
    let cursors = db
        .get_all_sync_cursors()
        .await
        .map_err(|e| format!("Failed to load sync cursors: {e}"))?;

    // Take the current session from the sync handle.
    // If no session exists (shouldn't happen, but handle gracefully), create a new one.
    let session = match sync_handle.session.lock().await.take() {
        Some(s) => s,
        None => {
            tracing::warn!("Sync session was None, creating a new one");
            unsafe { SyncSession::start(sync_handle.raw_db()) }
                .map_err(|e| format!("Failed to create replacement sync session: {e}"))?
        }
    };

    let timestamp = hlc.now().to_string();

    // Run the core sync cycle: grab changeset, drop session, pull remote changes
    let sync_result = unsafe {
        sync_service
            .sync(
                sync_handle.raw_db(),
                session,
                *local_seq,
                &cursors,
                bucket,
                &timestamp,
                "background sync",
                user_keypair,
                None, // membership_chain: solo/legacy library for now
                library_dir,
            )
            .await
    };

    let sync_result = match sync_result {
        Ok(r) => r,
        Err(e) => {
            // Try to restart the session even if the cycle failed
            match unsafe { SyncSession::start(sync_handle.raw_db()) } {
                Ok(new_session) => {
                    *sync_handle.session.lock().await = Some(new_session);
                }
                Err(session_err) => {
                    tracing::error!("Failed to restart sync session after error: {session_err}");
                }
            }
            return Err(format!("Sync cycle error: {e}"));
        }
    };

    // Handle outgoing changeset (push)
    if let Some(outgoing) = &sync_result.outgoing {
        let seq = outgoing.seq;

        // Stage before pushing so bytes survive a push failure
        stage_changeset(library_dir, &outgoing.packed);
        *staged_seq = Some(seq);
        let _ = db.set_sync_state("staged_seq", &seq.to_string()).await;

        match push_changeset(
            bucket,
            device_id,
            seq,
            outgoing.packed.clone(),
            *snapshot_seq,
            &timestamp,
        )
        .await
        {
            Ok(()) => {
                clear_staged_changeset(library_dir);
                *staged_seq = None;
                *local_seq = seq;
                let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                let _ = db.set_sync_state("staged_seq", "").await;

                tracing::info!(seq, "Pushed changeset");
            }
            Err(e) => {
                tracing::warn!(seq, "Push failed, changeset staged for retry: {e}");
                // staged_seq is already set; it will be retried next cycle
            }
        }
    }

    // Persist updated cursors
    for (cursor_device_id, cursor_seq) in &sync_result.updated_cursors {
        if let Err(e) = db.set_sync_cursor(cursor_device_id, *cursor_seq).await {
            tracing::warn!(
                device_id = cursor_device_id,
                seq = cursor_seq,
                "Failed to persist sync cursor: {e}"
            );
        }
    }

    // Update HLC with max remote timestamp from pull results
    let max_remote_ts = sync_result
        .pull
        .remote_heads
        .iter()
        .filter(|h| h.device_id != device_id)
        .filter_map(|h| h.last_sync.as_deref())
        .filter_map(|ts_str| {
            // The last_sync field is RFC 3339, not HLC format.
            // Parse the wall clock time and create a Timestamp for HLC update.
            chrono::DateTime::parse_from_rfc3339(ts_str)
                .ok()
                .map(|dt| dt.timestamp_millis().max(0) as u64)
        })
        .max();

    if let Some(remote_millis) = max_remote_ts {
        let remote_ts = Timestamp::new(remote_millis, 0, "remote".to_string());
        hlc.update(&remote_ts);
    }

    // Start a new sync session
    match unsafe { SyncSession::start(sync_handle.raw_db()) } {
        Ok(new_session) => {
            *sync_handle.session.lock().await = Some(new_session);
        }
        Err(e) => {
            tracing::error!("Failed to start new sync session: {e}");
            return Err(format!("Failed to restart sync session: {e}"));
        }
    }

    // Build sync status from remote heads
    let now = chrono::Utc::now().to_rfc3339();
    let status = build_sync_status(&sync_result.pull.remote_heads, device_id, Some(&now));

    Ok(SyncOutcome {
        status,
        changesets_applied: sync_result.pull.changesets_applied,
    })
}

/// Download membership entries from the bucket and build the display member list.
///
/// Returns an empty Vec if no membership chain exists (solo library).
async fn load_membership_from_bucket(
    bucket: &dyn SyncBucketClient,
    user_pubkey: Option<&str>,
) -> Result<Vec<Member>, String> {
    let entry_keys = bucket
        .list_membership_entries()
        .await
        .map_err(|e| format!("Failed to list membership entries: {e}"))?;

    if entry_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut raw_entries = Vec::new();
    for (author, seq) in &entry_keys {
        let data = bucket
            .get_membership_entry(author, *seq)
            .await
            .map_err(|e| format!("Failed to get membership entry {author}/{seq}: {e}"))?;

        let entry: MembershipEntry = serde_json::from_slice(&data)
            .map_err(|e| format!("Failed to parse membership entry {author}/{seq}: {e}"))?;
        raw_entries.push(entry);
    }

    let chain = MembershipChain::from_entries(raw_entries)
        .map_err(|e| format!("Invalid membership chain: {e}"))?;

    let attribution = AttributionMap::from_membership_chain(&chain);
    let current = chain.current_members();

    let members = current
        .into_iter()
        .map(|(pubkey, role)| {
            let display_name = attribution.display_name(&pubkey);
            let is_self = user_pubkey.is_some_and(|pk| pk == pubkey);
            let ui_role = match role {
                CoreMemberRole::Owner => MemberRole::Owner,
                CoreMemberRole::Member => MemberRole::Member,
            };
            Member {
                pubkey,
                display_name,
                role: ui_role,
                is_self,
            }
        })
        .collect();

    Ok(members)
}

/// Create a share grant JSON token for a release.
async fn create_share_grant_async(
    library_manager: &SharedLibraryManager,
    key_service: &KeyService,
    config: &config::Config,
    user_keypair: Option<&UserKeypair>,
    release_id: &str,
    recipient_pubkey_hex: &str,
) -> Result<String, String> {
    let keypair = user_keypair
        .ok_or("No user keypair configured. Set up your identity in Settings first.")?;

    let encryption_service = library_manager
        .get()
        .encryption_service()
        .cloned()
        .ok_or("Encryption is not configured.")?;

    // Verify the release is managed in the cloud.
    let release = library_manager
        .get()
        .database()
        .get_release_by_id(release_id)
        .await
        .map_err(|e| format!("Failed to look up release: {e}"))?
        .ok_or("Release not found.")?;

    if !release.managed_in_cloud {
        return Err("Release must be managed in the cloud to share.".to_string());
    }

    // Use cloud home config for bucket/region/endpoint.
    let bucket = config
        .cloud_home_s3_bucket
        .as_deref()
        .ok_or("Cloud home S3 bucket not configured.")?;
    let region = config
        .cloud_home_s3_region
        .as_deref()
        .ok_or("Cloud home S3 region not configured.")?;
    let endpoint = config.cloud_home_s3_endpoint.as_deref();

    // Read S3 credentials from keyring.
    let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
        Some(bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        }) => (Some(access_key), Some(secret_key)),
        _ => (None, None),
    };

    let grant = bae_core::sync::share_grant::create_share_grant(
        keypair,
        recipient_pubkey_hex,
        &encryption_service,
        &config.library_id,
        release_id,
        bucket,
        region,
        endpoint,
        access_key.as_deref(),
        secret_key.as_deref(),
        None, // no expiry for v1
    )
    .map_err(|e| format!("{e}"))?;

    serde_json::to_string_pretty(&grant).map_err(|e| format!("Failed to serialize grant: {e}"))
}

async fn create_share_link_async(
    library_manager: &SharedLibraryManager,
    key_service: &KeyService,
    config: &config::Config,
    release_id: &str,
) -> Result<String, String> {
    use bae_core::cloud_home;
    use bae_core::encryption::{generate_random_key, EncryptionService};
    use bae_core::sync::share_format;
    use base64::Engine;

    // 1. Verify release exists and is managed in the cloud
    let db = library_manager.get().database();
    let release = db
        .get_release_by_id(release_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?
        .ok_or("Release not found.")?;
    if !release.managed_in_cloud {
        return Err("Release must be managed in the cloud to share.".to_string());
    }

    // 2. Get album metadata
    let album = db
        .get_album_by_id(&release.album_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?
        .ok_or("Album not found.")?;
    let artists = db
        .get_artists_for_album(&release.album_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?;
    let artist_name = artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Unknown Artist".to_string());

    // 3. Get tracks and their file mappings
    let tracks = db
        .get_tracks_for_release(release_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?;
    let files = db
        .get_files_for_release(release_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?;

    // 4. Build track list with file keys
    let mut share_tracks = Vec::new();
    let mut manifest_files = Vec::new();

    for track in &tracks {
        let audio_format = db
            .get_audio_format_by_track_id(&track.id)
            .await
            .map_err(|e| format!("Database error: {e}"))?;
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

    // 5. Get cover image key
    let cover_release_id = album.cover_release_id.as_deref().unwrap_or(release_id);
    let cover_image_key = find_cover_image_key(db, cover_release_id).await;
    if let Some(ref key) = cover_image_key {
        manifest_files.push(key.clone());
    }

    // 6. Get per-release encryption key
    let encryption = library_manager
        .get()
        .encryption_service()
        .cloned()
        .ok_or("Encryption not configured.")?;
    let release_enc = encryption.derive_release_encryption(release_id);
    let release_key_b64 = base64::engine::general_purpose::STANDARD.encode(release_enc.key_bytes());

    // 7. Build ShareMeta
    let meta = share_format::ShareMeta {
        album_name: album.title,
        artist: artist_name,
        year: album.year,
        cover_image_key,
        tracks: share_tracks,
        release_key_b64,
    };
    let meta_json = serde_json::to_vec(&meta).map_err(|e| format!("Serialize error: {e}"))?;

    // 8. Generate per-share key and encrypt
    let per_share_key = generate_random_key();
    let per_share_enc = EncryptionService::from_key(per_share_key);
    let meta_encrypted = per_share_enc.encrypt_chunked(&meta_json);

    // 9. Build manifest
    let manifest = share_format::ShareManifest {
        files: manifest_files,
    };
    let manifest_json =
        serde_json::to_vec(&manifest).map_err(|e| format!("Serialize error: {e}"))?;

    // 10. Upload to cloud home
    let share_id = uuid::Uuid::new_v4().to_string();
    let cloud_home = cloud_home::create_cloud_home(config, key_service)
        .await
        .map_err(|e| format!("Cloud home error: {e}"))?;
    cloud_home
        .write(&format!("shares/{share_id}/meta.enc"), meta_encrypted)
        .await
        .map_err(|e| format!("Upload error: {e}"))?;
    cloud_home
        .write(&format!("shares/{share_id}/manifest.json"), manifest_json)
        .await
        .map_err(|e| format!("Upload error: {e}"))?;

    // 11. Build URL
    let base_url = config
        .share_base_url
        .as_deref()
        .ok_or("Share base URL not configured in settings.")?;
    let key_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(per_share_key);
    Ok(format!("{base_url}/share/{share_id}#{key_b64}"))
}

/// Find the S3 key for a release's cover image.
/// Returns `images/{ab}/{cd}/{release_id}` or None.
async fn find_cover_image_key(db: &bae_core::db::Database, release_id: &str) -> Option<String> {
    use bae_core::db::LibraryImageType;
    let image = db
        .get_library_image(release_id, &LibraryImageType::Cover)
        .await
        .ok()??;
    let hex = image.id.replace('-', "");
    Some(format!("images/{}/{}/{}", &hex[..2], &hex[2..4], &image.id))
}

/// Detect the iCloud Drive ubiquity container for the app.
///
/// Calls `NSFileManager.URLForUbiquityContainerIdentifier` with the app's
/// container identifier. Returns `Some(path)` to the container's `Documents/`
/// subdirectory if iCloud Drive is available, `None` otherwise.
#[cfg(target_os = "macos")]
fn detect_icloud_container() -> Option<std::path::PathBuf> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::CStr;

    unsafe {
        let nsfilemanager_class = Class::get("NSFileManager")?;
        let file_manager: *mut Object = msg_send![nsfilemanager_class, defaultManager];
        if file_manager.is_null() {
            return None;
        }

        // Create NSString for the container identifier (needs null-terminated C string)
        let nsstring_class = Class::get("NSString")?;
        let container_cstr = std::ffi::CString::new("iCloud.fm.bae.desktop").ok()?;
        let container_nsstring: *mut Object = msg_send![
            nsstring_class,
            stringWithUTF8String: container_cstr.as_ptr()
        ];

        let url: *mut Object =
            msg_send![file_manager, URLForUbiquityContainerIdentifier: container_nsstring];
        if url.is_null() {
            tracing::info!("iCloud Drive ubiquity container not available");
            return None;
        }

        // Get the path from the NSURL
        let path_nsstring: *mut Object = msg_send![url, path];
        if path_nsstring.is_null() {
            return None;
        }
        let path_cstr: *const std::ffi::c_char = msg_send![path_nsstring, UTF8String];
        if path_cstr.is_null() {
            return None;
        }
        let path_str = CStr::from_ptr(path_cstr).to_string_lossy().to_string();

        tracing::info!("iCloud Drive container detected at: {}", path_str);

        // Use Documents/ subdirectory (standard for user-visible data in ubiquity containers)
        Some(std::path::PathBuf::from(path_str).join("Documents"))
    }
}
