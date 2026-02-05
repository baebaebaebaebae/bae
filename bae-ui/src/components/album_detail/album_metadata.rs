//! Album metadata display component

use crate::display_types::{Album, Artist, Release};
use dioxus::prelude::*;

#[component]
pub fn AlbumMetadata(
    album: Album,
    artists: Vec<Artist>,
    track_count: usize,
    selected_release: Option<Release>,
    on_artist_click: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            h1 { class: "text-2xl font-bold text-white mb-2", "{album.title}" }
            p { class: "text-lg text-gray-300 mb-2",
                if artists.is_empty() {
                    "Unknown Artist"
                } else {
                    for (i , artist) in artists.iter().enumerate() {
                        if i > 0 {
                            ", "
                        }
                        span {
                            class: "hover:text-white hover:underline transition-colors cursor-pointer",
                            onclick: {
                                let artist_id = artist.id.clone();
                                move |_| on_artist_click.call(artist_id.clone())
                            },
                            "{artist.name}"
                        }
                    }
                }
                if let Some(year) = album.year {
                    " · {year}"
                }
            }
        }
    }
}
