//! Library view component - pure rendering, no data fetching
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
//! Only subscribes to specific fields needed for routing decisions.

use crate::components::album_card::AlbumCard;
use crate::components::helpers::{ErrorDisplay, LoadingSpinner};
use crate::components::icons::{ArrowDownIcon, ArrowUpIcon, ChevronDownIcon, PlusIcon, XIcon};
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton};
use crate::components::{MenuDropdown, MenuItem, Placement};
use crate::display_types::{Album, Artist, LibrarySortField, SortCriterion, SortDirection};
use crate::stores::library::{LibraryState, LibraryStateStoreExt};
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

/// Sort albums based on an ordered list of criteria
fn sort_albums(
    albums: &[Album],
    artists_by_album: &HashMap<String, Vec<Artist>>,
    criteria: &[SortCriterion],
) -> Vec<Album> {
    let mut sorted = albums.to_vec();
    sorted.sort_by(|a, b| {
        for criterion in criteria {
            let cmp = match criterion.field {
                LibrarySortField::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                LibrarySortField::Artist => {
                    let artist_a = artists_by_album
                        .get(&a.id)
                        .and_then(|v| v.first())
                        .map(|a| a.name.to_lowercase())
                        .unwrap_or_default();
                    let artist_b = artists_by_album
                        .get(&b.id)
                        .and_then(|v| v.first())
                        .map(|a| a.name.to_lowercase())
                        .unwrap_or_default();
                    artist_a.cmp(&artist_b)
                }
                LibrarySortField::Year => a.year.cmp(&b.year),
                LibrarySortField::DateAdded => a.date_added.cmp(&b.date_added),
            };
            let cmp = match criterion.direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            };
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
    sorted
}

fn sort_field_label(field: LibrarySortField) -> &'static str {
    match field {
        LibrarySortField::Title => "Title",
        LibrarySortField::Artist => "Artist",
        LibrarySortField::Year => "Year",
        LibrarySortField::DateAdded => "Date Added",
    }
}

/// Library view component - pure rendering, no data fetching
///
/// Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
#[component]
pub fn LibraryView(
    state: ReadStore<LibraryState>,
    // Navigation callback - called with album_id when an album is clicked
    on_album_click: EventHandler<String>,
    // Navigation callback - called with artist_id when an artist name is clicked
    on_artist_click: EventHandler<String>,
    // Action callbacks
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    // Empty state action (e.g., navigate to import)
    on_empty_action: EventHandler<()>,
) -> Element {
    // Use lenses to subscribe only to specific fields for routing decisions
    let loading = *state.loading().read();
    let error = state.error().read().clone();
    let albums = state.albums().read().clone();
    let artists_by_album = state.artists_by_album().read().clone();

    // Sort state is local to the view (UI concern, not persisted)
    let sort_criteria = use_signal(|| {
        vec![SortCriterion {
            field: LibrarySortField::DateAdded,
            direction: SortDirection::Descending,
        }]
    });

    let sorted_albums = sort_albums(&albums, &artists_by_album, &sort_criteria.read());
    let album_count = sorted_albums.len();

    let mut scroll_target: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex-grow overflow-y-auto flex flex-col py-10",
            onmounted: move |evt| scroll_target.set(Some(evt.data())),
            div { class: "container mx-auto flex flex-col flex-1",
                h1 { class: "text-3xl font-bold text-white mb-6", "Music Library" }
                if loading {
                    LoadingSpinner { message: "Loading your music library...".to_string() }
                } else if let Some(err) = error {
                    ErrorDisplay { message: err }
                    p { class: "text-sm mt-2 text-gray-400",
                        "An error occurred while loading your music library."
                    }
                } else if albums.is_empty() {
                    div { class: "flex-1 flex flex-col items-center justify-center",
                        p { class: "text-gray-500 mb-4", "No albums in your library" }
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Medium,
                            onclick: move |_| on_empty_action.call(()),
                            "Import"
                        }
                    }
                } else {
                    SortToolbar { album_count, sort_criteria }

                    AlbumGrid {
                        albums: sorted_albums,
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

/// Sort controls toolbar
#[component]
fn SortToolbar(album_count: usize, sort_criteria: Signal<Vec<SortCriterion>>) -> Element {
    let criteria = sort_criteria.read().clone();
    let used_fields: Vec<LibrarySortField> = criteria.iter().map(|c| c.field).collect();
    let all_used = used_fields.len() >= LibrarySortField::ALL.len();

    let album_label = if album_count == 1 {
        "1 album".to_string()
    } else {
        format!("{} albums", album_count)
    };

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            span { class: "text-sm text-gray-400", "{album_label}" }

            div { class: "flex items-center gap-1",
                for (idx , criterion) in criteria.iter().enumerate() {
                    SortCriterionItem {
                        key: "{idx}",
                        index: idx,
                        criterion: *criterion,
                        sort_criteria,
                        removable: criteria.len() > 1,
                    }
                }

                if !all_used {
                    ChromelessButton {
                        class: Some(
                            "p-1 rounded-md text-gray-400 hover:text-white hover:bg-hover transition-all"
                                .to_string(),
                        ),
                        aria_label: Some("Add sort criterion".to_string()),
                        onclick: move |_| {
                            let mut criteria = sort_criteria.write();
                            let used: Vec<_> = criteria.iter().map(|c| c.field).collect();
                            if let Some(next_field) = LibrarySortField::ALL
                                .iter()
                                .find(|f| !used.contains(f))
                            {
                                criteria
                                    .push(SortCriterion {
                                        field: *next_field,
                                        direction: SortDirection::Ascending,
                                    });
                            }
                        },
                        PlusIcon { class: "w-4 h-4" }
                    }
                }
            }
        }
    }
}

/// Single sort criterion: field dropdown + direction toggle + remove button
#[component]
fn SortCriterionItem(
    index: usize,
    criterion: SortCriterion,
    sort_criteria: Signal<Vec<SortCriterion>>,
    removable: bool,
) -> Element {
    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let anchor_id = format!("sort-field-btn-{index}");

    // Available fields: current field + any unused fields
    let used_fields: Vec<LibrarySortField> = sort_criteria.read().iter().map(|c| c.field).collect();
    let available_fields: Vec<LibrarySortField> = LibrarySortField::ALL
        .iter()
        .filter(|f| **f == criterion.field || !used_fields.contains(f))
        .copied()
        .collect();

    rsx! {
        div { class: "flex items-center gap-0.5",
            // Field dropdown trigger
            ChromelessButton {
                id: Some(anchor_id.clone()),
                class: Some(
                    "flex items-center gap-1 px-2 py-1 rounded-md text-sm text-gray-400 hover:text-white hover:bg-hover transition-all"
                        .to_string(),
                ),
                aria_label: Some("Sort by".to_string()),
                onclick: move |_| show_menu.set(!show_menu()),
                "{sort_field_label(criterion.field)}"
                ChevronDownIcon { class: "w-3 h-3" }
            }

            MenuDropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_menu.set(false),
                placement: Placement::BottomEnd,

                for field in available_fields {
                    MenuItem {
                        onclick: move |_| {
                            show_menu.set(false);
                            sort_criteria.write()[index].field = field;
                        },
                        span { class: if criterion.field == field { "text-accent-soft" } else { "" },
                            "{sort_field_label(field)}"
                        }
                    }
                }
            }

            // Direction toggle
            ChromelessButton {
                class: Some(
                    "p-1 rounded-md text-gray-400 hover:text-white hover:bg-hover transition-all"
                        .to_string(),
                ),
                aria_label: Some(
                    if criterion.direction == SortDirection::Ascending {
                        "Sort descending"
                    } else {
                        "Sort ascending"
                    }
                        .to_string(),
                ),
                onclick: move |_| {
                    let new_dir = match criterion.direction {
                        SortDirection::Ascending => SortDirection::Descending,
                        SortDirection::Descending => SortDirection::Ascending,
                    };
                    sort_criteria.write()[index].direction = new_dir;
                },
                if criterion.direction == SortDirection::Ascending {
                    ArrowUpIcon { class: "w-3.5 h-3.5" }
                } else {
                    ArrowDownIcon { class: "w-3.5 h-3.5" }
                }
            }

            // Remove button
            if removable {
                ChromelessButton {
                    class: Some(
                        "p-1 rounded-md text-gray-400 hover:text-white hover:bg-hover transition-all"
                            .to_string(),
                    ),
                    aria_label: Some("Remove sort criterion".to_string()),
                    onclick: move |_| {
                        sort_criteria.write().remove(index);
                    },
                    XIcon { class: "w-3 h-3" }
                }
            }
        }
    }
}

/// Grid component to display albums with virtual scrolling
#[component]
fn AlbumGrid(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    on_album_click: EventHandler<String>,
    on_artist_click: EventHandler<String>,
    on_play_album: EventHandler<String>,
    on_add_album_to_queue: EventHandler<String>,
    scroll_target: ScrollTarget,
) -> Element {
    // Prepare items by joining albums with their artists
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

    // Track which album's dropdown menu is open. Hoisted here so the signal
    // outlives virtual scroll item scopes (prevents use-after-drop on recycled items).
    let open_dropdown: Signal<Option<String>> = use_signal(|| None);

    // Create render function that captures the event handlers
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

    // Key function extracts album ID for stable DOM keys
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
