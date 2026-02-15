//! Match item view component

use crate::components::helpers::Tooltip;
use crate::components::icons::{
    ChevronDownIcon, ChevronRightIcon, ImageIcon, LoaderIcon, RefreshIcon,
};
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::{CandidateTrack, MatchCandidate};
use crate::floating_ui::Placement;
use crate::stores::import::PrefetchState;
use dioxus::prelude::*;

/// Displays a single match candidate
#[component]
pub fn MatchItemView(
    candidate: MatchCandidate,
    is_selected: bool,
    prefetch_state: Option<PrefetchState>,
    confirm_pending: bool,
    on_select: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_retry_cover: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
    confirm_button_text: &'static str,
) -> Element {
    let mut expanded = use_signal(|| false);

    // Reset expanded when item is deselected
    use_effect(move || {
        if !is_selected {
            expanded.set(false);
        }
    });

    let is_duplicate = candidate.existing_album_id.is_some();

    let border_class = if is_duplicate {
        "border-transparent bg-amber-900/15 opacity-75"
    } else if is_selected {
        "border-transparent bg-blue-900/30 ring-1 ring-blue-500"
    } else {
        "border-transparent bg-gray-800/50 hover:bg-gray-800/70"
    };

    let radio_border = if is_duplicate {
        "border-gray-600"
    } else if is_selected {
        "border-blue-500"
    } else {
        "border-gray-500"
    };

    let cursor_class = if is_duplicate {
        "cursor-default"
    } else {
        "cursor-pointer"
    };

    let format_text = candidate.format.as_ref().map(|f| format!("Format: {}", f));
    let country_text = candidate
        .country
        .as_ref()
        .map(|c| format!("Country: {}", c));
    let label_text = candidate.label.as_ref().map(|l| format!("Label: {}", l));
    let catalog_text = candidate
        .catalog_number
        .as_ref()
        .map(|c| format!("Catalog: {}", c));

    // Determine button state from prefetch
    let is_mismatch = matches!(
        prefetch_state,
        Some(PrefetchState::TrackCountMismatch { .. }) | Some(PrefetchState::FetchFailed(_))
    );
    let button_disabled = is_mismatch;

    rsx! {
        div {
            class: "border rounded-lg px-3 py-2 transition-colors {border_class} {cursor_class}",
            onclick: move |_| {
                if !is_duplicate {
                    on_select.call(());
                }
            },

            div { class: "flex items-center gap-3",
                // Radio indicator (dimmed for duplicates)
                if !is_duplicate {
                    div { class: "w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center {radio_border}",
                        if is_selected {
                            div { class: "w-2 h-2 rounded-full bg-blue-500" }
                        }
                    }
                }

                // Cover art: image / placeholder / retry
                div { class: "w-10 h-10 flex-shrink-0 bg-gray-700 rounded overflow-clip",
                    if let Some(ref cover_url) = candidate.cover_url {
                        img {
                            src: "{cover_url}",
                            alt: "",
                            class: "w-full h-full object-cover text-transparent",
                        }
                    } else if candidate.cover_fetch_failed {
                        Tooltip {
                            text: "Cover art failed to load. Click to retry.",
                            placement: Placement::Top,
                            nowrap: true,
                            div {
                                class: "w-full h-full flex items-center justify-center text-gray-500 hover:text-gray-300 cursor-pointer",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    on_retry_cover.call(());
                                },
                                RefreshIcon { class: "w-4 h-4" }
                            }
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center text-gray-500",
                            ImageIcon { class: "w-5 h-5" }
                        }
                    }
                }

                // Info
                div { class: "flex-1 min-w-0",
                    h4 { class: "text-sm font-medium text-white truncate", "{candidate.title}" }
                    div { class: "text-xs text-gray-400 flex flex-wrap gap-x-3",
                        if let Some(ref year) = candidate.year {
                            span { "{year}" }
                        }
                        if let Some(ref fmt) = format_text {
                            span { "{fmt}" }
                        }
                        if let Some(ref country) = country_text {
                            span { "{country}" }
                        }
                    }
                    if label_text.is_some() || catalog_text.is_some() {
                        div { class: "text-xs text-gray-500 flex flex-wrap gap-x-3",
                            if let Some(ref label) = label_text {
                                span { "{label}" }
                            }
                            if let Some(ref catalog) = catalog_text {
                                span { "{catalog}" }
                            }
                        }
                    }

                    // Error message for track count mismatch or fetch failure
                    if is_selected {
                        match &prefetch_state {
                            Some(PrefetchState::TrackCountMismatch { release_tracks, local_files }) => {
                                rsx! {
                                    p { class: "text-xs text-red-400 mt-1",
                                        "Release has {release_tracks} tracks but folder has {local_files} audio files"
                                    }
                                }
                            }
                            Some(PrefetchState::FetchFailed(err)) => rsx! {
                                p { class: "text-xs text-red-400 mt-1", "{err}" }
                            },
                            _ => rsx! {},
                        }
                    }
                }

                // Actions: "In library" badge + view link, or confirm button
                if is_duplicate {
                    div {
                        class: "flex-shrink-0 flex items-center gap-2",
                        onclick: move |e| e.stop_propagation(),
                        span { class: "text-[0.6875rem] font-medium text-amber-400/80 bg-amber-500/15 px-1.5 py-0.5 rounded",
                            "In library"
                        }
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Small,
                            onclick: {
                                let album_id = candidate.existing_album_id.clone().unwrap_or_default();
                                move |_| on_view_in_library.call(album_id.clone())
                            },
                            "View"
                        }
                    }
                } else if is_selected {
                    div {
                        class: "flex-shrink-0",
                        onclick: move |e| e.stop_propagation(),
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Small,
                            disabled: button_disabled,
                            loading: confirm_pending,
                            onclick: move |_| on_confirm.call(()),
                            "{confirm_button_text}"
                        }
                    }
                }
            }

            // Expand/collapse track listing toggle (only when selected, not on error states)
            if is_selected && !is_mismatch {
                match &prefetch_state {
                    Some(PrefetchState::Valid { tracks }) if !tracks.is_empty() => {
                        let track_count = tracks.len();
                        let tracks = tracks.clone();
                        rsx! {
                            button {
                                class: "flex items-center gap-1 text-xs text-gray-400 hover:text-gray-300 transition-colors ml-7 mt-1",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    expanded.toggle();
                                },
                                if *expanded.read() {
                                    ChevronDownIcon { class: "w-3 h-3" }
                                } else {
                                    ChevronRightIcon { class: "w-3 h-3" }
                                }
                                "{track_count} tracks"
                            }
                            if *expanded.read() {
                                TrackListingCompact { tracks }
                            }
                        }
                    }
                    Some(PrefetchState::Fetching) | None => rsx! {
                        button {
                            class: "flex items-center gap-1 text-xs text-gray-400 hover:text-gray-300 transition-colors ml-7 mt-1",
                            onclick: move |e| {
                                e.stop_propagation();
                                expanded.toggle();
                            },
                            if *expanded.read() {
                                ChevronDownIcon { class: "w-3 h-3" }
                            } else {
                                ChevronRightIcon { class: "w-3 h-3" }
                            }
                            "Tracks"
                        }
                        if *expanded.read() {
                            p { class: "text-xs text-gray-500 flex items-center gap-1.5 mt-1.5 ml-11",
                                LoaderIcon { class: "w-3 h-3 animate-spin" }
                                "Loading tracks..."
                            }
                        }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}

/// Compact track listing for display in match items and confirmation views
#[component]
pub fn TrackListingCompact(tracks: Vec<CandidateTrack>) -> Element {
    rsx! {
        div { class: "mt-2 ml-7 border-t border-gray-700/50 pt-2",
            div { class: "grid grid-cols-[auto_1fr_auto] gap-x-3 gap-y-0.5 text-xs text-gray-400",
                for track in tracks.iter() {
                    span { class: "text-gray-500 tabular-nums text-right", "{track.position}" }
                    span { class: "truncate", "{track.title}" }
                    if let Some(ref dur) = track.duration {
                        span { class: "text-gray-500 tabular-nums", "{dur}" }
                    } else {
                        span {}
                    }
                }
            }
        }
    }
}
