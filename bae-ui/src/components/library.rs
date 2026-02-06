//! Library view component - pure rendering, no data fetching
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
//! Only subscribes to specific fields needed for routing decisions.

use crate::components::album_card::AlbumCard;
use crate::components::helpers::{ErrorDisplay, LoadingSpinner};
use crate::components::icons::{
    ArrowDownIcon, ArrowUpIcon, ChevronDownIcon, PlusIcon, UserIcon, XIcon,
};
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton};
use crate::components::{MenuDropdown, MenuItem, Placement};
use crate::display_types::{
    Album, Artist, LibrarySortField, LibraryViewMode, SortCriterion, SortDirection,
};
use crate::stores::library::{LibraryState, LibraryStateStoreExt};
use crate::stores::ui::{LibrarySortState, LibrarySortStateStoreExt};
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

fn view_mode_label(mode: LibraryViewMode) -> &'static str {
    match mode {
        LibraryViewMode::Albums => "Albums",
        LibraryViewMode::Artists => "Artists",
    }
}

/// Library view component - pure rendering, no data fetching
///
/// Accepts `ReadStore<LibraryState>` and uses lenses for granular reactivity.
#[component]
pub fn LibraryView(
    state: ReadStore<LibraryState>,
    sort_state: ReadStore<LibrarySortState>,
    on_sort_criteria_change: EventHandler<Vec<SortCriterion>>,
    on_view_mode_change: EventHandler<LibraryViewMode>,
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

    let sort_criteria = sort_state.sort_criteria().read().clone();
    let view_mode = *sort_state.view_mode().read();

    let sorted_albums = sort_albums(&albums, &artists_by_album, &sort_criteria);
    let mut scroll_target: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex-grow overflow-y-auto flex flex-col py-10",
            onmounted: move |evt| scroll_target.set(Some(evt.data())),
            div { class: "container mx-auto flex flex-col flex-1",
                // Header row: title + controls on same line
                div { class: "flex items-center justify-between mb-6",
                    h1 { class: "text-3xl font-bold text-white", "Music Library" }

                    if !loading && error.is_none() && !albums.is_empty() {
                        SortToolbar {
                            sort_criteria: sort_criteria.clone(),
                            view_mode,
                            on_sort_criteria_change,
                            on_view_mode_change,
                        }
                    }
                }

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
                    match view_mode {
                        LibraryViewMode::Albums => rsx! {
                            AlbumGrid {
                                albums: sorted_albums,
                                artists_by_album,
                                on_album_click,
                                on_artist_click,
                                on_play_album,
                                on_add_album_to_queue,
                                scroll_target: ScrollTarget::Element(scroll_target.into()),
                            }
                        },
                        LibraryViewMode::Artists => rsx! {
                            ArtistListView { albums, artists_by_album, on_artist_click }
                        },
                    }
                }
            }
        }
    }
}

/// Sort controls toolbar (inline with header)
#[component]
fn SortToolbar(
    sort_criteria: Vec<SortCriterion>,
    view_mode: LibraryViewMode,
    on_sort_criteria_change: EventHandler<Vec<SortCriterion>>,
    on_view_mode_change: EventHandler<LibraryViewMode>,
) -> Element {
    let used_fields: Vec<LibrarySortField> = sort_criteria.iter().map(|c| c.field).collect();
    let all_used = used_fields.len() >= LibrarySortField::ALL.len();

    rsx! {
        div { class: "flex items-center gap-4",
            ViewModeDropdown { view_mode, on_view_mode_change }

            if view_mode == LibraryViewMode::Albums {
                div { class: "flex items-center gap-1",
                    for (idx , criterion) in sort_criteria.iter().enumerate() {
                        SortCriterionItem {
                            key: "{idx}",
                            index: idx,
                            criterion: *criterion,
                            sort_criteria: sort_criteria.clone(),
                            on_sort_criteria_change,
                        }
                    }

                    if !all_used {
                        ChromelessButton {
                            class: Some(
                                "p-1 rounded-md text-gray-400 hover:text-white hover:bg-hover transition-all"
                                    .to_string(),
                            ),
                            aria_label: Some("Add sort criterion".to_string()),
                            onclick: {
                                let sort_criteria = sort_criteria.clone();
                                move |_| {
                                    let used: Vec<_> = sort_criteria.iter().map(|c| c.field).collect();
                                    if let Some(next_field) = LibrarySortField::ALL
                                        .iter()
                                        .find(|f| !used.contains(f))
                                    {
                                        let mut new_criteria = sort_criteria.clone();
                                        new_criteria
                                            .push(SortCriterion {
                                                field: *next_field,
                                                direction: SortDirection::Ascending,
                                            });
                                        on_sort_criteria_change.call(new_criteria);
                                    }
                                }
                            },
                            PlusIcon { class: "w-4 h-4" }
                        }
                    }
                }
            }
        }
    }
}

/// View mode dropdown (Albums / Artists)
#[component]
fn ViewModeDropdown(
    view_mode: LibraryViewMode,
    on_view_mode_change: EventHandler<LibraryViewMode>,
) -> Element {
    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let anchor_id = "view-mode-btn";

    rsx! {
        ChromelessButton {
            id: Some(anchor_id.to_string()),
            class: Some(
                "flex items-center gap-1 px-2 py-1 rounded-md text-sm text-gray-400 hover:text-white hover:bg-hover transition-all"
                    .to_string(),
            ),
            aria_label: Some("View mode".to_string()),
            onclick: move |_| show_menu.set(!show_menu()),
            "{view_mode_label(view_mode)}"
            ChevronDownIcon { class: "w-3 h-3" }
        }

        MenuDropdown {
            anchor_id: anchor_id.to_string(),
            is_open,
            on_close: move |_| show_menu.set(false),
            placement: Placement::BottomEnd,

            for mode in [LibraryViewMode::Albums, LibraryViewMode::Artists] {
                MenuItem {
                    onclick: move |_| {
                        show_menu.set(false);
                        on_view_mode_change.call(mode);
                    },
                    span { class: if view_mode == mode { "text-accent-soft" } else { "" },
                        "{view_mode_label(mode)}"
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
    sort_criteria: Vec<SortCriterion>,
    on_sort_criteria_change: EventHandler<Vec<SortCriterion>>,
) -> Element {
    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let anchor_id = format!("sort-field-btn-{index}");
    let removable = sort_criteria.len() > 1;

    // Available fields: current field + any unused fields
    let used_fields: Vec<LibrarySortField> = sort_criteria.iter().map(|c| c.field).collect();
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
                        onclick: {
                            let sort_criteria = sort_criteria.clone();
                            move |_| {
                                show_menu.set(false);
                                let mut new_criteria = sort_criteria.clone();
                                new_criteria[index].field = field;
                                on_sort_criteria_change.call(new_criteria);
                            }
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
                onclick: {
                    let sort_criteria = sort_criteria.clone();
                    move |_| {
                        let new_dir = match criterion.direction {
                            SortDirection::Ascending => SortDirection::Descending,
                            SortDirection::Descending => SortDirection::Ascending,
                        };
                        let mut new_criteria = sort_criteria.clone();
                        new_criteria[index].direction = new_dir;
                        on_sort_criteria_change.call(new_criteria);
                    }
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
                    onclick: {
                        let sort_criteria = sort_criteria.clone();
                        move |_| {
                            let mut new_criteria = sort_criteria.clone();
                            new_criteria.remove(index);
                            on_sort_criteria_change.call(new_criteria);
                        }
                    },
                    XIcon { class: "w-3 h-3" }
                }
            }
        }
    }
}

/// An artist with their first non-compilation album cover
struct ArtistListItem {
    artist: Artist,
    cover_url: Option<String>,
}

/// A group of artists under the same letter heading
struct ArtistGroup {
    letter: String,
    artists: Vec<ArtistListItem>,
}

/// Derive unique artists from album data, picking the first non-compilation
/// album cover as the artist thumbnail.
fn derive_artist_list(
    albums: &[Album],
    artists_by_album: &HashMap<String, Vec<Artist>>,
) -> Vec<ArtistListItem> {
    // Invert the map: artist_id -> (Artist, first cover_url)
    let mut artist_map: HashMap<String, ArtistListItem> = HashMap::new();

    for album in albums {
        if album.is_compilation {
            continue;
        }

        if let Some(artists) = artists_by_album.get(&album.id) {
            for artist in artists {
                artist_map
                    .entry(artist.id.clone())
                    .and_modify(|item| {
                        // Keep the first cover we found
                        if item.cover_url.is_none() {
                            item.cover_url = album.cover_url.clone();
                        }
                    })
                    .or_insert_with(|| ArtistListItem {
                        artist: artist.clone(),
                        cover_url: album.cover_url.clone(),
                    });
            }
        }
    }

    let mut items: Vec<ArtistListItem> = artist_map.into_values().collect();
    items.sort_by(|a, b| {
        a.artist
            .name
            .to_lowercase()
            .cmp(&b.artist.name.to_lowercase())
    });
    items
}

/// Group a sorted list of artists by their first letter (# for non-alpha)
fn group_artists_by_letter(items: Vec<ArtistListItem>) -> Vec<ArtistGroup> {
    let mut groups: Vec<ArtistGroup> = Vec::new();

    for item in items {
        let first_char = item
            .artist
            .name
            .chars()
            .next()
            .unwrap_or('#')
            .to_uppercase()
            .next()
            .unwrap_or('#');
        let letter = if first_char.is_ascii_alphabetic() {
            first_char.to_string()
        } else {
            "#".to_string()
        };

        if let Some(last_group) = groups.last_mut() {
            if last_group.letter == letter {
                last_group.artists.push(item);
                continue;
            }
        }
        groups.push(ArtistGroup {
            letter,
            artists: vec![item],
        });
    }

    groups
}

/// Artists list view — groups artists alphabetically with letter headings
#[component]
fn ArtistListView(
    albums: Vec<Album>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    on_artist_click: EventHandler<String>,
) -> Element {
    let items = derive_artist_list(&albums, &artists_by_album);
    let groups = group_artists_by_letter(items);

    rsx! {
        div { class: "flex flex-col gap-6",
            for group in groups {
                div { key: "{group.letter}",
                    // Letter heading
                    div { class: "text-xs font-semibold text-gray-500 uppercase tracking-wider mb-2 border-b border-gray-800 pb-1",
                        "{group.letter}"
                    }

                    div { class: "flex flex-col",
                        for item in group.artists {
                            ArtistRow {
                                key: "{item.artist.id}",
                                artist: item.artist,
                                cover_url: item.cover_url,
                                on_click: on_artist_click,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Single artist row with round thumbnail + name
#[component]
fn ArtistRow(artist: Artist, cover_url: Option<String>, on_click: EventHandler<String>) -> Element {
    let artist_id = artist.id.clone();

    rsx! {
        button {
            class: "flex items-center gap-3 px-2 py-2 rounded-lg hover:bg-hover transition-colors text-left w-full",
            onclick: move |_| on_click.call(artist_id.clone()),

            // Round thumbnail
            div { class: "w-10 h-10 rounded-full overflow-clip flex-shrink-0 bg-gray-800 flex items-center justify-center",
                if let Some(url) = &cover_url {
                    img {
                        src: "{url}",
                        alt: "{artist.name}",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    UserIcon { class: "w-5 h-5 text-gray-500" }
                }
            }

            span { class: "text-sm text-white truncate", "{artist.name}" }
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
