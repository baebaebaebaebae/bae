//! Manual search panel view component

use super::match_results_panel::MatchResultsPanel;
use super::search_source_selector::SearchSourceSelectorView;
use super::{DiscIdPill, DiscIdSource, LoadingIndicator};
use crate::components::button::ButtonVariant;
use crate::components::segmented_control::{Segment, SegmentedControl};
use crate::components::{Button, ButtonSize, TextInput, TextInputSize};
use crate::display_types::{MatchCandidate, SearchSource, SearchTab};
use crate::floating_ui::Placement;
use crate::stores::import::ImportState;
use dioxus::prelude::*;

/// Manual search panel with tabs for General/Catalog#/Barcode search
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn ManualSearchPanelView(
    state: ReadStore<ImportState>,
    on_search_source_change: EventHandler<SearchSource>,
    on_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_cancel_search: EventHandler<()>,
    on_confirm: EventHandler<MatchCandidate>,
    on_retry_cover: EventHandler<usize>,
    on_switch_to_exact_matches: EventHandler<String>,
) -> Element {
    // Read state at this leaf component
    let st = state.read();
    let search_state = st.get_search_state();
    let disc_id_not_found = st.get_disc_id_not_found();
    let exact_matches = st.get_exact_match_candidates();
    let source_disc_id = st.get_source_disc_id();

    let source = search_state
        .as_ref()
        .map(|s| s.search_source)
        .unwrap_or(SearchSource::MusicBrainz);
    let tab = search_state
        .as_ref()
        .map(|s| s.search_tab)
        .unwrap_or(SearchTab::General);
    let artist = search_state
        .as_ref()
        .map(|s| s.search_artist.clone())
        .unwrap_or_default();
    let album = search_state
        .as_ref()
        .map(|s| s.search_album.clone())
        .unwrap_or_default();
    let catalog = search_state
        .as_ref()
        .map(|s| s.search_catalog_number.clone())
        .unwrap_or_default();
    let barcode = search_state
        .as_ref()
        .map(|s| s.search_barcode.clone())
        .unwrap_or_default();
    let searching = search_state
        .as_ref()
        .map(|s| s.is_searching)
        .unwrap_or(false);
    let tab_state = search_state.as_ref().map(|s| s.current_tab_state().clone());
    let error = tab_state.as_ref().and_then(|t| t.error_message.clone());
    let searched = tab_state.as_ref().map(|t| t.has_searched).unwrap_or(false);
    let candidates = tab_state
        .as_ref()
        .map(|t| t.search_results.clone())
        .unwrap_or_default();
    let selected = tab_state.as_ref().and_then(|t| t.selected_result_index);

    drop(st);

    rsx! {
        div { class: "flex-1 flex flex-col p-5 space-y-4",
            // Info banner if disc ID lookup found no results
            if let Some(disc_id) = disc_id_not_found {
                div { class: "bg-blue-500/15 rounded-lg p-3 flex items-center gap-2",
                    p { class: "text-sm text-blue-300",
                        "No releases found for Disc ID "
                        DiscIdPill {
                            disc_id,
                            source: DiscIdSource::Files,
                            tooltip_placement: Placement::Top,
                        }
                    }
                }
            }

            // Link back to exact matches if they exist
            if !exact_matches.is_empty() {
                if let Some(disc_id) = source_disc_id.clone() {
                    div { class: "bg-gray-600/20 rounded-lg p-3 flex items-center justify-between",
                        p { class: "text-sm text-gray-300 flex items-center gap-2",
                            "{exact_matches.len()} exact matches for"
                            DiscIdPill {
                                disc_id: disc_id.clone(),
                                source: DiscIdSource::Files,
                                tooltip_placement: Placement::Top,
                            }
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Small,
                            onclick: move |_| on_switch_to_exact_matches.call(disc_id.clone()),
                            "View"
                        }
                    }
                }
            }

            // Search controls panel
            div { class: "bg-gray-800/20 rounded-lg p-4 space-y-4",
                // Header row: tabs + source selector
                div { class: "flex items-center justify-between gap-4",
                    SegmentedControl {
                        segments: vec![
                            Segment::new("Title", "general"),
                            Segment::new("Catalog #", "catalog"),
                            Segment::new("Barcode", "barcode"),
                        ],
                        selected: match tab {
                            SearchTab::General => "general".to_string(),
                            SearchTab::CatalogNumber => "catalog".to_string(),
                            SearchTab::Barcode => "barcode".to_string(),
                        },
                        selected_variant: ButtonVariant::Primary,
                        on_select: move |value: &str| {
                            let tab = match value {
                                "catalog" => SearchTab::CatalogNumber,
                                "barcode" => SearchTab::Barcode,
                                _ => SearchTab::General,
                            };
                            on_tab_change.call(tab);
                        },
                    }

                    SearchSourceSelectorView {
                        selected_source: source,
                        on_select: on_search_source_change,
                    }
                }

                // Error message
                if let Some(ref err) = error {
                    div { class: "bg-red-500/15 rounded-lg p-3",
                        p { class: "text-sm text-red-300 select-text", "Error: {err}" }
                    }
                }

                // Search form based on active tab
                div {
                    onkeydown: move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter && !searching {
                            on_search.call(());
                        }
                    },
                    match tab {
                        SearchTab::General => rsx! {
                            div { class: "flex gap-3",
                                div { class: "flex-1",
                                    label { class: "block text-xs text-gray-400 mb-1.5", "Artist" }
                                    TextInput {
                                        value: artist,
                                        on_input: move |v| on_artist_change.call(v),
                                        size: TextInputSize::Medium,
                                        autofocus: true,
                                        disabled: searching,
                                    }
                                }
                                div { class: "flex-1",
                                    label { class: "block text-xs text-gray-400 mb-1.5", "Album" }
                                    TextInput {
                                        value: album,
                                        on_input: move |v| on_album_change.call(v),
                                        size: TextInputSize::Medium,
                                        disabled: searching,
                                    }
                                }
                                div { class: "flex items-end shrink-0",
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        size: ButtonSize::Medium,
                                        disabled: searching,
                                        loading: searching,
                                        onclick: move |_| on_search.call(()),
                                        "Search"
                                    }
                                }
                            }
                        },
                        SearchTab::CatalogNumber => rsx! {
                            div { class: "flex gap-3",
                                div { class: "flex-1",
                                    label { class: "block text-xs text-gray-400 mb-1.5", "Catalog Number" }
                                    TextInput {
                                        value: catalog,
                                        on_input: move |v| on_catalog_number_change.call(v),
                                        size: TextInputSize::Medium,
                                        placeholder: "e.g. WPCR-80001",
                                        autofocus: true,
                                        disabled: searching,
                                    }
                                }
                                div { class: "flex items-end shrink-0",
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        size: ButtonSize::Medium,
                                        disabled: searching,
                                        loading: searching,
                                        onclick: move |_| on_search.call(()),
                                        "Search"
                                    }
                                }
                            }
                        },
                        SearchTab::Barcode => rsx! {
                            div { class: "flex gap-3",
                                div { class: "flex-1",
                                    label { class: "block text-xs text-gray-400 mb-1.5", "Barcode" }
                                    TextInput {
                                        value: barcode,
                                        on_input: move |v| on_barcode_change.call(v),
                                        size: TextInputSize::Medium,
                                        placeholder: "e.g. 4943674251780",
                                        autofocus: true,
                                        disabled: searching,
                                    }
                                }
                                div { class: "flex items-end shrink-0",
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        size: ButtonSize::Medium,
                                        disabled: searching,
                                        loading: searching,
                                        onclick: move |_| on_search.call(()),
                                        "Search"
                                    }
                                }
                            }
                        },
                    }
                }
            }

            // Results
            if searching {
                div { class: "flex-1 flex flex-col items-center justify-center gap-4",
                    LoadingIndicator { message: format!("Searching {}...", source.display_name()) }
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        onclick: move |_| on_cancel_search.call(()),
                        "Cancel"
                    }
                }
            } else if candidates.is_empty() && searched && error.is_none() {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "No results found" }
                }
            } else if !candidates.is_empty() {
                MatchResultsPanel {
                    candidates,
                    selected_index: selected,
                    on_select: move |index| on_match_select.call(index),
                    on_confirm,
                    on_retry_cover,
                    confirm_button_text: "Select",
                }
            }
        }
    }
}
