use crate::api;
use crate::playback::{TrackInfo, WebPlaybackService};
use crate::Route;
use bae_ui::display_types::PlaybackDisplay;
use bae_ui::stores::playback::{PlaybackStatus, PlaybackUiStateStoreExt};
use bae_ui::stores::{AlbumDetailState, AlbumDetailStateStoreExt};
use bae_ui::{AlbumDetailView, BackButton};
use dioxus::prelude::*;

fn build_track_info(state: &AlbumDetailState, track_id: &str) -> Option<TrackInfo> {
    let track = state.tracks.iter().find(|t| t.id == track_id)?;
    let album = state.album.as_ref()?;
    let artist = state.artists.first();

    Some(TrackInfo {
        track_id: track_id.to_string(),
        track: track.clone(),
        album_title: album.title.clone(),
        cover_url: album.cover_url.clone(),
        artist_name: artist
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string()),
        artist_id: artist.map(|a| a.id.clone()),
    })
}

fn build_track_infos(state: &AlbumDetailState, track_ids: &[String]) -> Vec<TrackInfo> {
    track_ids
        .iter()
        .filter_map(|id| build_track_info(state, id))
        .collect()
}

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
            let mut service: Signal<WebPlaybackService> = use_context();

            // Compute PlaybackDisplay from playback store (provided via context in layout)
            let playback_store: Store<bae_ui::stores::playback::PlaybackUiState> = use_context();
            let playback_display = use_memo(move || {
                let track_id = playback_store
                    .current_track_id()
                    .read()
                    .clone()
                    .unwrap_or_default();
                let pos = *playback_store.position_ms().read();
                let dur = *playback_store.duration_ms().read();
                match *playback_store.status().read() {
                    PlaybackStatus::Stopped => PlaybackDisplay::Stopped,
                    PlaybackStatus::Loading => PlaybackDisplay::Loading { track_id },
                    PlaybackStatus::Playing => PlaybackDisplay::Playing {
                        track_id,
                        position_ms: pos,
                        duration_ms: dur,
                    },
                    PlaybackStatus::Paused => PlaybackDisplay::Paused {
                        track_id,
                        position_ms: pos,
                        duration_ms: dur,
                    },
                }
            });

            rsx! {
                BackButton {
                    on_click: move |_| {
                        navigator().push(Route::Library {});
                    },
                }

                AlbumDetailView {
                    state,
                    tracks,
                    playback: playback_display(),
                    on_release_select: |_| {},
                    on_album_deleted: |_| {},
                    on_export_release: |_| {},
                    on_delete_album: |_| {},
                    on_delete_release: |_| {},
                    on_track_play: move |track_id: String| {
                        let album_state = state.read().clone();
                        // Build infos for current track + everything after it
                        let track_ids = &album_state.track_ids;
                        if let Some(pos) = track_ids.iter().position(|id| *id == track_id) {
                            let remaining: Vec<String> = track_ids[pos..].to_vec();
                            let infos = build_track_infos(&album_state, &remaining);
                            service.write().play_album(infos);
                        }
                    },
                    on_track_pause: move |_| service.write().pause(),
                    on_track_resume: move |_| service.write().resume(),
                    on_track_add_next: move |track_id: String| {
                        let album_state = state.read().clone();
                        if let Some(info) = build_track_info(&album_state, &track_id) {
                            service.write().add_next_with_info(vec![info]);
                        }
                    },
                    on_track_add_to_queue: move |track_id: String| {
                        let album_state = state.read().clone();
                        if let Some(info) = build_track_info(&album_state, &track_id) {
                            service.write().add_to_queue_with_info(vec![info]);
                        }
                    },
                    on_track_export: |_| {},
                    on_artist_click: |_| {},
                    on_play_album: move |track_ids: Vec<String>| {
                        let album_state = state.read().clone();
                        let infos = build_track_infos(&album_state, &track_ids);
                        service.write().play_album(infos);
                    },
                    on_add_album_to_queue: move |track_ids: Vec<String>| {
                        let album_state = state.read().clone();
                        let infos = build_track_infos(&album_state, &track_ids);
                        service.write().add_to_queue_with_info(infos);
                    },
                    on_transfer_to_managed: |_| {},
                    on_eject: |_| {},
                    on_fetch_remote_covers: |_| {},
                    on_select_cover: |_| {},
                    on_copy_share_link: |_| {},
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
