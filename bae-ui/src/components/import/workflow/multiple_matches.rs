//! Multiple DiscID matches view component

use super::match_list::MatchListView;
use crate::display_types::MatchCandidate;
use dioxus::prelude::*;

/// Displays multiple DiscID matches for user to pick from
#[component]
pub fn MultipleMatchesView(
    candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    if candidates.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "bg-gray-900 rounded-lg shadow p-6",
            h3 { class: "text-lg font-semibold text-white mb-4", "Multiple DiscID Matches" }
            p { class: "text-sm text-gray-400 mb-4",
                "This DiscID matches multiple releases. Select the correct one:"
            }
            div { class: "mt-4",
                MatchListView {
                    candidates,
                    selected_index,
                    on_select: move |index| on_select.call(index),
                }
            }
        }
    }
}
