//! Match results panel component

use super::match_item::MatchItemView;
use crate::display_types::MatchCandidate;
use dioxus::prelude::*;

/// A panel displaying match results with selection and confirm button
#[component]
pub fn MatchResultsPanel(
    candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
    on_confirm: EventHandler<MatchCandidate>,
    confirm_button_text: &'static str,
) -> Element {
    if candidates.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "bg-gray-800/20 rounded-lg p-4",
            div { class: "space-y-2",
                for (index , candidate) in candidates.iter().enumerate() {
                    MatchItemView {
                        key: "{index}",
                        candidate: candidate.clone(),
                        is_selected: selected_index == Some(index),
                        on_select: move |_| on_select.call(index),
                        on_confirm: {
                            let candidate = candidate.clone();
                            move |_| on_confirm.call(candidate.clone())
                        },
                        confirm_button_text,
                    }
                }
            }
        }
    }
}
