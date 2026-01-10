//! Error display component

use dioxus::prelude::*;

/// Generic error display box
#[component]
pub fn ErrorDisplay(message: String) -> Element {
    rsx! {
        div { class: "bg-red-900 border border-red-700 text-red-100 px-4 py-3 rounded mb-4",
            p { "{message}" }
        }
    }
}
