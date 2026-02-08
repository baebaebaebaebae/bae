use crate::api;
use crate::Route;
use bae_ui::stores::{LibrarySortState, LibrarySortStateStoreExt, LibraryState};
use bae_ui::LibraryView;
use dioxus::prelude::*;

#[component]
pub fn Library() -> Element {
    let data = use_resource(api::fetch_albums);
    let read = data.read();

    let result = match &*read {
        Some(Ok((albums, artists_by_album))) => Ok((albums.clone(), artists_by_album.clone())),
        Some(Err(e)) => Err(e.clone()),
        None => {
            return rsx! {
                div { class: "flex items-center justify-center h-full text-gray-400",
                    "Loading..."
                }
            }
        }
    };
    drop(read);

    match result {
        Ok((albums, artists_by_album)) => {
            let state = use_store(move || LibraryState {
                albums,
                artists_by_album,
                loading: false,
                error: None,
            });

            let sort_state = use_store(LibrarySortState::default);

            rsx! {
                LibraryView {
                    state,
                    sort_state,
                    on_sort_criteria_change: move |criteria| {
                        sort_state.sort_criteria().set(criteria);
                    },
                    on_view_mode_change: move |mode| {
                        sort_state.view_mode().set(mode);
                    },
                    on_album_click: move |album_id: String| {
                        navigator().push(Route::AlbumDetail { album_id });
                    },
                    on_artist_click: |_| {},
                    on_play_album: |_| {},
                    on_add_album_to_queue: |_| {},
                    on_empty_action: |_| {},
                }
            }
        }
        Err(e) => {
            rsx! {
                div { class: "flex items-center justify-center h-full text-gray-400",
                    "Failed to load library: {e}"
                }
            }
        }
    }
}
