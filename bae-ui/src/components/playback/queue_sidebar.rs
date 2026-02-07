//! Queue Sidebar view component
//!
//! ## Reactive State Pattern
//! Accepts `ReadStore<PlaybackUiState>` and reads fields via lenses.
//! Each section only re-renders when its specific data changes.

use crate::components::icons::{EllipsisIcon, ImageIcon, PauseIcon, PlayIcon, XIcon};
use crate::components::utils::format_duration;
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton};
use crate::components::{MenuDropdown, MenuItem, Placement};
use crate::display_types::QueueItem;
use crate::stores::playback::{PlaybackStatus, PlaybackUiState, PlaybackUiStateStoreExt};
use crate::stores::ui::{SidebarState, SidebarStateStoreExt};
use dioxus::prelude::*;

/// Shared state for queue sidebar visibility
#[derive(Clone)]
pub struct QueueSidebarState {
    pub is_open: Signal<bool>,
}

/// Queue sidebar view - accepts stores for granular reactivity
#[component]
pub fn QueueSidebarView(
    /// Sidebar UI state (for is_open)
    sidebar: ReadStore<SidebarState>,
    /// Playback state store
    playback: ReadStore<PlaybackUiState>,
    // Callbacks
    on_close: EventHandler<()>,
    on_clear: EventHandler<()>,
    on_remove: EventHandler<usize>,
    on_track_click: EventHandler<String>,
    on_play_index: EventHandler<usize>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
) -> Element {
    // Read is_open via lens - only this check re-runs when visibility changes
    let is_open = *sidebar.is_open().read();

    if !is_open {
        return rsx! {};
    }

    rsx! {
        div { class: "w-80 flex-shrink-0 bg-gray-900 border-l border-gray-700 flex flex-col",
            // Header with controls
            div { class: "flex items-center justify-between px-4 py-3 border-b border-gray-700",
                h2 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide",
                    "Queue"
                }
                div { class: "flex items-center gap-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Small,
                        onclick: move |_| on_clear.call(()),
                        "Clear"
                    }
                    ChromelessButton {
                        class: Some("text-gray-400 hover:text-white transition-colors".to_string()),
                        aria_label: Some("Close queue".to_string()),
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }
            }

            div { class: "flex-1 overflow-y-auto",
                NowPlayingSection {
                    playback,
                    on_track_click,
                    on_pause,
                    on_resume,
                }

                UpNextSection {
                    playback,
                    on_track_click,
                    on_remove,
                    on_play_index,
                }
            }
        }
    }
}

/// Now playing section - reads current_track and status
#[component]
fn NowPlayingSection(
    playback: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
) -> Element {
    let current_track = playback.current_track().read().clone();
    let status = *playback.status().read();

    rsx! {
        div {
            div { class: "px-4 pt-4 pb-2",
                h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                    "Now playing"
                }
            }
            if let Some(item) = current_track {
                NowPlayingItem {
                    item,
                    status,
                    on_track_click,
                    on_pause,
                    on_resume,
                }
            } else {
                div { class: "px-4 py-3 text-gray-500 text-sm", "Nothing playing" }
            }
        }
    }
}

/// Now playing track row - special styling for the currently playing track
#[component]
fn NowPlayingItem(
    item: QueueItem,
    status: PlaybackStatus,
    on_track_click: EventHandler<String>,
    on_pause: EventHandler<()>,
    on_resume: EventHandler<()>,
) -> Element {
    let is_playing = status == PlaybackStatus::Playing;
    let is_paused = status == PlaybackStatus::Paused;

    rsx! {
        div { class: "flex items-center gap-3 py-2 px-3 mx-2 rounded-lg bg-accent/10 hover:bg-accent/15 transition-colors group",
            // Play/pause button
            if is_playing {
                ChromelessButton {
                    class: Some(
                        "w-6 h-6 rounded-full border border-blue-400 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10"
                            .to_string(),
                    ),
                    aria_label: Some("Pause".to_string()),
                    onclick: move |_| on_pause.call(()),
                    PauseIcon { class: "w-3 h-3" }
                }
            } else if is_paused {
                ChromelessButton {
                    class: Some(
                        "w-6 h-6 rounded-full border border-blue-400 flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10 transition-colors"
                            .to_string(),
                    ),
                    aria_label: Some("Resume".to_string()),
                    onclick: move |_| on_resume.call(()),
                    PlayIcon { class: "w-3 h-3" }
                }
            } else {
                div { class: "w-6" }
            }

            // Album cover
            div { class: "w-10 h-10 flex-shrink-0 bg-gray-700 rounded overflow-clip",
                if let Some(ref url) = item.cover_url {
                    img {
                        src: "{url}",
                        alt: "Album cover",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-gray-500",
                        ImageIcon { class: "w-5 h-5" }
                    }
                }
            }

            // Track info
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    h3 { class: "font-medium text-accent-soft truncate flex-1 text-left",
                        "{item.track.title}"
                    }
                    span { class: "text-sm text-gray-400 font-mono flex-shrink-0",
                        if let Some(duration_ms) = item.track.duration_ms {
                            {format_duration(duration_ms)}
                        } else {
                            "—:—"
                        }
                    }
                }
                div { class: "text-sm text-gray-400 truncate", "{item.album_title}" }
            }
        }
    }
}

/// Up next section - reads only queue_items
#[component]
fn UpNextSection(
    playback: ReadStore<PlaybackUiState>,
    on_track_click: EventHandler<String>,
    on_remove: EventHandler<usize>,
    on_play_index: EventHandler<usize>,
) -> Element {
    let queue = playback.queue_items().read().clone();

    rsx! {
        div {
            div { class: "px-4 pt-4 pb-2",
                h3 { class: "text-sm font-semibold text-gray-400 uppercase tracking-wide",
                    "Up next"
                }
            }
            if !queue.is_empty() {
                for (index , item) in queue.iter().enumerate() {
                    QueueItemView {
                        key: "{item.track.id}",
                        item: item.clone(),
                        index,
                        on_track_click,
                        on_remove,
                        on_play_index,
                    }
                }
            } else {
                div { class: "px-4 py-3 text-gray-500 text-sm", "No tracks queued" }
            }
        }
    }
}

/// Queue item row for "up next" tracks
#[component]
fn QueueItemView(
    item: QueueItem,
    index: usize,
    on_track_click: EventHandler<String>,
    on_remove: EventHandler<usize>,
    on_play_index: EventHandler<usize>,
) -> Element {
    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let anchor_id = format!("queue-menu-{}", item.track.id);

    let menu_is_open = is_open();

    rsx! {
        div { class: "flex items-center gap-3 py-2 px-3 mx-2 rounded-lg hover:bg-hover transition-colors group",
            // Play button (appears on hover)
            ChromelessButton {
                class: Some(
                    "w-6 h-6 rounded-full border border-blue-400 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-blue-400 hover:text-blue-300 hover:bg-blue-400/10"
                        .to_string(),
                ),
                aria_label: Some("Play".to_string()),
                onclick: move |_| on_play_index.call(index),
                PlayIcon { class: "w-3 h-3" }
            }

            // Album cover
            div { class: "w-10 h-10 flex-shrink-0 bg-gray-700 rounded overflow-clip",
                if let Some(ref url) = item.cover_url {
                    img {
                        src: "{url}",
                        alt: "Album cover",
                        class: "w-full h-full object-cover",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center text-gray-500",
                        ImageIcon { class: "w-5 h-5" }
                    }
                }
            }

            // Track info
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    h3 { class: "font-medium text-white group-hover:text-accent-soft transition-colors truncate flex-1 text-left",
                        "{item.track.title}"
                    }
                    span { class: "text-sm text-gray-400 font-mono flex-shrink-0",
                        if let Some(duration_ms) = item.track.duration_ms {
                            {format_duration(duration_ms)}
                        } else {
                            "—:—"
                        }
                    }
                }
                div { class: "text-sm text-gray-400 truncate", "{item.album_title}" }
            }

            // Context menu
            ChromelessButton {
                id: Some(anchor_id.clone()),
                class: Some(
                    if menu_is_open {
                        "px-2 py-1 rounded-md text-gray-400 hover:text-white hover:bg-hover transition-all"
                            .to_string()
                    } else {
                        "px-2 py-1 rounded-md text-gray-400 hover:text-white hover:bg-hover opacity-0 group-hover:opacity-100 transition-all"
                            .to_string()
                    },
                ),
                aria_label: Some("Track menu".to_string()),
                onclick: move |_| show_menu.set(!show_menu()),
                EllipsisIcon { class: "w-4 h-4" }
            }

            MenuDropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_menu.set(false),
                placement: Placement::BottomEnd,

                MenuItem {
                    onclick: move |_| {
                        show_menu.set(false);
                        on_remove.call(index);
                    },
                    "Remove from Queue"
                }
            }
        }
    }
}
