//! Match list view component

use super::match_item::MatchItemView;
use crate::display_types::MatchCandidate;
use dioxus::prelude::*;

/// Displays a list of match candidates with selection
#[component]
pub fn MatchListView(
    candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    if candidates.is_empty() {
        return rsx! {
            p { class: "text-gray-400 text-center",
                "No matches found. Try selecting a different folder or search manually."
            }
        };
    }

    rsx! {
        div {
            h3 { class: "text-sm font-medium text-gray-400 mb-3", "Select the correct release:" }
            div { class: "space-y-2",
                for (index , candidate) in candidates.iter().enumerate() {
                    MatchItemView {
                        key: "{index}",
                        candidate: candidate.clone(),
                        is_selected: selected_index == Some(index),
                        on_select: move |_| on_select.call(index),
                    }
                }
            }
        }
    }
}
