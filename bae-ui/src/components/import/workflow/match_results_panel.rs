//! Match results panel component

use super::match_list::MatchListView;
use crate::components::{Button, ButtonSize, ButtonVariant};
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
        div { class: "bg-gray-800/20 rounded-lg p-4 space-y-4",
            MatchListView {
                candidates: candidates.clone(),
                selected_index,
                on_select: move |index| on_select.call(index),
            }

            if let Some(index) = selected_index {
                if let Some(candidate) = candidates.get(index) {
                    div { class: "flex justify-end",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Medium,
                            onclick: {
                                let candidate = candidate.clone();
                                move |_| on_confirm.call(candidate.clone())
                            },
                            "{confirm_button_text}"
                        }
                    }
                }
            }
        }
    }
}
