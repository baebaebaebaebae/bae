//! Artist detail view component - shows artist info and their albums

use crate::components::album_card::AlbumCard;
use crate::components::helpers::{ErrorDisplay, LoadingSpinner};
use crate::display_types::{Album, Artist};
use crate::stores::artist_detail::{ArtistDetailState, ArtistDetailStateStoreExt};
use dioxus::prelude::*;
use dioxus_virtual_scroll::{KeyFn, RenderFn, ScrollTarget, VirtualGrid, VirtualGridConfig};
use std::collections::HashMap;
use std::rc::Rc;

/// Item type for the virtual album grid
#[derive(Clone, PartialEq)]
struct AlbumGridItem {
    album: Album,
    artists: Vec<Artist>,
}

/// Artist detail view component
///
/// Accepts `ReadStore<ArtistDetailState>` and uses lenses for granular reactivity.
#[component]
pub fn ArtistDetailView(
    state: ReadStore<ArtistDetailState>,
    on_album_click: EventHandler<String>,
    on_artist_click: EventHandler<String>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    on_back: EventHandler<()>,
) -> Element {
    let loading = *state.loading().read();
    let error = state.error().read().clone();
    let artist = state.artist().read().clone();
    let albums = state.albums().read().clone();
    let artists_by_album = state.artists_by_album().read().clone();

    let mut scroll_target: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex-grow overflow-y-auto flex flex-col py-10",
            onmounted: move |evt| scroll_target.set(Some(evt.data())),
            div { class: "container mx-auto flex flex-col flex-1",
                if loading {
                    LoadingSpinner { message: "Loading artist...".to_string() }
                } else if let Some(err) = error {
                    ErrorDisplay { message: err }
                } else if let Some(artist) = artist {
                    div { class: "flex items-center gap-6 mb-2",
                        if let Some(ref image_url) = artist.image_url {
                            img {
                                class: "w-32 h-32 rounded-full object-cover",
                                src: "{image_url}",
                            }
                        }
                        h1 { class: "text-3xl font-bold text-white", "{artist.name}" }
                    }

                    if !albums.is_empty() {
                        {
                            let album_label = if albums.len() == 1 {
                                "1 album".to_string()
                            } else {
                                format!("{} albums", albums.len())
                            };
                            rsx! {
                                p { class: "text-sm text-gray-400 mb-6", "{album_label}" }
                            }
                        }

                        ArtistAlbumGrid {
                            albums,
                            artists_by_album,
                            on_album_click,
                            on_artist_click,
                            on_play_album,
                            on_add_album_to_queue,
                            scroll_target: ScrollTarget::Element(scroll_target.into()),
                        }
                    }
                }
            }
        }
    }
}

/// Grid component to display artist's albums with virtual scrolling
#[component]
fn ArtistAlbumGrid(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    on_album_click: EventHandler<String>,
    on_artist_click: EventHandler<String>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    scroll_target: ScrollTarget,
) -> Element {
    let items: Vec<AlbumGridItem> = albums
        .into_iter()
        .map(|album| {
            let artists = artists_by_album.get(&album.id).cloned().unwrap_or_default();
            AlbumGridItem { album, artists }
        })
        .collect();

    let config = VirtualGridConfig {
        item_width: 200.0,
        item_height: 280.0,
        buffer_rows: 2,
        gap: 24.0,
    };

    let open_dropdown: Signal<Option<String>> = use_signal(|| None);

    let render_item = RenderFn(Rc::new(move |item: AlbumGridItem, _idx: usize| {
        rsx! {
            AlbumCard {
                key: "{item.album.id}",
                album: item.album,
                artists: item.artists,
                on_click: on_album_click,
                on_artist_click,
                on_play: on_play_album,
                on_add_to_queue: on_add_album_to_queue,
                open_dropdown,
            }
        }
    }));

    let key_fn = KeyFn(Rc::new(|item: &AlbumGridItem| item.album.id.clone()));

    rsx! {
        VirtualGrid {
            items,
            config,
            render_item,
            key_fn,
            scroll_target,
        }
    }
}
