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
use bae_core::db::{DbStorageProfile, ImportStatus, StorageLocation};
use bae_core::image_server::ImageServerHandle;
use bae_core::import::{self, ImportProgress};
use bae_core::keys::KeyService;
use bae_core::library::{LibraryEvent, SharedLibraryManager};
use bae_core::playback::{self, PlaybackProgress};
#[cfg(feature = "torrent")]
use bae_core::torrent;
use bae_ui::display_types::{QueueItem, TrackImportState};
use bae_ui::stores::{
    ActiveImport, ActiveImportsUiStateStoreExt, AlbumDetailStateStoreExt, AppState,
    AppStateStoreExt, ArtistDetailStateStoreExt, ConfigStateStoreExt, ImportOperationStatus,
    LibraryStateStoreExt, PlaybackStatus, PlaybackUiStateStoreExt, PrepareStep,
    StorageProfilesStateStoreExt,
};
use bae_ui::StorageProfile;
use dioxus::prelude::*;
use std::collections::HashMap;

use super::app_context::AppServices;

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
        self.subscribe_cloud_sync();
        self.subscribe_folder_scan_events();
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

                        state.playback().status().set(status);
                        state
                            .playback()
                            .current_track_id()
                            .set(current_track_id.clone());
                        state.playback().current_release_id().set(release_id);
                        state.playback().position_ms().set(position_ms);
                        state.playback().duration_ms().set(duration_ms);
                        state.playback().pregap_ms().set(pregap_ms);

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
                                                    .map(|rid| imgs.cover_url(rid))
                                                    .or(album.cover_art_url.clone());
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

                        state.playback().current_track().set(current_track);
                        state.playback().artist_name().set(artist_name);
                        state.playback().artist_id().set(artist_id);
                        state.playback().cover_url().set(cover_url);
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
                        state.playback().queue().set(tracks.clone());

                        // Load track/album details for queue items
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
                                            .map(|rid| imgs.cover_url(rid))
                                            .or(album.cover_art_url.clone());
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
                        state.playback().queue_items().set(queue_items);
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
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();

        spawn(async move {
            let mut progress_rx = import_handle.subscribe_all_imports();
            while let Some(event) = progress_rx.recv().await {
                // Reload library when import completes
                let should_reload = matches!(event, ImportProgress::Complete { .. });

                handle_import_progress(&state, event);

                if should_reload {
                    load_library(&state, &library_manager, &imgs).await;
                }
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

    /// Subscribe to library changes and auto-upload to cloud sync (2s debounce)
    fn subscribe_cloud_sync(&self) {
        let app = self.clone();

        spawn(async move {
            let mut rx = app.library_manager.get().subscribe_events();

            loop {
                // Wait for an AlbumsChanged event
                match rx.recv().await {
                    Ok(LibraryEvent::AlbumsChanged) => {}
                    Err(_) => break,
                }

                // Check if cloud sync is enabled
                if !app.config.cloud_sync_enabled {
                    continue;
                }

                // Debounce: wait 2s, draining any additional events
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                while rx.try_recv().is_ok() {}

                // Trigger upload
                tracing::info!("Auto cloud sync triggered by library change");

                app.state
                    .config()
                    .cloud_sync_status()
                    .set(bae_ui::stores::CloudSyncStatus::Syncing);

                match cloud_sync_upload(&app).await {
                    Ok(timestamp) => {
                        app.save_config(|c| {
                            c.cloud_sync_last_upload = Some(timestamp.clone());
                        });
                        app.state
                            .config()
                            .cloud_sync_last_upload()
                            .set(Some(timestamp));
                        app.state
                            .config()
                            .cloud_sync_status()
                            .set(bae_ui::stores::CloudSyncStatus::Idle);
                    }
                    Err(e) => {
                        tracing::error!("Auto cloud sync failed: {}", e);

                        app.state
                            .config()
                            .cloud_sync_status()
                            .set(bae_ui::stores::CloudSyncStatus::Error(e.to_string()));
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

    /// Load initial data from database
    fn load_initial_data(&self) {
        self.state.playback().volume().set(1.0);
        self.load_config();
        self.load_active_imports();
        self.load_library();
        self.load_storage_profiles();
    }

    /// Process any pending file deletions from previous transfers
    fn process_pending_deletions(&self) {
        let library_dir = self.config.library_dir.clone();
        let library_manager = self.library_manager.clone();

        spawn(async move {
            bae_core::storage::cleanup::process_pending_deletions(&library_dir, library_manager)
                .await;
        });
    }

    /// Load config into Store
    fn load_config(&self) {
        let config = &self.config;
        self.state
            .config()
            .discogs_key_stored()
            .set(config.discogs_key_stored);
        self.state
            .config()
            .encryption_key_stored()
            .set(config.encryption_key_stored);
        self.state
            .config()
            .encryption_key_fingerprint()
            .set(config.encryption_key_fingerprint.clone());
        self.state
            .config()
            .subsonic_enabled()
            .set(config.subsonic_enabled);
        self.state
            .config()
            .subsonic_port()
            .set(config.subsonic_port);
        self.state
            .config()
            .torrent_bind_interface()
            .set(config.torrent_bind_interface.clone());
        self.state
            .config()
            .torrent_listen_port()
            .set(config.torrent_listen_port);
        self.state
            .config()
            .torrent_enable_upnp()
            .set(config.torrent_enable_upnp);
        self.state
            .config()
            .torrent_max_connections()
            .set(config.torrent_max_connections);
        self.state
            .config()
            .torrent_max_connections_per_torrent()
            .set(config.torrent_max_connections_per_torrent);
        self.state
            .config()
            .torrent_max_uploads()
            .set(config.torrent_max_uploads);
        self.state
            .config()
            .torrent_max_uploads_per_torrent()
            .set(config.torrent_max_uploads_per_torrent);
        self.state
            .config()
            .cloud_sync_enabled()
            .set(config.cloud_sync_enabled);
        self.state
            .config()
            .cloud_sync_bucket()
            .set(config.cloud_sync_bucket.clone());
        self.state
            .config()
            .cloud_sync_region()
            .set(config.cloud_sync_region.clone());
        self.state
            .config()
            .cloud_sync_endpoint()
            .set(config.cloud_sync_endpoint.clone());
        self.state
            .config()
            .cloud_sync_last_upload()
            .set(config.cloud_sync_last_upload.clone());
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
                            cover_art_url: None,
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
    fn load_library(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let imgs = self.image_server.clone();

        spawn(async move {
            load_library(&state, &library_manager, &imgs).await;
        });
    }

    // =========================================================================
    // Album Detail Methods
    // =========================================================================

    /// Load album detail data into Store (called when navigating to album page)
    pub fn load_album_detail(&self, album_id: &str, release_id: Option<&str>) {
        let state = self.state;
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
            let busted_url = format!("{}&t={}", imgs.cover_url(&release_id), timestamp);
            let mut albums_lens = state.library().albums();
            let mut albums = albums_lens.write();
            if let Some(album) = albums.iter_mut().find(|a| a.id == album_id) {
                album.cover_url = Some(busted_url);
            }
        });
    }

    /// Transfer a release to a different storage profile
    pub fn transfer_release_storage(&self, release_id: &str, target_profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let config = self.config.clone();
        let release_id = release_id.to_string();
        let target_profile_id = target_profile_id.to_string();
        let imgs = self.image_server.clone();

        spawn(async move {
            // Look up the target profile
            let target_profile = match library_manager
                .get()
                .database()
                .get_storage_profile(&target_profile_id)
                .await
            {
                Ok(Some(p)) => p,
                Ok(None) => {
                    state
                        .album_detail()
                        .transfer_error()
                        .set(Some("Target storage profile not found".into()));
                    return;
                }
                Err(e) => {
                    state
                        .album_detail()
                        .transfer_error()
                        .set(Some(format!("Failed to load target profile: {}", e)));
                    return;
                }
            };

            let encryption_service = library_manager.get().encryption_service().cloned();
            let library_dir = config.library_dir.clone();

            let transfer_service = bae_core::storage::transfer::TransferService::new(
                library_manager.clone(),
                encryption_service,
                library_dir.clone(),
            );

            let mut rx = transfer_service.transfer(
                release_id.clone(),
                bae_core::storage::transfer::TransferTarget::Profile(target_profile),
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
                        bae_core::storage::cleanup::schedule_cleanup(
                            &library_dir,
                            library_manager.clone(),
                        );
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
                        bae_core::storage::cleanup::schedule_cleanup(
                            &library_dir,
                            library_manager.clone(),
                        );
                    }
                    bae_core::storage::transfer::TransferProgress::Failed { error, .. } => {
                        state.album_detail().transfer_progress().set(None);
                        state.album_detail().transfer_error().set(Some(error));
                    }
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

        // Update Store
        self.state
            .config()
            .discogs_key_stored()
            .set(new_config.discogs_key_stored);
        self.state
            .config()
            .encryption_key_stored()
            .set(new_config.encryption_key_stored);
        self.state
            .config()
            .encryption_key_fingerprint()
            .set(new_config.encryption_key_fingerprint.clone());
        self.state
            .config()
            .subsonic_enabled()
            .set(new_config.subsonic_enabled);
        self.state
            .config()
            .subsonic_port()
            .set(new_config.subsonic_port);
        self.state
            .config()
            .torrent_bind_interface()
            .set(new_config.torrent_bind_interface.clone());
        self.state
            .config()
            .torrent_listen_port()
            .set(new_config.torrent_listen_port);
        self.state
            .config()
            .torrent_enable_upnp()
            .set(new_config.torrent_enable_upnp);
        self.state
            .config()
            .torrent_max_connections()
            .set(new_config.torrent_max_connections);
        self.state
            .config()
            .torrent_max_connections_per_torrent()
            .set(new_config.torrent_max_connections_per_torrent);
        self.state
            .config()
            .torrent_max_uploads()
            .set(new_config.torrent_max_uploads);
        self.state
            .config()
            .torrent_max_uploads_per_torrent()
            .set(new_config.torrent_max_uploads_per_torrent);
        self.state
            .config()
            .cloud_sync_enabled()
            .set(new_config.cloud_sync_enabled);
        self.state
            .config()
            .cloud_sync_bucket()
            .set(new_config.cloud_sync_bucket.clone());
        self.state
            .config()
            .cloud_sync_region()
            .set(new_config.cloud_sync_region.clone());
        self.state
            .config()
            .cloud_sync_endpoint()
            .set(new_config.cloud_sync_endpoint.clone());
        self.state
            .config()
            .cloud_sync_last_upload()
            .set(new_config.cloud_sync_last_upload.clone());
    }

    // =========================================================================
    // Storage Profile Methods
    // =========================================================================

    /// Load storage profiles from database into Store
    pub fn load_storage_profiles(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();

        spawn(async move {
            state.storage_profiles().loading().set(true);
            state.storage_profiles().error().set(None);

            match library_manager.get_all_storage_profiles().await {
                Ok(db_profiles) => {
                    let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                    state.storage_profiles().profiles().set(profiles);
                }
                Err(e) => {
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to load storage profiles: {}", e)));
                }
            }

            state.storage_profiles().loading().set(false);
        });
    }

    /// Save (create or update) a storage profile
    pub fn save_storage_profile(&self, profile: StorageProfile) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let is_new = profile.id.is_empty();

        spawn(async move {
            let result = if is_new {
                let location = storage_location_from_display(profile.location);
                let db_profile = if location == StorageLocation::Local {
                    DbStorageProfile::new_local(
                        &profile.name,
                        &profile.location_path,
                        profile.encrypted,
                    )
                } else {
                    DbStorageProfile::new_cloud(
                        &profile.name,
                        profile.cloud_bucket.as_deref().unwrap_or(""),
                        profile.cloud_region.as_deref().unwrap_or(""),
                        profile.cloud_endpoint.as_deref(),
                        profile.cloud_access_key.as_deref().unwrap_or(""),
                        profile.cloud_secret_key.as_deref().unwrap_or(""),
                        profile.encrypted,
                    )
                }
                .with_default(profile.is_default);
                library_manager.insert_storage_profile(&db_profile).await
            } else {
                let mut db_profile = DbStorageProfile {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    location: storage_location_from_display(profile.location),
                    location_path: profile.location_path.clone(),
                    encrypted: profile.encrypted,
                    is_default: profile.is_default,
                    cloud_bucket: profile.cloud_bucket.clone(),
                    cloud_region: profile.cloud_region.clone(),
                    cloud_endpoint: profile.cloud_endpoint.clone(),
                    cloud_access_key: profile.cloud_access_key.clone(),
                    cloud_secret_key: profile.cloud_secret_key.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };

                if db_profile.location == StorageLocation::Local {
                    db_profile.cloud_bucket = None;
                    db_profile.cloud_region = None;
                    db_profile.cloud_endpoint = None;
                    db_profile.cloud_access_key = None;
                    db_profile.cloud_secret_key = None;
                }

                library_manager.update_storage_profile(&db_profile).await
            };

            match result {
                Ok(()) => {
                    tracing::info!("Saved storage profile: {}", profile.name);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to save storage profile: {}", e);
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to save: {}", e)));
                }
            }
        });
    }

    /// Delete a storage profile
    pub fn delete_storage_profile(&self, profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let profile_id = profile_id.to_string();

        spawn(async move {
            state.storage_profiles().error().set(None);

            match library_manager.delete_storage_profile(&profile_id).await {
                Ok(()) => {
                    tracing::info!("Deleted storage profile: {}", profile_id);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to delete storage profile: {}", e);

                    // Extract a clean user-facing message. The inner sqlx::Error::Protocol
                    // wrapping adds confusing "server backend" framing, so we pull out
                    // our business-logic message when present.
                    let full = e.to_string();
                    let user_msg = if let Some(pos) = full.find("Cannot delete") {
                        full[pos..].to_string()
                    } else {
                        format!("Failed to delete storage profile: {}", e)
                    };
                    state.storage_profiles().error().set(Some(user_msg));
                }
            }
        });
    }

    /// Set a storage profile as default
    pub fn set_default_storage_profile(&self, profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let profile_id = profile_id.to_string();

        spawn(async move {
            match library_manager
                .set_default_storage_profile(&profile_id)
                .await
            {
                Ok(()) => {
                    tracing::info!("Set default storage profile: {}", profile_id);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles.iter().map(storage_profile_from_db).collect();
                        state.storage_profiles().profiles().set(profiles);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to set default storage profile: {}", e);
                    state
                        .storage_profiles()
                        .error()
                        .set(Some(format!("Failed to set default: {}", e)));
                }
            }
        });
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert DbStorageProfile to display StorageProfile
fn storage_profile_from_db(p: &DbStorageProfile) -> StorageProfile {
    StorageProfile {
        id: p.id.clone(),
        name: p.name.clone(),
        location: storage_location_to_display(p.location),
        location_path: p.location_path.clone(),
        encrypted: p.encrypted,
        is_default: p.is_default,
        cloud_bucket: p.cloud_bucket.clone(),
        cloud_region: p.cloud_region.clone(),
        cloud_endpoint: p.cloud_endpoint.clone(),
        cloud_access_key: p.cloud_access_key.clone(),
        cloud_secret_key: p.cloud_secret_key.clone(),
    }
}

/// Convert StorageLocation to display type
fn storage_location_to_display(loc: StorageLocation) -> bae_ui::StorageLocation {
    match loc {
        StorageLocation::Local => bae_ui::StorageLocation::Local,
        StorageLocation::Cloud => bae_ui::StorageLocation::Cloud,
    }
}

/// Convert display StorageLocation to DB type
fn storage_location_from_display(loc: bae_ui::StorageLocation) -> StorageLocation {
    match loc {
        bae_ui::StorageLocation::Local => StorageLocation::Local,
        bae_ui::StorageLocation::Cloud => StorageLocation::Cloud,
    }
}

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

            state.library().albums().set(display_albums);
            state.library().artists_by_album().set(artists_map);
        }
        Err(e) => {
            state
                .library()
                .error()
                .set(Some(format!("Failed to load library: {}", e)));
        }
    }

    state.library().loading().set(false);
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

    // Load album
    let album = match library_manager.get().get_album_by_id(album_id).await {
        Ok(Some(db_album)) => Some(album_from_db_ref(&db_album, imgs)),
        Ok(None) => {
            state
                .album_detail()
                .error()
                .set(Some("Album not found".to_string()));
            state.album_detail().loading().set(false);
            return;
        }
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load album: {}", e)));
            state.album_detail().loading().set(false);
            return;
        }
    };
    state.album_detail().album().set(album);

    // Load releases
    let releases = match library_manager.get().get_releases_for_album(album_id).await {
        Ok(db_releases) => db_releases,
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load releases: {}", e)));
            state.album_detail().loading().set(false);
            return;
        }
    };

    if releases.is_empty() {
        state
            .album_detail()
            .error()
            .set(Some("Album has no releases".to_string()));
        state.album_detail().loading().set(false);
        return;
    }

    // Determine selected release
    let selected_release = if let Some(rid) = release_id_param {
        releases
            .iter()
            .find(|r| r.id == rid)
            .unwrap_or(&releases[0])
    } else {
        &releases[0]
    };
    let selected_release_id = selected_release.id.clone();

    // Check if release is importing
    let is_importing = selected_release.import_status == ImportStatus::Importing
        || selected_release.import_status == ImportStatus::Queued;
    if is_importing {
        // Progress will be updated by import subscription
        state.album_detail().import_progress().set(Some(0));
    }

    let display_releases = releases.iter().map(release_from_db_ref).collect();
    state.album_detail().releases().set(display_releases);
    state
        .album_detail()
        .selected_release_id()
        .set(Some(selected_release_id.clone()));

    // Load artists
    if let Ok(db_artists) = library_manager.get().get_artists_for_album(album_id).await {
        let artists = db_artists
            .iter()
            .map(|a| artist_from_db_ref(a, imgs))
            .collect();
        state.album_detail().artists().set(artists);
    }

    // Load tracks for selected release (sorted by disc/track number)
    match library_manager.get().get_tracks(&selected_release_id).await {
        Ok(db_tracks) => {
            let mut tracks: Vec<_> = db_tracks.iter().map(track_from_db_ref).collect();
            tracks.sort_by(|a, b| {
                (a.disc_number, a.track_number).cmp(&(b.disc_number, b.track_number))
            });

            // Set derived fields first to avoid subscribing to tracks for count/ids/disc info
            let track_count = tracks.len();
            let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
            let track_disc_info: Vec<(Option<i32>, String)> = tracks
                .iter()
                .map(|t| (t.disc_number, t.id.clone()))
                .collect();
            state.album_detail().track_count().set(track_count);
            state.album_detail().track_ids().set(track_ids);
            state.album_detail().track_disc_info().set(track_disc_info);
            state.album_detail().tracks().set(tracks);
        }
        Err(e) => {
            state
                .album_detail()
                .error()
                .set(Some(format!("Failed to load tracks: {}", e)));
        }
    }

    // Load files for selected release
    if let Ok(db_files) = library_manager
        .get()
        .get_files_for_release(&selected_release_id)
        .await
    {
        let files = db_files.iter().map(file_from_db_ref).collect();
        state.album_detail().files().set(files);
    }

    // Load gallery images from release files (images are just files with image content types)
    if let Ok(db_files) = library_manager
        .get()
        .get_files_for_release(&selected_release_id)
        .await
    {
        let images = db_files
            .iter()
            .filter(|f| f.content_type.is_image())
            .map(|f| bae_ui::Image {
                id: f.id.clone(),
                filename: f.original_filename.clone(),
                url: imgs.file_url(&f.id),
            })
            .collect();
        state.album_detail().images().set(images);
    }

    // Load storage profile for selected release
    let storage_profile = library_manager
        .get()
        .get_storage_profile_for_release(&selected_release_id)
        .await
        .ok()
        .flatten()
        .map(|p| storage_profile_from_db(&p));
    state.album_detail().storage_profile().set(storage_profile);
    state.album_detail().transfer_progress().set(None);
    state.album_detail().transfer_error().set(None);
    state.album_detail().remote_covers().set(vec![]);
    state.album_detail().loading_remote_covers().set(false);

    state.album_detail().loading().set(false);
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

            // Read the file bytes from source_path
            let source_path = file
                .source_path
                .as_ref()
                .ok_or_else(|| "File has no source_path".to_string())?;
            let bytes =
                std::fs::read(source_path).map_err(|e| format!("Failed to read file: {}", e))?;

            let content_type = file.content_type.clone();

            // Write to covers/{release_id}
            let cover_path = library_dir.cover_path(release_id);
            if let Some(parent) = cover_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create covers dir: {}", e))?;
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

            // Write to covers/{release_id}
            let cover_path = library_dir.cover_path(release_id);
            if let Some(parent) = cover_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create covers dir: {}", e))?;
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

/// Load artist detail data into the Store
async fn load_artist_detail(
    state: &Store<AppState>,
    library_manager: &SharedLibraryManager,
    artist_id: &str,
    imgs: &ImageServerHandle,
) {
    state.artist_detail().loading().set(true);
    state.artist_detail().error().set(None);

    // Load artist
    match library_manager.get().get_artist_by_id(artist_id).await {
        Ok(Some(db_artist)) => {
            state
                .artist_detail()
                .artist()
                .set(Some(artist_from_db_ref(&db_artist, imgs)));
        }
        Ok(None) => {
            state
                .artist_detail()
                .error()
                .set(Some("Artist not found".to_string()));
            state.artist_detail().loading().set(false);
            return;
        }
        Err(e) => {
            state
                .artist_detail()
                .error()
                .set(Some(format!("Failed to load artist: {}", e)));
            state.artist_detail().loading().set(false);
            return;
        }
    };

    // Load albums for this artist
    match library_manager.get().get_albums_for_artist(artist_id).await {
        Ok(db_albums) => {
            let mut artists_map = HashMap::new();
            for album in &db_albums {
                if let Ok(db_artists) = library_manager.get().get_artists_for_album(&album.id).await
                {
                    let artists = db_artists
                        .iter()
                        .map(|a| artist_from_db_ref(a, imgs))
                        .collect();
                    artists_map.insert(album.id.clone(), artists);
                }
            }
            let display_albums = db_albums
                .iter()
                .map(|a| album_from_db_ref(a, imgs))
                .collect();

            state.artist_detail().albums().set(display_albums);
            state.artist_detail().artists_by_album().set(artists_map);
        }
        Err(e) => {
            state
                .artist_detail()
                .error()
                .set(Some(format!("Failed to load albums: {}", e)));
        }
    }

    state.artist_detail().loading().set(false);
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
            cover_art_url,
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
                        cover_art_url,
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

/// Build CloudSyncService from config + keyring, upload DB + covers.
/// Returns the upload timestamp on success.
pub(crate) async fn cloud_sync_upload(
    app: &AppService,
) -> Result<String, Box<dyn std::error::Error>> {
    use bae_core::cloud_sync::CloudSyncService;
    use bae_core::encryption::EncryptionService;

    let config = &app.config;

    let bucket = config
        .cloud_sync_bucket
        .as_ref()
        .ok_or("No bucket configured")?
        .clone();
    let region = config
        .cloud_sync_region
        .as_ref()
        .ok_or("No region configured")?
        .clone();
    let endpoint = config.cloud_sync_endpoint.clone();

    let access_key = app
        .key_service
        .get_cloud_sync_access_key()
        .ok_or("No access key configured")?;
    let secret_key = app
        .key_service
        .get_cloud_sync_secret_key()
        .ok_or("No secret key configured")?;

    let encryption_key = app
        .key_service
        .get_encryption_key()
        .ok_or("No encryption key")?;
    let encryption_service = EncryptionService::new(&encryption_key)?;

    let sync_service = CloudSyncService::new(
        bucket,
        region,
        endpoint,
        access_key,
        secret_key,
        config.library_id.clone(),
        encryption_service,
    )
    .await?;

    // Create DB snapshot via VACUUM INTO
    let db_path = config.library_dir.db_path();
    let snapshot_path = db_path.with_extension("db.snapshot");
    app.library_manager
        .database()
        .vacuum_into(snapshot_path.to_str().unwrap())
        .await?;

    // Upload DB + covers + artist images
    let timestamp = sync_service.upload_db(&db_path).await?;

    let covers_dir = config.library_dir.covers_dir();
    sync_service.upload_covers(&covers_dir).await?;

    let artists_dir = config.library_dir.artists_dir();
    sync_service.upload_artists(&artists_dir).await?;

    Ok(timestamp)
}

/// Hook to access the AppService from any component
pub fn use_app() -> AppService {
    use_context::<AppService>()
}
