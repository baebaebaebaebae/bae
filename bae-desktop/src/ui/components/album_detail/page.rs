use super::back_button::BackButton;
use super::error::AlbumDetailError;
use super::loading::AlbumDetailLoading;
use super::utils::maybe_not_empty;
use super::AlbumDetailView;
use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::display_types::{CoverChange, PlaybackDisplay};
use bae_ui::stores::config::LibrarySource;
use bae_ui::stores::{
    AlbumDetailStateStoreExt, AppStateStoreExt, LibraryStateStoreExt, PlaybackStatus,
    PlaybackUiStateStoreExt,
};
use bae_ui::{ErrorToast, SuccessToast};
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use tracing::error;

/// Album detail page showing album info and tracklist
///
/// Passes state lens to AlbumDetailView - no memos, just direct lens access.
#[component]
pub fn AlbumDetail(album_id: ReadSignal<String>, release_id: ReadSignal<String>) -> Element {
    let app = use_app();

    // Read store values here (in component body) so the effect closure doesn't
    // call state.read() and subscribe its ReactiveContext to the whole store.
    let active_source = app.state.library().active_source().read().clone();

    // Load album detail data into Store on mount/param change
    use_effect({
        let app = app.clone();
        move || {
            let album_id = album_id();
            let release_id = maybe_not_empty(release_id());
            app.load_album_detail(&album_id, release_id.as_deref(), &active_source);
        }
    });

    let playback = app.playback_handle.clone();
    let library_manager = app.library_manager.clone();
    let cache = app.cache.clone();

    // Pass state lens directly - don't read here!
    let state = app.state.album_detail();
    // Pass tracks store separately for per-track reactivity via .iter()
    let tracks = app.state.album_detail().tracks();

    // Read playback state from Store and convert to display type
    // (This is from a different store, so we compute it here)
    let playback_store = app.state.playback();
    let playback_display = use_memo(move || {
        let track_id = playback_store
            .current_track_id()
            .read()
            .clone()
            .unwrap_or_default();
        let pos = *playback_store.position_ms().read();
        let dur = *playback_store.duration_ms().read();
        match *playback_store.status().read() {
            PlaybackStatus::Stopped => PlaybackDisplay::Stopped,
            PlaybackStatus::Loading => PlaybackDisplay::Loading { track_id },
            PlaybackStatus::Playing => PlaybackDisplay::Playing {
                track_id,
                position_ms: pos,
                duration_ms: dur,
            },
            PlaybackStatus::Paused => PlaybackDisplay::Paused {
                track_id,
                position_ms: pos,
                duration_ms: dur,
            },
        }
    });

    // Playback callbacks
    let on_track_play = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.play(track_id);
        }
    });
    let on_track_pause = EventHandler::new({
        let playback = playback.clone();
        move |_: ()| {
            playback.pause();
        }
    });
    let on_track_resume = EventHandler::new({
        let playback = playback.clone();
        move |_: ()| {
            playback.resume();
        }
    });
    let on_track_add_next = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.add_next(vec![track_id]);
        }
    });
    let on_track_add_to_queue = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.add_to_queue(vec![track_id]);
        }
    });
    let on_track_export = EventHandler::new({
        let library_manager = library_manager.clone();
        let cache = cache.clone();
        let library_dir = app.config.library_dir.clone();
        move |track_id: String| {
            let library_manager = library_manager.clone();
            let cache = cache.clone();
            let library_dir = library_dir.clone();
            spawn(async move {
                if let Some(file_handle) = AsyncFileDialog::new()
                    .set_title("Export Track")
                    .set_file_name(format!("{}.flac", track_id))
                    .add_filter("FLAC", &["flac"])
                    .save_file()
                    .await
                {
                    let output_path = file_handle.path().to_path_buf();
                    if let Err(e) = library_manager
                        .get()
                        .export_track(&track_id, &output_path, &cache, &library_dir)
                        .await
                    {
                        error!("Failed to export track: {}", e);
                    }
                }
            });
        }
    });

    // Toast feedback signals
    let mut success_toast = use_signal(|| None::<String>);
    let mut error_toast = use_signal(|| None::<String>);

    // Copy share link callback (creates encrypted share blob in cloud home)
    let on_copy_share_link = EventHandler::new({
        let app = app.clone();
        move |release_id: String| {
            app.create_share_link(&release_id);
        }
    });

    // Watch share link result from store and show toast
    let share_link_copied = *state.share_link_copied().read();
    let share_link_error = state.share_error().read().clone();
    use_effect(move || {
        if share_link_copied {
            success_toast.set(Some("Link copied".to_string()));
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                success_toast.set(None);
            });
        }
    });
    use_effect(move || {
        if let Some(ref e) = share_link_error {
            error_toast.set(Some(e.clone()));
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                error_toast.set(None);
            });
        }
    });

    // Album playback callbacks
    let on_play_album = EventHandler::new({
        let playback = playback.clone();
        move |track_ids: Vec<String>| {
            playback.play_album(track_ids);
        }
    });
    let on_add_album_to_queue = EventHandler::new({
        let playback = playback.clone();
        move |track_ids: Vec<String>| {
            playback.add_to_queue(track_ids);
        }
    });

    // Export release callback
    let on_export_release = EventHandler::new({
        let library_manager = library_manager.clone();
        let cache = cache.clone();
        let library_dir = app.config.library_dir.clone();
        move |release_id: String| {
            let library_manager = library_manager.clone();
            let cache = cache.clone();
            let library_dir = library_dir.clone();
            spawn(async move {
                if let Some(folder_handle) = AsyncFileDialog::new()
                    .set_title("Select Export Directory")
                    .pick_folder()
                    .await
                {
                    let target_dir = folder_handle.path().to_path_buf();
                    if let Err(e) = library_manager
                        .get()
                        .export_release(&release_id, &target_dir, &cache, &library_dir)
                        .await
                    {
                        error!("Failed to export release: {}", e);
                    }
                }
            });
        }
    });

    // Delete release callback
    let on_delete_release = EventHandler::new({
        let library_manager = library_manager.clone();
        let library_dir = app.config.library_dir.clone();
        let playback = playback.clone();
        move |release_id: String| {
            // Stop playback if current track belongs to the release being deleted
            let status = *playback_store.status().read();
            if matches!(status, PlaybackStatus::Playing | PlaybackStatus::Paused) {
                if let Some(current_release) = playback_store.current_release_id().read().clone() {
                    if current_release == release_id {
                        playback.stop();
                    }
                }
            }

            let library_manager = library_manager.clone();
            let library_dir = library_dir.clone();
            spawn(async move {
                if let Err(e) = library_manager
                    .get()
                    .delete_release(&release_id, &library_dir)
                    .await
                {
                    error!("Failed to delete release: {}", e);
                }
            });
        }
    });

    let on_album_deleted = EventHandler::new(move |_| {
        navigator().push(Route::Library {});
    });

    let on_artist_click = EventHandler::new(move |artist_id: String| {
        navigator().push(Route::ArtistDetail { artist_id });
    });

    // Delete album callback
    let on_delete_album = EventHandler::new({
        let library_manager = library_manager.clone();
        let library_dir = app.config.library_dir.clone();
        let playback = playback.clone();
        move |album_id: String| {
            // Stop playback if current track belongs to the album being deleted
            let status = *playback_store.status().read();
            if matches!(status, PlaybackStatus::Playing | PlaybackStatus::Paused) {
                if let Some(current_release) = playback_store.current_release_id().read().clone() {
                    let releases_list = state.releases().read().clone();
                    if releases_list.iter().any(|r| r.id == current_release) {
                        playback.stop();
                    }
                }
            }

            let library_manager = library_manager.clone();
            let library_dir = library_dir.clone();
            spawn(async move {
                if let Err(e) = library_manager
                    .get()
                    .delete_album(&album_id, &library_dir)
                    .await
                {
                    error!("Failed to delete album: {}", e);
                }
            });
        }
    });

    // Transfer callbacks
    let on_transfer_to_managed = EventHandler::new({
        let app = app.clone();
        move |release_id: String| {
            app.transfer_release_to_managed(&release_id);
        }
    });
    let on_eject = EventHandler::new({
        let app = app.clone();
        move |release_id: String| {
            app.eject_release_storage(&release_id);
        }
    });

    // Cover picker callbacks
    let on_fetch_remote_covers = EventHandler::new({
        let app = app.clone();
        move |_: ()| {
            app.fetch_remote_covers();
        }
    });
    let on_select_cover = EventHandler::new({
        let app = app.clone();
        move |selection: CoverChange| {
            let album_id = album_id();
            let release_id = state
                .selected_release_id()
                .read()
                .clone()
                .unwrap_or_default();
            app.change_cover(&album_id, &release_id, selection);
        }
    });

    // Share grant callback
    let on_create_share_grant = EventHandler::new({
        let app = app.clone();
        move |(release_id, recipient_pubkey): (String, String)| {
            app.create_share_grant(&release_id, &recipient_pubkey);
        }
    });

    // Check if viewing a followed library (read-only mode)
    let active_source = app.state.library().active_source().read().clone();
    let is_followed = matches!(active_source, LibrarySource::Followed(_));

    // Release select callback - navigate to new URL which triggers data reload
    let on_release_select = {
        move |new_release_id: String| {
            navigator().push(Route::AlbumDetail {
                album_id: album_id(),
                release_id: new_release_id,
            });
        }
    };

    // Use lenses for routing decisions
    let loading = *state.loading().read();
    let error = state.error().read().clone();
    let has_album = state.album().read().is_some();

    rsx! {
        BackButton {}
        if loading {
            AlbumDetailLoading {}
        } else if let Some(err) = error {
            AlbumDetailError { message: err }
        } else if has_album {
            AlbumDetailView {
                state,
                tracks,
                playback: playback_display(),
                read_only: is_followed,
                on_release_select,
                on_album_deleted,
                on_export_release,
                on_delete_album,
                on_delete_release,
                on_track_play,
                on_track_pause,
                on_track_resume,
                on_track_add_next,
                on_track_add_to_queue,
                on_track_export,
                on_artist_click,
                on_play_album,
                on_add_album_to_queue,
                on_transfer_to_managed,
                on_eject,
                on_fetch_remote_covers,
                on_select_cover,
                on_create_share_grant,
                on_copy_share_link,
            }

            if let Some(ref msg) = success_toast() {
                SuccessToast {
                    message: msg.clone(),
                    on_dismiss: move |_| success_toast.set(None),
                }
            }

            if let Some(ref msg) = error_toast() {
                ErrorToast {
                    message: msg.clone(),
                    on_dismiss: move |_| error_toast.set(None),
                }
            }
        } else {
            AlbumDetailLoading {}
        }
    }
}
