//! Album card component - pure view with callbacks

use crate::components::helpers::Tooltip;
use crate::components::icons::{EllipsisIcon, ImageIcon, PlayIcon, PlusIcon};
use crate::components::{MenuDropdown, MenuItem, Placement, TextLink};
use crate::display_types::{Album, Artist};
use dioxus::prelude::*;

/// Individual album card component
///
/// Pure view component - displays album info with hover dropdown for actions.
/// Navigation is handled via on_click callback, not direct router calls.
#[component]
pub fn AlbumCard(
    album: Album,
    artists: Vec<Artist>,
    // Navigation callback - called with album_id when card is clicked
    on_click: EventHandler<String>,
    // Navigation callback - called with artist_id when artist name is clicked
    on_artist_click: EventHandler<String>,
    // Action callbacks
    on_play: EventHandler<String>,
    on_add_to_queue: EventHandler<String>,
    // Which album's dropdown is open (hoisted to parent to outlive virtual scroll recycling)
    mut open_dropdown: Signal<Option<String>>,
) -> Element {
    let album_id = album.id.clone();
    let album_title = album.title.clone();
    let album_year = album.year;
    let cover_url = album.cover_url.clone();

    let is_open = {
        let album_id = album_id.clone();
        use_memo(move || open_dropdown() == Some(album_id.clone()))
    };
    // Use album_id for anchor to ensure uniqueness even if component is recycled
    let anchor_id = format!("album-card-btn-{}", album_id);

    // Note: use overflow-clip (not overflow-hidden) to clip rounded corners without blocking scroll propagation
    let card_class = "bg-gray-800 rounded-lg overflow-clip shadow-lg hover:shadow-xl transition-shadow duration-300 cursor-pointer group relative";

    rsx! {
        div {
            class: "{card_class}",
            "data-testid": "album-card",
            onclick: {
                let album_id = album_id.clone();
                move |_| {
                    if !is_open() {
                        on_click.call(album_id.clone());
                    }
                }
            },
            div { class: "aspect-square bg-gray-700 flex items-center justify-center relative",
                if let Some(url) = &cover_url {
                    img {
                        src: "{url}",
                        alt: "Album cover for {album_title}",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    ImageIcon { class: "w-12 h-12 text-gray-500" }
                }

                // Hover overlay with dropdown trigger - stays visible when dropdown is open
                div {
                    class: "absolute inset-0 transition-colors flex items-start justify-end p-2",
                    class: if is_open() { "bg-black/40" } else { "bg-black/0 group-hover:bg-black/40" },
                    button {
                        id: "{anchor_id}",
                        class: "transition-opacity bg-gray-900/80 hover:bg-gray-800 rounded-full w-8 h-8 flex items-center justify-center text-white",
                        class: if is_open() { "opacity-100" } else { "opacity-0 group-hover:opacity-100" },
                        onclick: {
                            let album_id = album_id.clone();
                            move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                if is_open() {
                                    open_dropdown.set(None);
                                } else {
                                    open_dropdown.set(Some(album_id.clone()));
                                }
                            }
                        },
                        EllipsisIcon { class: "w-5 h-5" }
                    }
                }
            }
            div { class: "p-4",
                Tooltip {
                    text: album_title.clone(),
                    placement: Placement::Bottom,
                    nowrap: true,
                    h3 { class: "font-bold text-white text-lg mb-1 truncate", "{album_title}" }
                }
                p { class: "text-gray-400 text-sm truncate",
                    if artists.is_empty() {
                        "Unknown Artist"
                    } else {
                        for (i , artist) in artists.iter().enumerate() {
                            if i > 0 {
                                ", "
                            }
                            TextLink {
                                onclick: {
                                    let artist_id = artist.id.clone();
                                    move |evt: Event<MouseData>| {
                                        evt.stop_propagation();
                                        on_artist_click.call(artist_id.clone());
                                    }
                                },
                                "{artist.name}"
                            }
                        }
                    }
                }
                if let Some(year) = album_year {
                    p { class: "text-gray-500 text-xs mt-1", "{year}" }
                }
            }

            // Dropdown menu
            MenuDropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| open_dropdown.set(None),
                placement: Placement::BottomEnd,

                MenuItem {
                    onclick: {
                        let album_id = album_id.clone();
                        move |_| {
                            open_dropdown.set(None);
                            on_play.call(album_id.clone());
                        }
                    },
                    PlayIcon { class: "w-4 h-4" }
                    "Play"
                }
                MenuItem {
                    onclick: {
                        let album_id = album_id.clone();
                        move |_| {
                            open_dropdown.set(None);
                            on_add_to_queue.call(album_id.clone());
                        }
                    },
                    PlusIcon { class: "w-4 h-4" }
                    "Add to Queue"
                }
            }
        }
    }
}
