//! Library page component
//!
//! Uses bae-ui's LibraryView with app-specific navigation callbacks.
//! Reads library state from the global app store (populated by AppService).

use crate::ui::app_service::use_app;
use crate::ui::components::album_detail::utils::get_album_track_ids;
use crate::ui::Route;
use bae_ui::stores::{AppStateStoreExt, LibraryStateStoreExt};
use bae_ui::LibraryView;
use dioxus::prelude::*;

/// Library page component - reads from global store and passes to bae-ui's LibraryView
#[component]
pub fn LibraryPage() -> Element {
    let app = use_app();
    let library_manager = app.library_manager.clone();
    let playback = app.playback_handle.clone();

    // Read signals from the Store
    let library_store = app.state.library();
    let albums = use_memo(move || library_store.albums().read().clone());
    let artists_by_album = use_memo(move || library_store.artists_by_album().read().clone());
    let loading = use_memo(move || *library_store.loading().read());
    let error = use_memo(move || library_store.error().read().clone());

    // Navigation callback - navigate to album detail
    let on_album_click = move |album_id: String| {
        navigator().push(Route::AlbumDetail {
            album_id,
            release_id: String::new(),
        });
    };

    // Play album callback
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

    // Add to queue callback
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

    // Empty state action - navigate to import workflow
    let on_empty_action = move |_| {
        navigator().push(Route::ImportWorkflowManager {});
    };

    rsx! {
        LibraryView {
            albums,
            artists_by_album,
            loading,
            error,
            on_album_click,
            on_play_album,
            on_add_album_to_queue,
            on_empty_action,
        }
    }
}

// Keep the old name as an alias for backwards compatibility with routes
pub use LibraryPage as Library;
