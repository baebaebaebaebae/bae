//! Queue Sidebar component
//!
//! Wrapper that passes stores to QueueSidebarView.
//! The view reads fields via lenses for granular reactivity.

use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::stores::{AppStateStoreExt, SidebarStateStoreExt, UiStateStoreExt};
use bae_ui::QueueSidebarView;
use dioxus::prelude::*;

/// Queue Sidebar - passes stores to view
#[component]
pub fn QueueSidebar() -> Element {
    let app = use_app();
    let sidebar_store = app.state.ui().sidebar();
    let mut is_open = sidebar_store.is_open();
    let library_manager = app.library_manager.clone();
    let playback_handle = app.playback_handle.clone();
    let playback_store = app.state.playback();

    // Navigation callback
    let on_track_click = {
        let library_manager = library_manager.clone();
        move |track_id: String| {
            let library_manager = library_manager.clone();
            spawn(async move {
                if let Ok(album_id) = library_manager
                    .get()
                    .get_album_id_for_track(&track_id)
                    .await
                {
                    navigator().push(Route::AlbumDetail {
                        album_id,
                        release_id: String::new(),
                    });
                }
            });
        }
    };

    let playback_for_clear = playback_handle.clone();
    let playback_for_remove = playback_handle.clone();
    let playback_for_skip = playback_handle.clone();
    let playback_for_pause = playback_handle.clone();
    let playback_for_resume = playback_handle.clone();

    rsx! {
        QueueSidebarView {
            sidebar: sidebar_store,
            playback: playback_store,
            on_close: move |_| is_open.set(false),
            on_clear: move |_| playback_for_clear.clear_queue(),
            on_remove: move |idx: usize| playback_for_remove.remove_from_queue(idx),
            on_track_click,
            on_play_index: move |idx: usize| playback_for_skip.skip_to(idx),
            on_pause: move |_| playback_for_pause.pause(),
            on_resume: move |_| playback_for_resume.resume(),
        }
    }
}
