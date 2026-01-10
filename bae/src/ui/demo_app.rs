//! Demo app for screenshot generation
//!
//! A minimal web app that renders UI components with fixture data.
//! Used for Playwright-based screenshot generation.

use crate::ui::components::album_detail::{AlbumDetailError, AlbumDetailView, PageContainer};
use crate::ui::components::dialog::GlobalDialog;
use crate::ui::components::dialog_context::DialogContext;
use crate::ui::components::library::LibraryView;
use crate::ui::demo_data;
use crate::ui::display_types::PlaybackDisplay;
use dioxus::prelude::*;

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum DemoRoute {
    #[layout(DemoNavbar)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id")]
    AlbumDetail { album_id: String },
}

/// Layout component for demo app
#[component]
fn DemoNavbar() -> Element {
    rsx! {
        Outlet::<DemoRoute> {}
        GlobalDialog {}
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
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
        }
    }
}

/// Demo album detail page - uses static fixture data
#[component]
fn AlbumDetail(album_id: String) -> Element {
    let album = demo_data::get_album(&album_id);
    let artists = demo_data::get_artists_for_album(&album_id);
    let releases = demo_data::get_releases_for_album(&album_id);
    let tracks = demo_data::get_tracks_for_album(&album_id);

    let track_signals: Vec<Signal<crate::ui::display_types::Track>> =
        tracks.into_iter().map(Signal::new).collect();

    let selected_release_id = releases.first().map(|r| r.id.clone());
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    // Release selection navigation not needed for screenshots
    let on_release_select = move |_new_release_id: String| {};

    let noop = |_: ()| {};
    let noop_string = |_: String| {};
    let noop_vec = |_: Vec<String>| {};

    rsx! {
        PageContainer {
            DemoBackButton {}
            if let Some(album) = album {
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    track_signals: track_signals.clone(),
                    selected_release_id,
                    import_progress,
                    import_error,
                    playback: PlaybackDisplay::Stopped,
                    on_release_select,
                    on_album_deleted: noop,
                    on_export_release: noop_string,
                    on_delete_album: noop_string,
                    on_delete_release: noop_string,
                    on_track_play: noop_string,
                    on_track_pause: noop,
                    on_track_resume: noop,
                    on_track_add_next: noop_string,
                    on_track_add_to_queue: noop_string,
                    on_track_export: noop_string,
                    on_play_album: noop_vec,
                    on_add_album_to_queue: noop_vec,
                }
            } else {
                AlbumDetailError { message: "Album not found in demo data".to_string() }
            }
        }
    }
}

/// Back button that navigates using DemoRoute
#[component]
fn DemoBackButton() -> Element {
    rsx! {
        Link {
            to: DemoRoute::Library {},
            class: "inline-flex items-center text-gray-400 hover:text-white mb-4 transition-colors",
            svg {
                class: "w-5 h-5 mr-2",
                fill: "none",
                stroke: "currentColor",
                view_box: "0 0 24 24",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    stroke_width: "2",
                    d: "M15 19l-7-7 7-7",
                }
            }
            "Back to Library"
        }
    }
}

/// Main demo app component
#[component]
pub fn DemoApp() -> Element {
    use_context_provider(DialogContext::new);

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "pb-24 h-screen overflow-y-auto", Router::<DemoRoute> {} }
    }
}

/// Launch the demo app (web)
pub fn launch_demo() {
    dioxus::launch(DemoApp);
}
