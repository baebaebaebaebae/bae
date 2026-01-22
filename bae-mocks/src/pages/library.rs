//! Library page

use crate::demo_data;
use crate::Route;
use bae_ui::LibraryView;
use dioxus::prelude::*;

#[component]
pub fn Library() -> Element {
    let albums = use_memo(demo_data::get_albums);
    let artists_by_album = use_memo(demo_data::get_artists_by_album);
    let loading = use_memo(|| false);
    let error = use_memo(|| None);

    rsx! {
        LibraryView {
            albums,
            artists_by_album,
            loading,
            error,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
            on_empty_action: |_| {},
        }
    }
}
