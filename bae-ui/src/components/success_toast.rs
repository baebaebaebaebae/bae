//! Generic success toast notification

use crate::components::icons::XIcon;
use crate::components::ChromelessButton;
use dioxus::prelude::*;

/// A dismissible success toast notification
#[component]
pub fn SuccessToast(
    /// The success message to display
    message: String,
    /// Called when the user dismisses the toast
    on_dismiss: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "fixed bottom-20 right-4 bg-green-600 text-white px-6 py-4 rounded-lg shadow-lg z-50 max-w-md",
            div { class: "flex items-center justify-between gap-4",
                div { class: "flex-1",
                    span { "{message}" }
                }
                ChromelessButton {
                    class: Some("text-white hover:text-gray-200".to_string()),
                    aria_label: Some("Dismiss".to_string()),
                    onclick: move |_| on_dismiss.call(()),
                    XIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}
