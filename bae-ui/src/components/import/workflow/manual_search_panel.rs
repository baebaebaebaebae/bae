//! Manual search panel view component

use super::match_list::MatchListView;
use super::search_source_selector::SearchSourceSelectorView;
use crate::display_types::{MatchCandidate, SearchSource, SearchTab};
use dioxus::prelude::*;

/// Manual search panel with tabs for General/Catalog#/Barcode search
///
/// This component accepts reactive props (ReadSignal) for fine-grained reactivity.
/// Only the specific fields that change will trigger re-renders.
#[component]
pub fn ManualSearchPanelView(
    // Search source selection
    search_source: ReadSignal<SearchSource>,
    on_search_source_change: EventHandler<SearchSource>,
    // Active tab
    active_tab: ReadSignal<SearchTab>,
    on_tab_change: EventHandler<SearchTab>,
    // General search fields
    search_artist: ReadSignal<String>,
    on_artist_change: EventHandler<String>,
    search_album: ReadSignal<String>,
    on_album_change: EventHandler<String>,
    search_year: ReadSignal<String>,
    on_year_change: EventHandler<String>,
    search_label: ReadSignal<String>,
    on_label_change: EventHandler<String>,
    // Catalog number search
    search_catalog_number: ReadSignal<String>,
    on_catalog_number_change: EventHandler<String>,
    // Barcode search
    search_barcode: ReadSignal<String>,
    on_barcode_change: EventHandler<String>,
    // Search tokens (suggestions from folder name)
    search_tokens: ReadSignal<Vec<String>>,
    // Search state
    is_searching: ReadSignal<bool>,
    error_message: ReadSignal<Option<String>>,
    has_searched: ReadSignal<bool>,
    // Results
    match_candidates: ReadSignal<Vec<MatchCandidate>>,
    selected_index: ReadSignal<Option<usize>>,
    on_match_select: EventHandler<usize>,
    // Actions
    on_search: EventHandler<()>,
    on_confirm: EventHandler<MatchCandidate>,
) -> Element {
    // Read values for use in the template
    let source = search_source();
    let tab = active_tab();
    let artist = search_artist();
    let album = search_album();
    let year = search_year();
    let label = search_label();
    let catalog = search_catalog_number();
    let barcode = search_barcode();
    let tokens = search_tokens();
    let searching = is_searching();
    let error = error_message();
    let searched = has_searched();
    let candidates = match_candidates();
    let selected = selected_index();
    rsx! {
        div { class: "space-y-4",
            // Header with search source selector
            div { class: "flex justify-between items-center",
                h3 { class: "text-sm font-medium text-gray-200", "Search for Release" }
                SearchSourceSelectorView {
                    selected_source: source,
                    on_select: on_search_source_change,
                }
            }

            // Tab bar
            div { class: "flex gap-1",
                button {
                    class: format!(
                        "px-3 py-1.5 text-sm rounded-lg transition-all duration-150 {}",
                        if tab == SearchTab::General {
                            "bg-surface-raised text-white"
                        } else {
                            "text-gray-300 hover:text-white hover:bg-surface-raised/50"
                        },
                    ),
                    onclick: move |_| on_tab_change.call(SearchTab::General),
                    "General"
                }
                button {
                    class: format!(
                        "px-3 py-1.5 text-sm rounded-lg transition-all duration-150 {}",
                        if tab == SearchTab::CatalogNumber {
                            "bg-surface-raised text-white"
                        } else {
                            "text-gray-300 hover:text-white hover:bg-surface-raised/50"
                        },
                    ),
                    onclick: move |_| on_tab_change.call(SearchTab::CatalogNumber),
                    "Catalog #"
                }
                button {
                    class: format!(
                        "px-3 py-1.5 text-sm rounded-lg transition-all duration-150 {}",
                        if tab == SearchTab::Barcode {
                            "bg-surface-raised text-white"
                        } else {
                            "text-gray-300 hover:text-white hover:bg-surface-raised/50"
                        },
                    ),
                    onclick: move |_| on_tab_change.call(SearchTab::Barcode),
                    "Barcode"
                }
            }

            // Search token pills (suggestions)
            if !tokens.is_empty() {
                div { class: "flex flex-wrap gap-1.5",
                    for token in tokens.iter() {
                        span { class: "px-2.5 py-1 text-xs bg-surface-raised text-gray-300 rounded-full",
                            "{token}"
                        }
                    }
                }
            }

            // Search form based on active tab
            div { class: "space-y-3",
                match tab {
                    SearchTab::General => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Artist" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{artist}",
                                    oninput: move |e| on_artist_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Album" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{album}",
                                    oninput: move |e| on_album_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Year" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{year}",
                                    oninput: move |e| on_year_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Label" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{label}",
                                    oninput: move |e| on_label_change.call(e.value()),
                                }
                            }
                        }
                        div { class: "flex justify-end pt-1",
                            button {
                                class: "px-4 py-1.5 bg-surface-raised text-sm text-white rounded-lg hover:bg-hover disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-150",
                                disabled: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::CatalogNumber => rsx! {
                        div { class: "flex gap-2",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "Enter catalog number...",
                                    value: "{catalog}",
                                    oninput: move |e| on_catalog_number_change.call(e.value()),
                                }
                            }
                            button {
                                class: "px-4 py-1.5 bg-surface-raised text-sm text-white rounded-lg hover:bg-hover disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-150",
                                disabled: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::Barcode => rsx! {
                        div { class: "flex gap-2",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "Enter barcode...",
                                    value: "{barcode}",
                                    oninput: move |e| on_barcode_change.call(e.value()),
                                }
                            }
                            button {
                                class: "px-4 py-1.5 bg-surface-raised text-sm text-white rounded-lg hover:bg-hover disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-150",
                                disabled: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                }
            }

            // Error message
            if let Some(ref err) = error {
                div { class: "bg-red-500/15 rounded-lg p-3",
                    p { class: "text-sm text-red-300 select-text", "Error: {err}" }
                }
            }

            // Results
            if searching {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "Searching..." }
                }
            } else if candidates.is_empty() && searched {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "No results found" }
                }
            } else if !candidates.is_empty() {
                div { class: "space-y-4 mt-4",
                    MatchListView {
                        candidates: candidates.clone(),
                        selected_index: selected,
                        on_select: move |index| on_match_select.call(index),
                    }

                    if selected.is_some() {
                        div { class: "flex justify-end",
                            button {
                                class: "px-4 py-1.5 bg-green-500/25 text-sm text-green-300 rounded-lg hover:bg-green-500/35 transition-all duration-150",
                                onclick: move |_| {
                                    if let Some(index) = selected {
                                        if let Some(candidate) = candidates.get(index) {
                                            on_confirm.call(candidate.clone());
                                        }
                                    }
                                },
                                "Confirm Selection"
                            }
                        }
                    }
                }
            }
        }
    }
}
