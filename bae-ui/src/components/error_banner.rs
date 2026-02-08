//! Amber warning banner for displaying errors with a retry action.

use crate::components::icons::AlertTriangleIcon;
use crate::components::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

/// Amber warning banner with icon, heading, detail text, and a retry button.
///
/// Used for transient/retryable errors throughout the app (import failures,
/// API lookup failures, etc.).
#[component]
pub fn ErrorBanner(
    heading: String,
    detail: String,
    button_label: String,
    on_retry: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4",
            div { class: "flex items-start gap-3",
                AlertTriangleIcon { class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" }
                div { class: "flex-1",
                    p { class: "text-sm font-medium text-amber-200 mb-1", "{heading}" }
                    p { class: "text-sm text-gray-400 select-text break-words", "{detail}" }
                    div { class: "mt-3 flex gap-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_retry.call(()),
                            "{button_label}"
                        }
                    }
                }
            }
        }
    }
}
