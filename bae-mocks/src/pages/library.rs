//! Library page

use crate::demo_data;
use crate::Route;
use bae_ui::stores::LibraryState;
use bae_ui::LibraryView;
use dioxus::prelude::*;

#[component]
pub fn Library() -> Element {
    let state = use_store(|| LibraryState {
        albums: demo_data::get_albums(),
        artists_by_album: demo_data::get_artists_by_album(),
        loading: false,
        error: None,
    });

    rsx! {
        LibraryView {
            state,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
            on_empty_action: |_| {},
        }
    }
}
