//! Artist detail page

use crate::demo_data;
use crate::Route;
use bae_ui::stores::ArtistDetailState;
use bae_ui::ArtistDetailView;
use dioxus::prelude::*;

#[component]
pub fn ArtistDetail(artist_id: ReadSignal<String>) -> Element {
    let artist_id_val = artist_id();

    // Find the artist from demo data
    let artists_by_album = demo_data::get_artists_by_album();
    let albums = demo_data::get_albums();

    // Find the artist
    let artist = artists_by_album
        .values()
        .flatten()
        .find(|a| a.id == artist_id_val)
        .cloned();

    // Find albums for this artist
    let artist_albums: Vec<_> = albums
        .into_iter()
        .filter(|album| {
            artists_by_album
                .get(&album.id)
                .map(|artists| artists.iter().any(|a| a.id == artist_id_val))
                .unwrap_or(false)
        })
        .collect();

    let state = use_store(|| ArtistDetailState {
        artist,
        albums: artist_albums,
        artists_by_album: artists_by_album.clone(),
        loading: false,
        error: None,
    });

    rsx! {
        ArtistDetailView {
            state,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_artist_click: move |artist_id: String| {
                navigator().push(Route::ArtistDetail { artist_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
            on_back: move |_| {
                navigator().go_back();
            },
        }
    }
}
