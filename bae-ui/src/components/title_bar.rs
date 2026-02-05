//! Title bar view component
//!
//! Pure, props-based component for the app title bar with navigation and search.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::components::icons::{ChevronDownIcon, DiscIcon, ImageIcon, SettingsIcon, UserIcon};
use crate::components::utils::format_duration;
use crate::components::{ChromelessButton, Dropdown, Placement};
use dioxus::prelude::*;

/// Counter for generating unique element IDs
static BUTTON_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    on_search_focus: EventHandler<()>,
    on_search_blur: EventHandler<()>,
    // Settings
    on_settings_click: EventHandler<()>,
    #[props(default)] settings_active: bool,
    // Platform hooks (no-ops on web)
    #[props(default)] on_bar_mousedown: Option<EventHandler<()>>,
    #[props(default)] on_bar_double_click: Option<EventHandler<()>>,
    // Import split button
    #[props(default)] import_count: usize,
    #[props(default)] show_imports_dropdown: Option<ReadSignal<bool>>,
    #[props(default)] on_imports_dropdown_toggle: Option<EventHandler<()>>,
    #[props(default)] on_imports_dropdown_close: Option<EventHandler<()>>,
    #[props(default)] imports_dropdown_content: Option<Element>,
    // Left padding for traffic lights on macOS
    #[props(default = 80)] left_padding: u32,
) -> Element {
    let mut is_search_active = use_signal(|| false);
    let search_active = is_search_active();
    let mut selected_index = use_signal(|| None::<usize>);

    // Flat list of actions for keyboard navigation (arrow keys + Enter)
    let nav_actions: Vec<SearchAction> = {
        let mut list = Vec::new();
        for a in &search_results.artists {
            list.push(SearchAction::Artist(a.id.clone()));
        }
        for a in &search_results.albums {
            list.push(SearchAction::Album(a.id.clone()));
        }
        for t in &search_results.tracks {
            list.push(SearchAction::Track {
                album_id: t.album_id.clone(),
            });
        }
        list
    };
    let total_results = nav_actions.len();

    let chevron_button_id = use_hook(|| {
        let id = BUTTON_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("imports-chevron-{}", id)
    });

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

            // Left section: Navigation
            div {
                class: "flex gap-2 flex-none items-center",
                style: "-webkit-app-region: no-drag;",
                for item in nav_items.iter() {
                    if item.id == "import" && import_count > 0 {
                        ImportSplitButton {
                            key: "{item.id}",
                            is_active: item.is_active,
                            label: item.label.clone(),
                            chevron_button_id: chevron_button_id.clone(),
                            on_label_click: {
                                let id = item.id.clone();
                                move |_| on_nav_click.call(id.clone())
                            },
                            on_chevron_click: move |_| {
                                if let Some(handler) = &on_imports_dropdown_toggle {
                                    handler.call(());
                                }
                            },
                        }
                    } else {
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
                }
            }

            // Right section: Search + Settings
            div {
                class: "flex-none flex items-center gap-2",
                style: "-webkit-app-region: no-drag;",

                // Search input with autocomplete results
                div {
                    class: "relative transition-all duration-200",
                    class: if search_active { "w-56" } else { "w-40" },
                    // Stop mousedown propagation so title bar drag doesn't
                    // interfere with focus.
                    onmousedown: move |evt| evt.stop_propagation(),
                    input {
                        id: SEARCH_INPUT_ID,
                        r#type: "text",
                        placeholder: "Search...",
                        autocomplete: "off",
                        class: "w-full h-7 px-2 bg-surface-input border border-border-default rounded text-white text-xs placeholder-gray-400 focus:outline-none focus:border-border-strong",
                        value: "{search_value}",
                        oninput: move |evt| {
                            selected_index.set(None);
                            on_search_change.call(evt.value());
                        },
                        onfocus: move |_| {
                            is_search_active.set(true);
                            on_search_focus.call(());
                        },
                        onblur: move |_| {
                            // Only narrow the search bar if the document still has focus
                            // (user clicked elsewhere in the app). Skip when the window
                            // is deactivating so we don't replay the width animation on
                            // reactivation.
                            let doc_has_focus = web_sys_x::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.has_focus().ok())
                                .unwrap_or(true);

                            if doc_has_focus {
                                is_search_active.set(false);
                                selected_index.set(None);
                            }
                            on_search_blur.call(());
                        },
                        onkeydown: move |evt| {
                            match evt.key() {
                                Key::Escape => {
                                    spawn(async move {
                                        let js = format!(
                                            "document.getElementById('{}')?.blur()",
                                            SEARCH_INPUT_ID,
                                        );
                                        dioxus::document::eval(&js);
                                    });
                                }
                                Key::ArrowDown if total_results > 0 => {
                                    evt.prevent_default();
                                    let next = match selected_index() {
                                        None => 0,
                                        Some(i) => (i + 1) % total_results,
                                    };
                                    selected_index.set(Some(next));
                                }
                                Key::ArrowUp if total_results > 0 => {
                                    evt.prevent_default();
                                    let next = match selected_index() {
                                        None | Some(0) => total_results - 1,
                                        Some(i) => i - 1,
                                    };
                                    selected_index.set(Some(next));
                                }
                                Key::Enter => {
                                    if let Some(i) = selected_index() {
                                        if let Some(action) = nav_actions.get(i) {
                                            on_search_result_click.call(action.clone());
                                            blur_search_input();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        },
                    }

                    // Search results panel (visible when focused with results)
                    if search_active && !search_results.is_empty() {
                        div {
                            class: "absolute top-full right-0 mt-1 bg-surface-overlay border border-border-strong rounded-lg shadow-lg w-72 max-h-96 overflow-y-auto z-50",
                            // Prevent mousedown from blurring the search input
                            onmousedown: move |evt| evt.prevent_default(),
                            SearchResultsContent {
                                results: search_results,
                                on_click: move |action: SearchAction| {
                                    on_search_result_click.call(action);
                                    blur_search_input();
                                },
                                selected_index: selected_index(),
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

        // Imports dropdown (anchored to chevron button via popover API)
        if let Some(is_open) = show_imports_dropdown {
            if let Some(content) = &imports_dropdown_content {
                Dropdown {
                    anchor_id: chevron_button_id.clone(),
                    is_open,
                    on_close: move |_| {
                        if let Some(handler) = &on_imports_dropdown_close {
                            handler.call(());
                        }
                    },
                    placement: Placement::Bottom,
                    class: "w-96 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl overflow-clip",
                    {content.clone()}
                }
            }
        }
    }
}

/// Split button for the Import nav item: [Import | ▼]
#[component]
fn ImportSplitButton(
    is_active: bool,
    label: String,
    chevron_button_id: String,
    on_label_click: EventHandler<()>,
    on_chevron_click: EventHandler<()>,
) -> Element {
    let active_class = if is_active {
        "bg-gray-700 text-white"
    } else {
        "text-gray-400 hover:text-white"
    };

    let left_hover = if is_active { "" } else { "hover:bg-gray-700" };
    let right_hover = if is_active { "" } else { "hover:bg-gray-700" };

    rsx! {
        span {
            class: "inline-flex items-center",
            onmousedown: move |evt| evt.stop_propagation(),

            // Left part: label
            ChromelessButton {
                class: Some(
                    format!(
                        "text-[12px] cursor-pointer px-2 py-1.5 rounded-l {active_class} {left_hover} transition-colors",
                    ),
                ),
                onclick: move |_| on_label_click.call(()),
                "{label}"
            }

            // Divider
            span { class: "w-px h-4 bg-gray-600" }

            // Right part: chevron
            ChromelessButton {
                id: Some(chevron_button_id.clone()),
                class: Some(
                    format!(
                        "text-[12px] cursor-pointer px-1 py-1.5 rounded-r {active_class} {right_hover} transition-colors",
                    ),
                ),
                onclick: move |_| on_chevron_click.call(()),
                ChevronDownIcon { class: "w-3 h-3" }
            }
        }
    }
}

/// Grouped search results content
#[component]
fn SearchResultsContent(
    results: GroupedSearchResults,
    on_click: EventHandler<SearchAction>,
    selected_index: Option<usize>,
) -> Element {
    let album_offset = results.artists.len();
    let track_offset = album_offset + results.albums.len();

    rsx! {
        // Artists section
        if !results.artists.is_empty() {
            SearchSectionHeader { label: "Artists" }
            for (i , artist) in results.artists.iter().enumerate() {
                ArtistResultItem {
                    key: "{artist.id}",
                    artist: artist.clone(),
                    is_selected: selected_index == Some(i),
                    on_click,
                }
            }
        }

        // Albums section
        if !results.albums.is_empty() {
            SearchSectionHeader { label: "Albums" }
            for (i , album) in results.albums.iter().enumerate() {
                AlbumResultItem {
                    key: "{album.id}",
                    album: album.clone(),
                    is_selected: selected_index == Some(album_offset + i),
                    on_click,
                }
            }
        }

        // Tracks section
        if !results.tracks.is_empty() {
            SearchSectionHeader { label: "Tracks" }
            for (i , track) in results.tracks.iter().enumerate() {
                TrackResultItem {
                    key: "{track.id}",
                    track: track.clone(),
                    is_selected: selected_index == Some(track_offset + i),
                    on_click,
                }
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
fn ArtistResultItem(
    artist: ArtistResult,
    is_selected: bool,
    on_click: EventHandler<SearchAction>,
) -> Element {
    let id = artist.id.clone();
    let album_label = if artist.album_count == 1 {
        "1 album".to_string()
    } else {
        format!("{} albums", artist.album_count)
    };
    let selected_class = if is_selected { "bg-hover" } else { "" };

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer {selected_class}",
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
fn AlbumResultItem(
    album: AlbumResult,
    is_selected: bool,
    on_click: EventHandler<SearchAction>,
) -> Element {
    let id = album.id.clone();
    let subtitle = if let Some(year) = album.year {
        format!("{} \u{2022} {}", album.artist_name, year)
    } else {
        album.artist_name.clone()
    };
    let selected_class = if is_selected { "bg-hover" } else { "" };

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer {selected_class}",
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
fn TrackResultItem(
    track: TrackResult,
    is_selected: bool,
    on_click: EventHandler<SearchAction>,
) -> Element {
    let album_id = track.album_id.clone();
    let subtitle = format!("{} \u{2022} {}", track.album_title, track.artist_name);
    let duration = track.duration_ms.map(format_duration).unwrap_or_default();
    let selected_class = if is_selected { "bg-hover" } else { "" };

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover cursor-pointer {selected_class}",
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

/// Blur the search input by spawning a deferred eval.
///
/// We spawn instead of calling eval inline to avoid re-entrant borrow panics
/// in wry's webview bridge (the blur triggers `onblur` synchronously).
fn blur_search_input() {
    spawn(async move {
        let js = format!("document.getElementById('{}')?.blur()", SEARCH_INPUT_ID,);
        dioxus::document::eval(&js);
    });
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
