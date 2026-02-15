use crate::api;
use crate::playback::{TrackInfo, WebPlaybackService};
use crate::Route;
use bae_ui::stores::{AlbumDetailState, LibrarySortState, LibrarySortStateStoreExt, LibraryState};
use bae_ui::LibraryView;
use dioxus::prelude::*;

fn build_track_infos_from_detail(detail: &AlbumDetailState) -> Vec<TrackInfo> {
    let album = match detail.album.as_ref() {
        Some(a) => a,
        None => return vec![],
    };
    let artist = detail.artists.first();

    detail
        .tracks
        .iter()
        .map(|track| TrackInfo {
            track_id: track.id.clone(),
            track: track.clone(),
            album_title: album.title.clone(),
            cover_url: album.cover_url.clone(),
            artist_name: artist
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string()),
            artist_id: artist.map(|a| a.id.clone()),
        })
        .collect()
}

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
                active_source: Default::default(),
            });

            let sort_state = use_store(LibrarySortState::default);
            let mut service: Signal<WebPlaybackService> = use_context();

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
                    on_play_album: move |album_id: String| {
                        spawn(async move {
                            if let Ok(detail) = api::fetch_album(&album_id).await {
                                let infos = build_track_infos_from_detail(&detail);
                                service.write().play_album(infos);
                            }
                        });
                    },
                    on_add_album_to_queue: move |album_id: String| {
                        spawn(async move {
                            if let Ok(detail) = api::fetch_album(&album_id).await {
                                let infos = build_track_infos_from_detail(&detail);
                                service.write().add_to_queue_with_info(infos);
                            }
                        });
                    },
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
