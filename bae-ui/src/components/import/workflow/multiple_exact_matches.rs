//! Multiple DiscID matches view component

use super::match_results_panel::MatchResultsPanel;
use super::{DiscIdPill, DiscIdSource};
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::{IdentifyMode, MatchCandidate};
use crate::floating_ui::Placement;
use crate::stores::import::{CandidateState, ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

/// Displays multiple DiscID matches for user to pick from
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn MultipleExactMatchesView(
    state: ReadStore<ImportState>,
    on_select: EventHandler<usize>,
    on_confirm: EventHandler<MatchCandidate>,
    on_switch_to_manual_search: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    // Read via lenses — only subscribes to current_candidate_key + candidate_states
    let current_key = state.current_candidate_key().read().clone();
    let candidate_states = state.candidate_states().read().clone();
    let candidate_state = current_key.as_ref().and_then(|k| candidate_states.get(k));

    let (candidates, selected_index, disc_id, prefetch_state) = match candidate_state {
        Some(CandidateState::Identifying(is)) => {
            let disc_id = match &is.mode {
                IdentifyMode::MultipleExactMatches(id) => Some(id.clone()),
                _ => None,
            };
            (
                is.auto_matches.clone(),
                is.selected_match_index,
                disc_id,
                is.exact_match_prefetch.clone(),
            )
        }
        _ => (vec![], None, None, None),
    };

    if candidates.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "p-5 space-y-4",
            // Disc ID context and manual search option
            div { class: "flex justify-between items-center",
                if let Some(id) = disc_id {
                    p { class: "text-sm text-gray-300 flex items-center gap-2",
                        "Multiple exact matches for"
                        DiscIdPill {
                            disc_id: id,
                            source: DiscIdSource::Files,
                            tooltip_placement: Placement::Top,
                        }
                    }
                }

                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Small,
                    onclick: move |_| on_switch_to_manual_search.call(()),
                    "Search manually"
                }
            }

            MatchResultsPanel {
                candidates,
                selected_index,
                prefetch_state,
                on_select: move |index| on_select.call(index),
                on_confirm: move |candidate| on_confirm.call(candidate),
                on_retry_cover: move |_| {},
                on_view_in_library,
                confirm_button_text: "Select",
            }
        }
    }
}
