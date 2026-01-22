//! Queue Sidebar component
//!
//! Wrapper component that connects bae-ui's QueueSidebarView to app state.
//! All playback state is read from the Store (populated by AppService).

use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::stores::{
    AppStateStoreExt, PlaybackUiStateStoreExt, SidebarStateStoreExt, UiStateStoreExt,
};
use bae_ui::QueueSidebarView;
use dioxus::prelude::*;

/// Queue Sidebar wrapper that handles state subscription
#[component]
pub fn QueueSidebar() -> Element {
    let app = use_app();
    let sidebar_store = app.state.ui().sidebar();
    let mut is_open = sidebar_store.is_open();
    let library_manager = app.library_manager.clone();
    let playback = app.playback_handle.clone();

    // Read from Store (updated by AppService)
    let playback_store = app.state.playback();
    let current_track_id = use_memo(move || playback_store.current_track_id().read().clone());
    let current_track = use_memo(move || playback_store.current_track().read().clone());
    let queue_items = use_memo(move || playback_store.queue_items().read().clone());

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

    rsx! {
        QueueSidebarView {
            is_open: is_open(),
            current_track: current_track(),
            queue: queue_items(),
            current_track_id: current_track_id(),
            on_close: move |_| is_open.set(false),
            on_clear: {
                let playback = playback.clone();
                move |_| playback.clear_queue()
            },
            on_remove: {
                let playback = playback.clone();
                move |idx: usize| playback.remove_from_queue(idx)
            },
            on_track_click,
        }
    }
}
