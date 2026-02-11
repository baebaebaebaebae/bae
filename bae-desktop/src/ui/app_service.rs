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
use bae_core::keys::{KeyService, UserKeypair};
use bae_core::library::{LibraryEvent, SharedLibraryManager};
use bae_core::playback::{self, PlaybackProgress};
#[cfg(feature = "torrent")]
use bae_core::torrent;
use bae_ui::display_types::{QueueItem, TrackImportState};
use bae_ui::stores::{
    ActiveImport, ActiveImportsUiStateStoreExt, AlbumDetailStateStoreExt, AppState,
    AppStateStoreExt, ArtistDetailStateStoreExt, ConfigStateStoreExt, DeviceActivityInfo,
    ImportOperationStatus, LibraryStateStoreExt, Member, MemberRole, PlaybackStatus,
    PlaybackUiStateStoreExt, PrepareStep, StorageProfilesStateStoreExt, SyncStateStoreExt,
};
use bae_ui::StorageProfile;
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
                                                    .map(|rid| imgs.image_url(rid))
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
                                            .map(|rid| imgs.image_url(rid))
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
        let key_service = self.key_service.clone();

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
                &key_service,
                &mut trigger_rx,
            )
            .await;
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
        let key_service = self.key_service.clone();

        spawn(async move {
            bae_core::storage::cleanup::process_pending_deletions(
                &library_dir,
                library_manager,
                &key_service,
            )
            .await;
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

        // Sync config
        self.state
            .sync()
            .sync_bucket()
            .set(config.sync_s3_bucket.clone());
        self.state
            .sync()
            .sync_region()
            .set(config.sync_s3_region.clone());
        self.state
            .sync()
            .sync_endpoint()
            .set(config.sync_s3_endpoint.clone());
        self.state
            .sync()
            .sync_configured()
            .set(config.sync_enabled(&self.key_service));
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
        let key_service = self.key_service.clone();

        spawn(async move {
            load_album_detail(
                &state,
                &library_manager,
                &album_id,
                release_id.as_deref(),
                &imgs,
                &key_service,
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
        let key_service = self.key_service.clone();

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
                &key_service,
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

    /// Transfer a release to a different storage profile
    pub fn transfer_release_storage(&self, release_id: &str, target_profile_id: &str) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let config = self.config.clone();
        let release_id = release_id.to_string();
        let target_profile_id = target_profile_id.to_string();
        let imgs = self.image_server.clone();
        let key_service = self.key_service.clone();

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
                key_service.clone(),
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
                                &key_service,
                            )
                            .await;
                        }

                        // Schedule deferred cleanup of old files
                        bae_core::storage::cleanup::schedule_cleanup(
                            &library_dir,
                            library_manager.clone(),
                            key_service.clone(),
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
        let key_service = self.key_service.clone();

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
                key_service.clone(),
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
                                &key_service,
                            )
                            .await;
                        }

                        // Schedule deferred cleanup of old files
                        bae_core::storage::cleanup::schedule_cleanup(
                            &library_dir,
                            library_manager.clone(),
                            key_service.clone(),
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

        // Sync config might have changed via save_config too
        self.state
            .sync()
            .sync_bucket()
            .set(new_config.sync_s3_bucket.clone());
        self.state
            .sync()
            .sync_region()
            .set(new_config.sync_s3_region.clone());
        self.state
            .sync()
            .sync_endpoint()
            .set(new_config.sync_s3_endpoint.clone());
        self.state
            .sync()
            .sync_configured()
            .set(new_config.sync_enabled(&self.key_service));
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

            let result: Result<(), String> = async {
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

                bae_core::sync::invite::create_invitation(
                    bucket,
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

                Ok(())
            }
            .await;

            match result {
                Ok(()) => {
                    state
                        .sync()
                        .invite_status()
                        .set(Some(bae_ui::stores::InviteStatus::Success));

                    // Set share info for the UI.
                    state
                        .sync()
                        .share_info()
                        .set(Some(bae_ui::stores::ShareInfo {
                            bucket: config.sync_s3_bucket.clone().unwrap_or_default(),
                            region: config.sync_s3_region.clone().unwrap_or_default(),
                            endpoint: config.sync_s3_endpoint.clone(),
                            invitee_pubkey: invitee_pubkey_hex,
                        }));

                    // Reload the member list.
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
                let revoke_ts = hlc.now().to_string();
                let new_key = bae_core::sync::invite::revoke_member(
                    bucket,
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
    /// Updates the store with the new values.
    pub fn save_sync_config(&self, config_data: bae_ui::SyncBucketConfig) -> Result<(), String> {
        let state = self.state;
        let key_service = self.key_service.clone();
        let mut new_config = self.config.clone();

        new_config.sync_s3_bucket = Some(config_data.bucket.clone());
        new_config.sync_s3_region = Some(config_data.region.clone());
        new_config.sync_s3_endpoint = if config_data.endpoint.is_empty() {
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
            .set_sync_access_key(&config_data.access_key)
            .map_err(|e| format!("Failed to save access key: {}", e))?;
        key_service
            .set_sync_secret_key(&config_data.secret_key)
            .map_err(|e| format!("Failed to save secret key: {}", e))?;

        // Update store
        state
            .sync()
            .sync_bucket()
            .set(new_config.sync_s3_bucket.clone());
        state
            .sync()
            .sync_region()
            .set(new_config.sync_s3_region.clone());
        state
            .sync()
            .sync_endpoint()
            .set(new_config.sync_s3_endpoint.clone());
        state
            .sync()
            .sync_configured()
            .set(new_config.sync_enabled(&key_service));

        Ok(())
    }

    // =========================================================================
    // Storage Profile Methods
    // =========================================================================

    /// Load storage profiles from database into Store
    pub fn load_storage_profiles(&self) {
        let state = self.state;
        let library_manager = self.library_manager.clone();
        let key_service = self.key_service.clone();

        spawn(async move {
            state.storage_profiles().loading().set(true);
            state.storage_profiles().error().set(None);

            match library_manager.get_all_storage_profiles().await {
                Ok(db_profiles) => {
                    let profiles = db_profiles
                        .iter()
                        .map(|p| storage_profile_from_db(p, &key_service))
                        .collect();
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
        let key_service = self.key_service.clone();
        let is_new = profile.id.is_empty();

        spawn(async move {
            let result = if is_new {
                let location = storage_location_from_display(profile.location);
                // Cloud profiles must always be encrypted; local profiles are never encrypted
                let encrypted = location == StorageLocation::Cloud;
                let db_profile = if location == StorageLocation::Local {
                    DbStorageProfile::new_local(&profile.name, &profile.location_path, encrypted)
                } else {
                    DbStorageProfile::new_cloud(
                        &profile.name,
                        profile.cloud_bucket.as_deref().unwrap_or(""),
                        profile.cloud_region.as_deref().unwrap_or(""),
                        profile.cloud_endpoint.as_deref(),
                        encrypted,
                    )
                }
                .with_default(profile.is_default);

                // Save S3 credentials to keyring before DB insert
                if location == StorageLocation::Cloud {
                    if let Some(ref ak) = profile.cloud_access_key {
                        if let Err(e) = key_service.set_profile_access_key(&db_profile.id, ak) {
                            tracing::error!("Failed to save S3 access key: {}", e);
                        }
                    }
                    if let Some(ref sk) = profile.cloud_secret_key {
                        if let Err(e) = key_service.set_profile_secret_key(&db_profile.id, sk) {
                            tracing::error!("Failed to save S3 secret key: {}", e);
                        }
                    }
                }

                library_manager.insert_storage_profile(&db_profile).await
            } else {
                let location = storage_location_from_display(profile.location);
                // Cloud profiles must always be encrypted; local profiles are never encrypted
                let encrypted = location == StorageLocation::Cloud;
                let mut db_profile = DbStorageProfile {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    location,
                    location_path: profile.location_path.clone(),
                    encrypted,
                    is_default: profile.is_default,
                    is_home: false,
                    cloud_bucket: profile.cloud_bucket.clone(),
                    cloud_region: profile.cloud_region.clone(),
                    cloud_endpoint: profile.cloud_endpoint.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };

                if db_profile.location == StorageLocation::Local {
                    db_profile.cloud_bucket = None;
                    db_profile.cloud_region = None;
                    db_profile.cloud_endpoint = None;

                    // Clean up any stale keyring entries when switching to local
                    let _ = key_service.delete_profile_credentials(&db_profile.id);
                } else {
                    // Update S3 credentials in keyring
                    if let Some(ref ak) = profile.cloud_access_key {
                        if let Err(e) = key_service.set_profile_access_key(&db_profile.id, ak) {
                            tracing::error!("Failed to save S3 access key: {}", e);
                        }
                    }
                    if let Some(ref sk) = profile.cloud_secret_key {
                        if let Err(e) = key_service.set_profile_secret_key(&db_profile.id, sk) {
                            tracing::error!("Failed to save S3 secret key: {}", e);
                        }
                    }
                }

                library_manager.update_storage_profile(&db_profile).await
            };

            match result {
                Ok(()) => {
                    tracing::info!("Saved storage profile: {}", profile.name);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles
                            .iter()
                            .map(|p| storage_profile_from_db(p, &key_service))
                            .collect();
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
        let key_service = self.key_service.clone();
        let profile_id = profile_id.to_string();

        spawn(async move {
            state.storage_profiles().error().set(None);

            match library_manager.delete_storage_profile(&profile_id).await {
                Ok(()) => {
                    // Clean up S3 credentials from keyring
                    if let Err(e) = key_service.delete_profile_credentials(&profile_id) {
                        tracing::warn!(
                            "Failed to delete keyring credentials for profile {}: {}",
                            profile_id,
                            e
                        );
                    }

                    tracing::info!("Deleted storage profile: {}", profile_id);
                    // Reload profiles
                    if let Ok(db_profiles) = library_manager.get_all_storage_profiles().await {
                        let profiles = db_profiles
                            .iter()
                            .map(|p| storage_profile_from_db(p, &key_service))
                            .collect();
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
        let key_service = self.key_service.clone();
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
                        let profiles = db_profiles
                            .iter()
                            .map(|p| storage_profile_from_db(p, &key_service))
                            .collect();
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

/// Convert DbStorageProfile to display StorageProfile, reading credentials from keyring
fn storage_profile_from_db(
    p: &DbStorageProfile,
    key_service: &bae_core::keys::KeyService,
) -> StorageProfile {
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
        cloud_access_key: key_service.get_profile_access_key(&p.id),
        cloud_secret_key: key_service.get_profile_secret_key(&p.id),
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
    key_service: &bae_core::keys::KeyService,
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
        .map(|p| storage_profile_from_db(&p, key_service));
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
    key_service: &KeyService,
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

                state.sync().last_sync_time().set(Some(now.clone()));
                state.sync().other_devices().set(other_devices);
                state.sync().syncing().set(false);
                state.sync().error().set(None);

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
                            key_service,
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
