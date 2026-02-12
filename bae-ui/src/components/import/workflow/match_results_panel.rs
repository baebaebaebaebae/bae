//! Match results panel component

use super::match_item::MatchItemView;
use crate::display_types::MatchCandidate;
use crate::stores::import::PrefetchState;
use dioxus::prelude::*;

/// A panel displaying match results with selection and confirm button
#[component]
pub fn MatchResultsPanel(
    candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    prefetch_state: Option<PrefetchState>,
    confirm_pending: bool,
    on_select: EventHandler<usize>,
    on_confirm: EventHandler<MatchCandidate>,
    on_retry_cover: EventHandler<usize>,
    on_view_in_library: EventHandler<String>,
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
                        prefetch_state: if selected_index == Some(index) { prefetch_state.clone() } else { None },
                        confirm_pending: if selected_index == Some(index) { confirm_pending } else { false },
                        on_select: move |_| on_select.call(index),
                        on_confirm: {
                            let candidate = candidate.clone();
                            move |_| on_confirm.call(candidate.clone())
                        },
                        on_retry_cover: move |_| on_retry_cover.call(index),
                        on_view_in_library,
                        confirm_button_text,
                    }
                }
            }
        }
    }
}
