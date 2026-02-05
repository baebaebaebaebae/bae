//! Artist detail page component

use crate::ui::app_service::use_app;
use crate::ui::components::album_detail::utils::get_album_track_ids;
use crate::ui::Route;
use bae_ui::stores::{AppStateStoreExt, ArtistDetailStateStoreExt};
use bae_ui::ArtistDetailView;
use dioxus::prelude::*;

use super::album_detail::back_button::BackButton;

/// Artist detail page - loads artist data and wires navigation
#[component]
pub fn ArtistDetail(artist_id: ReadSignal<String>) -> Element {
    let app = use_app();

    // Load artist detail data on mount/param change
    use_effect({
        let app = app.clone();
        move || {
            let artist_id = artist_id();
            app.load_artist_detail(&artist_id);
        }
    });

    let library_manager = app.library_manager.clone();
    let playback = app.playback_handle.clone();
    let state = app.state.artist_detail();

    let loading = *state.loading().read();
    let error = state.error().read().clone();

    let on_album_click = move |album_id: String| {
        navigator().push(Route::AlbumDetail {
            album_id,
            release_id: String::new(),
        });
    };

    let on_artist_click = move |artist_id: String| {
        navigator().push(Route::ArtistDetail { artist_id });
    };

    let on_play_album = {
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |album_id: String| {
            let library_manager = library_manager.clone();
            let playback = playback.clone();
            spawn(async move {
                if let Ok(track_ids) = get_album_track_ids(&library_manager, &album_id).await {
                    playback.play_album(track_ids);
                }
            });
        }
    };

    let on_add_album_to_queue = {
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |album_id: String| {
            let library_manager = library_manager.clone();
            let playback = playback.clone();
            spawn(async move {
                if let Ok(track_ids) = get_album_track_ids(&library_manager, &album_id).await {
                    playback.add_to_queue(track_ids);
                }
            });
        }
    };

    let on_back = move |_| {
        navigator().go_back();
    };

    // Show loading/error states, or delegate to view
    if loading || error.is_some() || state.artist().read().is_some() {
        rsx! {
            BackButton {}
            ArtistDetailView {
                state,
                on_album_click,
                on_artist_click,
                on_play_album,
                on_add_album_to_queue,
                on_back,
            }
        }
    } else {
        rsx! {
            BackButton {}
        }
    }
}
