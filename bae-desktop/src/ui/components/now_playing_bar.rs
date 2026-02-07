//! Now Playing Bar component
//!
//! Wrapper that passes playback store to NowPlayingBarView.
//! The view reads fields via lenses for granular reactivity.

use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::stores::{
    AppStateStoreExt, PlaybackUiStateStoreExt, RepeatMode, SidebarStateStoreExt, UiStateStoreExt,
};
use bae_ui::NowPlayingBarView;
use dioxus::prelude::*;

/// Now Playing Bar - passes playback store to view
#[component]
pub fn NowPlayingBar() -> Element {
    let app = use_app();
    let playback_handle = app.playback_handle.clone();
    let library_manager = app.library_manager.clone();

    // Get stores - view will read via lenses
    let playback_store = app.state.playback();
    let mut playback_error_store = playback_store.playback_error();
    let mut sidebar_is_open = app.state.ui().sidebar().is_open();

    // For navigation callback, we still need to read current_release_id
    let current_release_id_store = playback_store.current_release_id();

    let on_track_click = {
        move |_track_id: String| {
            if let Some(release_id) = current_release_id_store.read().clone() {
                let library_manager = library_manager.clone();
                spawn(async move {
                    if let Ok(album_id) = library_manager
                        .get()
                        .get_album_id_for_release(&release_id)
                        .await
                    {
                        navigator().push(Route::AlbumDetail {
                            album_id,
                            release_id,
                        });
                    }
                });
            }
        }
    };

    // Clone handles for callbacks
    let playback_for_prev = playback_handle.clone();
    let playback_for_pause = playback_handle.clone();
    let playback_for_resume = playback_handle.clone();
    let playback_for_next = playback_handle.clone();
    let playback_for_seek = playback_handle.clone();
    let playback_for_repeat = playback_handle.clone();
    let repeat_mode_store = playback_store.repeat_mode();

    rsx! {
        NowPlayingBarView {
            state: playback_store,
            on_previous: move |_| playback_for_prev.previous(),
            on_pause: move |_| playback_for_pause.pause(),
            on_resume: move |_| playback_for_resume.resume(),
            on_next: move |_| playback_for_next.next(),
            on_seek: move |ms: u64| playback_for_seek.seek(std::time::Duration::from_millis(ms)),
            on_cycle_repeat: move |_| {
                let next = match *repeat_mode_store.read() {
                    RepeatMode::None => bae_core::playback::RepeatMode::Album,
                    RepeatMode::Album => bae_core::playback::RepeatMode::Track,
                    RepeatMode::Track => bae_core::playback::RepeatMode::None,
                };
                playback_for_repeat.set_repeat_mode(next);
            },
            on_toggle_queue: move |_| {
                let current = *sidebar_is_open.read();
                sidebar_is_open.set(!current);
            },
            on_track_click,
            on_artist_click: move |artist_id: String| {
                navigator().push(Route::ArtistDetail { artist_id });
            },
            on_dismiss_error: Some(EventHandler::new(move |_| playback_error_store.set(None))),
        }
    }
}
