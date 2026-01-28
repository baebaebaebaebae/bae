//! Multiple DiscID matches view component

use super::match_results_panel::MatchResultsPanel;
use super::{DiscIdPill, DiscIdSource};
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::{IdentifyMode, MatchCandidate};
use crate::floating_ui::Placement;
use crate::stores::import::ImportState;
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
) -> Element {
    // Read state at leaf - these are computed values
    let st = state.read();
    let candidates = st.get_exact_match_candidates();
    let selected_index = st.get_selected_match_index();
    // Extract disc_id from the mode - it's carried in MultipleExactMatches(disc_id)
    let disc_id = match st.get_identify_mode() {
        IdentifyMode::MultipleExactMatches(id) => Some(id),
        _ => None,
    };
    drop(st);

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
                on_select: move |index| on_select.call(index),
                on_confirm: move |candidate| on_confirm.call(candidate),
                confirm_button_text: "Continue",
            }
        }
    }
}
