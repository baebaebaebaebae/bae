//! Dropdown test page - A simple grid of album cards for e2e testing dropdown behavior

use bae_ui::components::AlbumCard;
use bae_ui::display_types::{Album, Artist};
use dioxus::prelude::*;

fn generate_test_albums() -> Vec<(Album, Vec<Artist>)> {
    (1..=9)
        .map(|i| {
            let album = Album {
                id: format!("album-{}", i),
                title: format!("Test Album {}", i),
                year: Some(2020 + (i % 5)),
                cover_url: None,
                is_compilation: false,
            };
            let artist = Artist {
                id: format!("artist-{}", i),
                name: format!("Artist {}", i),
            };
            (album, vec![artist])
        })
        .collect()
}

#[component]
pub fn MockDropdownTest() -> Element {
    let albums = generate_test_albums();

    rsx! {
        style {
            r#"
            body {{ margin: 0; background: #1a1a2e; font-family: system-ui; }}
            * {{ box-sizing: border-box; }}
            "#
        }
        div { style: "padding: 20px;",
            h1 { style: "color: white; margin: 0 0 20px 0;", "Dropdown Test Grid" }
            p { style: "color: #888; margin: 0 0 20px 0;",
                "Click the ellipsis button on any card to open its dropdown menu."
            }
            div {
                class: "album-grid",
                style: "display: grid; grid-template-columns: repeat(3, 200px); gap: 20px;",
                for (album , artists) in albums {
                    AlbumCard {
                        key: "{album.id}",
                        album: album.clone(),
                        artists,
                        on_click: |_| {},
                        on_play: |_| {},
                        on_add_to_queue: |_| {},
                    }
                }
            }
        }
    }
}
