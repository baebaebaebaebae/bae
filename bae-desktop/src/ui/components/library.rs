//! Library page component
//!
//! Uses bae-ui's LibraryView with app-specific navigation callbacks.
//! Reads library state from the global app store (populated by AppService).

use crate::ui::app_service::use_app;
use crate::ui::components::album_detail::utils::get_album_track_ids;
use crate::ui::Route;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::LibraryView;
use dioxus::prelude::*;

/// Library page component - passes state lens to bae-ui's LibraryView
#[component]
pub fn LibraryPage() -> Element {
    let app = use_app();
    let library_manager = app.library_manager.clone();
    let playback = app.playback_handle.clone();

    // Pass the state lens directly - don't read here!
    let state = app.state.library();

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
            state,
            on_album_click,
            on_play_album,
            on_add_album_to_queue,
            on_empty_action,
        }
    }
}

// Keep the old name as an alias for backwards compatibility with routes
pub use LibraryPage as Library;
