//! Library page

use crate::demo_data;
use crate::Route;
use bae_ui::stores::{LibrarySortState, LibrarySortStateStoreExt, LibraryState};
use bae_ui::{Album, Artist, LibraryView};
use dioxus::prelude::*;
use std::collections::HashMap;

// TODO: Remove this - temporary for testing large libraries
const ALBUM_COUNT: usize = 2000;

#[component]
pub fn Library() -> Element {
    let (albums, artists_by_album) = generate_albums(ALBUM_COUNT);

    let state = use_store(|| LibraryState {
        albums,
        artists_by_album,
        loading: false,
        error: None,
        active_source: bae_ui::stores::config::LibrarySource::Local,
    });

    let sort_state = use_store(LibrarySortState::default);

    let on_sort_criteria_change = move |criteria| {
        sort_state.sort_criteria().set(criteria);
    };

    let on_view_mode_change = move |mode| {
        sort_state.view_mode().set(mode);
    };

    rsx! {
        LibraryView {
            state,
            sort_state,
            on_sort_criteria_change,
            on_view_mode_change,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_artist_click: move |artist_id: String| {
                navigator().push(Route::ArtistDetail { artist_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
            on_empty_action: |_| {},
        }
    }
}

fn generate_albums(count: usize) -> (Vec<Album>, HashMap<String, Vec<Artist>>) {
    let base_albums = demo_data::get_albums();
    let base_artists = demo_data::get_artists_by_album();

    let mut albums = Vec::with_capacity(count);
    let mut artists_by_album = HashMap::new();

    for i in 0..count {
        let idx = i % base_albums.len();
        let base = &base_albums[idx];

        // First batch uses real IDs (so album detail navigation works),
        // subsequent batches use generated IDs
        let id = if i < base_albums.len() {
            base.id.clone()
        } else {
            format!("album-{}", i + 1)
        };

        albums.push(Album {
            id: id.clone(),
            title: base.title.clone(),
            year: base.year,
            cover_url: base.cover_url.clone(),
            is_compilation: base.is_compilation,
            date_added: base.date_added,
        });

        if let Some(artists) = base_artists.get(&base.id) {
            artists_by_album.insert(id, artists.clone());
        }
    }

    (albums, artists_by_album)
}
