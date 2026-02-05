//! Title bar wrapper for desktop app
//!
//! Wraps the shared TitleBarView with desktop-specific behavior:
//! window dragging (macOS), zoom (macOS), and database-backed search.

use crate::ui::app_service::use_app;
use crate::ui::components::imports_dropdown::ImportsDropdown;
use crate::ui::Route;
use bae_ui::stores::{
    ActiveImportsUiStateStoreExt, AppStateStoreExt, LibraryStateStoreExt, SearchStateStoreExt,
    UiStateStoreExt,
};
use bae_ui::{
    AlbumResult, ArtistResult, GroupedSearchResults, NavItem, SearchAction, TitleBarView,
    TrackResult,
};
#[cfg(target_os = "macos")]
use cocoa::appkit::NSApplication;
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use dioxus::desktop::use_window;
use dioxus::prelude::*;
#[cfg(target_os = "macos")]
use dispatch::Queue;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
use std::collections::HashMap;

/// Custom title bar component with navigation and search
/// On macOS: includes window dragging, zoom, and space for traffic lights
/// On Linux/Windows: simpler version without native window controls
#[component]
pub fn TitleBar() -> Element {
    #[cfg(target_os = "macos")]
    let window = use_window();

    let app = use_app();
    let current_route = use_route::<Route>();
    let search_store = app.state.ui().search();
    let mut search_query_store = search_store.query();
    let mut show_results = use_signal(|| false);
    let show_results_read: ReadSignal<bool> = show_results.into();
    let mut search_results = use_signal(GroupedSearchResults::default);
    let mut imports_dropdown_open = use_signal(|| false);
    let imports_dropdown_open_read: ReadSignal<bool> = imports_dropdown_open.into();

    // Read albums/artists from global store for suggestion computation
    let artists_store = app.state.library().artists_by_album();

    // Read import count for split button
    let import_count = app.state.active_imports().imports().read().len();

    // Search effect: when query changes, search the DB or show suggestions
    use_effect({
        let library_manager = app.library_manager.clone();
        move || {
            let query = search_query_store.read().clone();
            if query.is_empty() {
                search_results.set(GroupedSearchResults::default());
                show_results.set(false);
            } else {
                let library_manager = library_manager.clone();
                let query = query.clone();
                spawn(async move {
                    match library_manager.search_library(&query, 5).await {
                        Ok(db_results) => {
                            let grouped = GroupedSearchResults {
                                artists: db_results
                                    .artists
                                    .into_iter()
                                    .map(|a| ArtistResult {
                                        id: a.id,
                                        name: a.name,
                                        album_count: a.album_count as usize,
                                    })
                                    .collect(),
                                albums: db_results
                                    .albums
                                    .into_iter()
                                    .map(|a| AlbumResult {
                                        id: a.id,
                                        title: a.title,
                                        artist_name: a.artist_name,
                                        year: a.year,
                                        cover_url: a.cover_art_url,
                                    })
                                    .collect(),
                                tracks: db_results
                                    .tracks
                                    .into_iter()
                                    .map(|t| TrackResult {
                                        id: t.id,
                                        album_id: t.album_id,
                                        title: t.title,
                                        artist_name: t.artist_name,
                                        album_title: t.album_title,
                                        duration_ms: t.duration_ms,
                                    })
                                    .collect(),
                            };
                            search_results.set(grouped);
                            show_results.set(true);
                        }
                        Err(e) => {
                            tracing::warn!("Search failed: {}", e);
                        }
                    }
                });
            }
        }
    });

    // Build nav items
    let nav_items = vec![
        NavItem {
            id: "library".to_string(),
            label: "Library".to_string(),
            is_active: matches!(
                current_route,
                Route::Library {} | Route::AlbumDetail { .. } | Route::ArtistDetail { .. }
            ),
        },
        NavItem {
            id: "import".to_string(),
            label: "Import".to_string(),
            is_active: matches!(current_route, Route::ImportWorkflowManager {}),
        },
    ];

    // Platform-specific: left padding for traffic lights on macOS
    #[cfg(target_os = "macos")]
    let left_padding = 80;
    #[cfg(not(target_os = "macos"))]
    let left_padding = 16;

    // Platform-specific: window drag handler (macOS only)
    #[cfg(target_os = "macos")]
    let on_bar_mousedown = Some(EventHandler::new(move |_| {
        let _ = window.drag_window();
    }));
    #[cfg(not(target_os = "macos"))]
    let on_bar_mousedown: Option<EventHandler<()>> = None;

    // Platform-specific: window zoom handler (macOS only)
    #[cfg(target_os = "macos")]
    let on_bar_double_click = Some(EventHandler::new(move |_| perform_zoom()));
    #[cfg(not(target_os = "macos"))]
    let on_bar_double_click: Option<EventHandler<()>> = None;

    rsx! {
        TitleBarView {
            nav_items,
            on_nav_click: move |id: String| {
                let route = match id.as_str() {
                    "library" => Route::Library {},
                    "import" => Route::ImportWorkflowManager {},
                    _ => return,
                };
                navigator().push(route);
            },
            search_value: search_query_store.read().clone(),
            on_search_change: move |value| search_query_store.set(value),
            search_results: search_results(),
            on_search_result_click: move |action: SearchAction| {
                show_results.set(false);
                search_query_store.set(String::new());
                match action {
                    SearchAction::Artist(artist_id) => {
                        navigator().push(Route::ArtistDetail { artist_id });
                    }
                    SearchAction::Album(album_id) => {
                        navigator()
                            .push(Route::AlbumDetail {
                                album_id,
                                release_id: String::new(),
                            });
                    }
                    SearchAction::Track { album_id } => {
                        navigator()
                            .push(Route::AlbumDetail {
                                album_id,
                                release_id: String::new(),
                            });
                    }
                }
            },
            show_search_results: show_results_read,
            on_search_dismiss: move |_| show_results.set(false),
            on_search_focus: move |_| {
                if search_query_store.read().is_empty() {
                    // Show top artists as suggestions
                    let top_artists = compute_top_artists(&artists_store.read());
                    if !top_artists.is_empty() {
                        search_results
                            .set(GroupedSearchResults {
                                artists: top_artists,
                                albums: vec![],
                                tracks: vec![],
                            });
                        show_results.set(true);
                    }
                } else {
                    show_results.set(true);
                }
            },
            on_search_blur: |_| {},
            on_settings_click: move |_| {
                navigator().push(Route::Settings {});
            },
            settings_active: matches!(current_route, Route::Settings {}),
            on_bar_mousedown,
            on_bar_double_click,
            import_count,
            show_imports_dropdown: Some(imports_dropdown_open_read),
            on_imports_dropdown_toggle: Some(EventHandler::new(move |_| imports_dropdown_open.toggle())),
            on_imports_dropdown_close: Some(EventHandler::new(move |_| imports_dropdown_open.set(false))),
            imports_dropdown_content: rsx! {
                ImportsDropdown {}
            },
            left_padding,
        }
    }
}

/// Compute top artists by album count from the artists_by_album map
fn compute_top_artists(
    artists_by_album: &HashMap<String, Vec<bae_ui::Artist>>,
) -> Vec<ArtistResult> {
    // Count albums per artist
    let mut artist_counts: HashMap<String, (String, usize)> = HashMap::new();
    for artists in artists_by_album.values() {
        for artist in artists {
            artist_counts
                .entry(artist.id.clone())
                .and_modify(|(_, count)| *count += 1)
                .or_insert((artist.name.clone(), 1));
        }
    }

    // Sort by album count descending, take top 5
    let mut sorted: Vec<_> = artist_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

    sorted
        .into_iter()
        .take(5)
        .map(|(id, (name, album_count))| ArtistResult {
            id,
            name,
            album_count,
        })
        .collect()
}

/// Perform window zoom (maximize/restore) using native macOS API
#[cfg(target_os = "macos")]
fn perform_zoom() {
    Queue::main().exec_async(|| unsafe {
        let app = NSApplication::sharedApplication(nil);
        let window: id = msg_send![app, keyWindow];
        if window != nil {
            let _: () = msg_send![window, performZoom: nil];
        }
    });
}
