use crate::api;
use crate::Route;
use bae_ui::stores::{AlbumDetailState, AlbumDetailStateStoreExt};
use bae_ui::{AlbumDetailView, BackButton, PlaybackDisplay};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetail(album_id: String) -> Element {
    let id = album_id.clone();
    let data = use_resource(move || {
        let id = id.clone();
        async move { api::fetch_album(&id).await }
    });
    let read = data.read();

    let result: Result<AlbumDetailState, String> = match &*read {
        Some(Ok(album_state)) => Ok(album_state.clone()),
        Some(Err(e)) => Err(e.clone()),
        None => {
            return rsx! {
                div { class: "flex items-center justify-center h-full text-gray-400",
                    "Loading..."
                }
            };
        }
    };
    drop(read);

    match result {
        Ok(album_state) => {
            let state = use_store(move || album_state);
            let tracks = state.tracks();

            rsx! {
                BackButton {
                    on_click: move |_| {
                        navigator().push(Route::Library {});
                    },
                }

                AlbumDetailView {
                    state,
                    tracks,
                    playback: PlaybackDisplay::Stopped,
                    on_release_select: |_| {},
                    on_album_deleted: |_| {},
                    on_export_release: |_| {},
                    on_delete_album: |_| {},
                    on_delete_release: |_| {},
                    on_track_play: |_| {},
                    on_track_pause: |_| {},
                    on_track_resume: |_| {},
                    on_track_add_next: |_| {},
                    on_track_add_to_queue: |_| {},
                    on_track_export: |_| {},
                    on_artist_click: |_| {},
                    on_play_album: |_| {},
                    on_add_album_to_queue: |_| {},
                }
            }
        }
        Err(e) => {
            rsx! {
                BackButton {
                    on_click: move |_| {
                        navigator().push(Route::Library {});
                    },
                }
                div { class: "flex items-center justify-center h-full text-gray-400",
                    "Failed to load album: {e}"
                }
            }
        }
    }
}
