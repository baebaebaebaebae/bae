//! Match item view component

use crate::components::icons::{ImageIcon, RefreshIcon};
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::MatchCandidate;
use crate::stores::import::PrefetchState;
use dioxus::prelude::*;

/// Displays a single match candidate
#[component]
pub fn MatchItemView(
    candidate: MatchCandidate,
    is_selected: bool,
    prefetch_state: Option<PrefetchState>,
    on_select: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_retry_cover: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
    confirm_button_text: &'static str,
) -> Element {
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
    let is_fetching = matches!(prefetch_state, Some(PrefetchState::Fetching));
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
                        div {
                            class: "w-full h-full flex items-center justify-center text-gray-500 hover:text-gray-300 cursor-pointer",
                            title: "Cover art failed to load. Click to retry.",
                            onclick: move |e| {
                                e.stop_propagation();
                                on_retry_cover.call(());
                            },
                            RefreshIcon { class: "w-4 h-4" }
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
                            loading: is_fetching,
                            onclick: move |_| on_confirm.call(()),
                            "{confirm_button_text}"
                        }
                    }
                }
            }
        }
    }
}
