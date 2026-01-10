//! bae demo - Web demo for screenshot generation
//!
//! A minimal web app that renders UI components with fixture data.
//! Used for Playwright-based screenshot generation.

mod demo_data;

use bae_ui::{
    AlbumDetailView, BackButton, ErrorDisplay, LibraryView, PageContainer, PlaybackDisplay, Track,
};
use dioxus::prelude::*;

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(DemoLayout)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id")]
    AlbumDetail { album_id: String },
}

/// Layout component for demo app
#[component]
fn DemoLayout() -> Element {
    rsx! {
        Outlet::<Route> {}
    }
}

/// Demo library page - uses static fixture data
#[component]
fn Library() -> Element {
    let albums = demo_data::get_albums();
    let artists_by_album = demo_data::get_artists_by_album();

    rsx! {
        LibraryView {
            albums,
            artists_by_album,
            loading: false,
            error: None,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
        }
    }
}

/// Demo album detail page - uses bae-ui's AlbumDetailView with fixture data
#[component]
fn AlbumDetail(album_id: String) -> Element {
    let album = demo_data::get_album(&album_id);
    let artists = demo_data::get_artists_for_album(&album_id);
    let releases = demo_data::get_releases_for_album(&album_id);
    let tracks = demo_data::get_tracks_for_album(&album_id);

    // Create per-track signals for reactivity
    let track_signals: Vec<Signal<Track>> = tracks.into_iter().map(Signal::new).collect();

    // Signals for import state (not used in demo, but required by component)
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    let selected_release_id = releases.first().map(|r| r.id.clone());

    rsx! {
        PageContainer {
            BackButton {
                on_click: move |_| {
                    navigator().push(Route::Library {});
                },
            }

            if let Some(album) = album {
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    tracks: track_signals,
                    selected_release_id,
                    import_progress,
                    import_error,
                    playback: PlaybackDisplay::Stopped,
                    // Navigation callback
                    on_release_select: |_release_id: String| {},
                    // Album-level callbacks (no-ops for demo)
                    on_album_deleted: |_| {},
                    on_export_release: |_| {},
                    on_delete_album: |_| {},
                    on_delete_release: |_| {},
                    // Track playback callbacks (no-ops for demo)
                    on_track_play: |_| {},
                    on_track_pause: |_| {},
                    on_track_resume: |_| {},
                    on_track_add_next: |_| {},
                    on_track_add_to_queue: |_| {},
                    on_track_export: |_| {},
                    // Album playback callbacks (no-ops for demo)
                    on_play_album: |_| {},
                    on_add_album_to_queue: |_| {},
                }
            } else {
                ErrorDisplay { message: "Album not found in demo data".to_string() }
            }
        }
    }
}

/// Main demo app component
#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "min-h-screen bg-gray-900", Router::<Route> {} }
    }
}

fn main() {
    dioxus::launch(App);
}
