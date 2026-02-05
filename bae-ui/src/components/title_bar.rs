//! Title bar view component
//!
//! Pure, props-based component for the app title bar with navigation and search.

use crate::components::icons::{DiscIcon, ImageIcon, SettingsIcon, UserIcon};
use crate::components::utils::format_duration;
use crate::components::{ChromelessButton, Dropdown, Placement};
use dioxus::prelude::*;

/// Well-known ID for the search input, used by "/" keyboard shortcut
pub const SEARCH_INPUT_ID: &str = "global-search-input";

/// Navigation item for title bar
#[derive(Clone, PartialEq)]
pub struct NavItem {
    pub id: String,
    pub label: String,
    pub is_active: bool,
}

/// Grouped search results for the dropdown
#[derive(Clone, PartialEq, Default)]
pub struct GroupedSearchResults {
    pub artists: Vec<ArtistResult>,
    pub albums: Vec<AlbumResult>,
    pub tracks: Vec<TrackResult>,
}

impl GroupedSearchResults {
    pub fn is_empty(&self) -> bool {
        self.artists.is_empty() && self.albums.is_empty() && self.tracks.is_empty()
    }
}

/// Artist search result
#[derive(Clone, PartialEq)]
pub struct ArtistResult {
    pub id: String,
    pub name: String,
    pub album_count: usize,
}

/// Album search result
#[derive(Clone, PartialEq)]
pub struct AlbumResult {
    pub id: String,
    pub title: String,
    pub artist_name: String,
    pub year: Option<i32>,
    pub cover_url: Option<String>,
}

/// Track search result
#[derive(Clone, PartialEq)]
pub struct TrackResult {
    pub id: String,
    pub album_id: String,
    pub title: String,
    pub artist_name: String,
    pub album_title: String,
    pub duration_ms: Option<i64>,
}

/// Action when a search result is clicked
#[derive(Clone, Debug)]
pub enum SearchAction {
    Artist(String),
    Album(String),
    Track { album_id: String },
}

/// Title bar view (pure, props-based)
/// Renders the visual structure with callbacks for all interactions.
#[component]
pub fn TitleBarView(
    // Navigation
    nav_items: Vec<NavItem>,
    on_nav_click: EventHandler<String>,
    // Search
    search_value: String,
    on_search_change: EventHandler<String>,
    search_results: GroupedSearchResults,
    on_search_result_click: EventHandler<SearchAction>,
    show_search_results: ReadSignal<bool>,
    on_search_dismiss: EventHandler<()>,
    on_search_focus: EventHandler<()>,
    on_search_blur: EventHandler<()>,
    // Settings
    on_settings_click: EventHandler<()>,
    #[props(default)] settings_active: bool,
    // Platform hooks (no-ops on web)
    #[props(default)] on_bar_mousedown: Option<EventHandler<()>>,
    #[props(default)] on_bar_double_click: Option<EventHandler<()>>,
    // Optional imports indicator slot
    #[props(default)] imports_indicator: Option<Element>,
    // Left padding for traffic lights on macOS
    #[props(default = 80)] left_padding: u32,
) -> Element {
    let mut is_search_active = use_signal(|| false);
    let search_active = is_search_active();

    rsx! {
        // Title bar
        div {
            id: "title-bar",
            class: "shrink-0 h-10 bg-surface-raised flex items-center justify-between px-2 cursor-default border-b border-border-subtle",
            style: "padding-left: {left_padding}px;",
            onmousedown: move |_| {
                if let Some(handler) = &on_bar_mousedown {
                    handler.call(());
                }
            },
            ondoubleclick: move |_| {
                if let Some(handler) = &on_bar_double_click {
                    handler.call(());
                }
            },

            // Left section: Navigation + imports indicator
            div {
                class: "flex gap-2 flex-none items-center",
                style: "-webkit-app-region: no-drag;",
                for item in nav_items.iter() {
                    NavButton {
                        key: "{item.id}",
                        is_active: item.is_active,
                        on_click: {
                            let id = item.id.clone();
                            move |_| on_nav_click.call(id.clone())
                        },
                        "{item.label}"
                    }
                }

                // Imports indicator
                if let Some(indicator) = imports_indicator {
                    div { class: "relative ml-2", {indicator} }
                }
            }

            // Right section: Search + Settings
            div {
                class: "flex-none flex items-center gap-2",
                style: "-webkit-app-region: no-drag;",

                // Search input with dropdown
                div {
                    class: "relative transition-all duration-200",
                    class: if search_active { "w-56" } else { "w-40" },
                    input {
                        id: SEARCH_INPUT_ID,
                        r#type: "text",
                        placeholder: "Search...",
                        autocomplete: "off",
                        class: "w-full h-7 px-2 bg-surface-input border border-border-default rounded text-white text-xs placeholder-gray-400 focus:outline-none focus:border-border-strong",
                        value: "{search_value}",
                        oninput: move |evt| on_search_change.call(evt.value()),
                        onfocus: move |_| {
                            is_search_active.set(true);
                            on_search_focus.call(());
                        },
                        onblur: move |_| {
                            is_search_active.set(false);
                            on_search_blur.call(());
                        },
                        onkeydown: move |evt| {
                            if evt.key() == Key::Escape {
                                on_search_dismiss.call(());
                            }
                        },
                    }

                    // Search results dropdown
                    if !search_results.is_empty() {
                        Dropdown {
                            anchor_id: SEARCH_INPUT_ID.to_string(),
                            is_open: show_search_results,
                            on_close: on_search_dismiss,
                            placement: Placement::Bottom,
                            class: "bg-surface-overlay border border-border-strong rounded-lg shadow-lg w-72 max-h-96 overflow-y-auto",
                            SearchResultsContent {
                                results: search_results,
                                on_click: on_search_result_click,
                            }
                        }
                    }
                }

                // Settings button
                NavButton {
                    is_active: settings_active,
                    on_click: move |_| on_settings_click.call(()),
                    SettingsIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}

/// Grouped search results content
#[component]
fn SearchResultsContent(
    results: GroupedSearchResults,
    on_click: EventHandler<SearchAction>,
) -> Element {
    rsx! {
        // Artists section
        if !results.artists.is_empty() {
            SearchSectionHeader { label: "Artists" }
            for artist in results.artists.iter() {
                ArtistResultItem {
                    key: "{artist.id}",
                    artist: artist.clone(),
                    on_click,
                }
            }
        }

        // Albums section
        if !results.albums.is_empty() {
            SearchSectionHeader { label: "Albums" }
            for album in results.albums.iter() {
                AlbumResultItem { key: "{album.id}", album: album.clone(), on_click }
            }
        }

        // Tracks section
        if !results.tracks.is_empty() {
            SearchSectionHeader { label: "Tracks" }
            for track in results.tracks.iter() {
                TrackResultItem { key: "{track.id}", track: track.clone(), on_click }
            }
        }
    }
}

/// Section header in search results
#[component]
fn SearchSectionHeader(label: &'static str) -> Element {
    rsx! {
        div { class: "px-3 py-1.5 text-[10px] font-semibold text-gray-500 uppercase tracking-wider",
            "{label}"
        }
    }
}

/// Artist result item
#[component]
fn ArtistResultItem(artist: ArtistResult, on_click: EventHandler<SearchAction>) -> Element {
    let id = artist.id.clone();
    let album_label = if artist.album_count == 1 {
        "1 album".to_string()
    } else {
        format!("{} albums", artist.album_count)
    };

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer",
            onclick: move |evt| {
                evt.stop_propagation();
                on_click.call(SearchAction::Artist(id.clone()));
            },
            div { class: "w-8 h-8 bg-gray-700 rounded-full flex items-center justify-center flex-shrink-0",
                UserIcon { class: "w-4 h-4 text-gray-400" }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-white text-xs font-medium truncate", "{artist.name}" }
                div { class: "text-gray-400 text-[10px] truncate", "{album_label}" }
            }
        }
    }
}

/// Album result item
#[component]
fn AlbumResultItem(album: AlbumResult, on_click: EventHandler<SearchAction>) -> Element {
    let id = album.id.clone();
    let subtitle = if let Some(year) = album.year {
        format!("{} \u{2022} {}", album.artist_name, year)
    } else {
        album.artist_name.clone()
    };

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer",
            onclick: move |evt| {
                evt.stop_propagation();
                on_click.call(SearchAction::Album(id.clone()));
            },
            if let Some(url) = &album.cover_url {
                img {
                    src: "{url}",
                    class: "w-8 h-8 rounded object-cover flex-shrink-0",
                    alt: "{album.title}",
                }
            } else {
                div { class: "w-8 h-8 bg-gray-700 rounded flex items-center justify-center flex-shrink-0",
                    ImageIcon { class: "w-4 h-4 text-gray-500" }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-white text-xs font-medium truncate", "{album.title}" }
                div { class: "text-gray-400 text-[10px] truncate", "{subtitle}" }
            }
        }
    }
}

/// Track result item
#[component]
fn TrackResultItem(track: TrackResult, on_click: EventHandler<SearchAction>) -> Element {
    let album_id = track.album_id.clone();
    let subtitle = format!("{} \u{2022} {}", track.album_title, track.artist_name);
    let duration = track.duration_ms.map(format_duration).unwrap_or_default();

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer",
            onclick: move |evt| {
                evt.stop_propagation();
                on_click
                    .call(SearchAction::Track {
                        album_id: album_id.clone(),
                    });
            },
            div { class: "w-8 h-8 bg-gray-700 rounded flex items-center justify-center flex-shrink-0",
                DiscIcon { class: "w-4 h-4 text-gray-500" }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-white text-xs font-medium truncate", "{track.title}" }
                div { class: "text-gray-400 text-[10px] truncate", "{subtitle}" }
            }
            if !duration.is_empty() {
                span { class: "text-gray-500 text-[10px] flex-shrink-0", "{duration}" }
            }
        }
    }
}

/// Navigation button with generic children
#[component]
fn NavButton(is_active: bool, on_click: EventHandler<()>, children: Element) -> Element {
    let class = if is_active {
        "text-white text-[12px] cursor-pointer px-2 py-1.5 rounded bg-gray-700 transition-colors"
    } else {
        "text-gray-400 text-[12px] cursor-pointer px-2 py-1.5 rounded hover:bg-gray-700 hover:text-white transition-colors"
    };

    rsx! {
        span {
            class: "inline-block",
            onmousedown: move |evt| evt.stop_propagation(),
            ChromelessButton {
                class: Some(class.to_string()),
                onclick: move |_| on_click.call(()),
                {children}
            }
        }
    }
}
