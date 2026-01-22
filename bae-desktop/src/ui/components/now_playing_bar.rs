//! Now Playing Bar component
//!
//! Wrapper component that connects bae-ui's NowPlayingBarView to global Store.

use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::display_types::PlaybackDisplay;
use bae_ui::stores::{
    AppStateStoreExt, PlaybackStatus, PlaybackUiStateStoreExt, SidebarStateStoreExt,
    UiStateStoreExt,
};
use bae_ui::NowPlayingBarView;
use dioxus::prelude::*;

/// Now Playing Bar - reads all state from global Store
#[component]
pub fn NowPlayingBar() -> Element {
    let app = use_app();
    let playback = app.playback_handle.clone();
    let library_manager = app.library_manager.clone();

    // Read playback state from Store
    let playback_store = app.state.playback();
    let status = use_memo(move || *playback_store.status().read());
    let current_track = use_memo(move || playback_store.current_track().read().clone());
    let current_release_id = use_memo(move || playback_store.current_release_id().read().clone());
    let position_ms = use_memo(move || *playback_store.position_ms().read());
    let duration_ms = use_memo(move || *playback_store.duration_ms().read());
    let pregap_ms = use_memo(move || *playback_store.pregap_ms().read());
    let artist_name = use_memo(move || playback_store.artist_name().read().clone());
    let cover_url = use_memo(move || playback_store.cover_url().read().clone());
    let mut playback_error_store = playback_store.playback_error();

    // Convert status to PlaybackDisplay
    let playback_display = use_memo(move || {
        let track_id = playback_store
            .current_track_id()
            .read()
            .clone()
            .unwrap_or_default();
        let pos = *playback_store.position_ms().read();
        let dur = *playback_store.duration_ms().read();
        match status() {
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

    // Get track from current_track QueueItem
    let track = use_memo(move || current_track().map(|qi| qi.track));

    // Read error from Store
    let playback_error = use_memo(move || playback_error_store.read().clone());

    // Callbacks
    let playback_for_prev = playback.clone();
    let playback_for_pause = playback.clone();
    let playback_for_resume = playback.clone();
    let playback_for_next = playback.clone();
    let playback_for_seek = playback.clone();

    // Use Store-based sidebar state
    let sidebar_store = app.state.ui().sidebar();
    let mut sidebar_is_open = sidebar_store.is_open();

    let on_track_click = {
        move |_track_id: String| {
            if let Some(release_id) = current_release_id() {
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

    rsx! {
        NowPlayingBarView {
            track: track(),
            artist_name: artist_name(),
            cover_url: cover_url(),
            playback: playback_display(),
            position_ms: position_ms(),
            duration_ms: duration_ms(),
            pregap_ms: pregap_ms(),
            playback_error: playback_error(),
            on_dismiss_error: move |_| playback_error_store.set(None),
            on_previous: move |_| playback_for_prev.previous(),
            on_pause: move |_| playback_for_pause.pause(),
            on_resume: move |_| playback_for_resume.resume(),
            on_next: move |_| playback_for_next.next(),
            on_seek: move |ms: u64| playback_for_seek.seek(std::time::Duration::from_millis(ms)),
            on_toggle_queue: move |_| {
                let current = *sidebar_is_open.read();
                sidebar_is_open.set(!current);
            },
            on_track_click,
        }
    }
}
