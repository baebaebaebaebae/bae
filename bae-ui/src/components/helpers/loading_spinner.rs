//! Loading spinner component

use dioxus::prelude::*;

/// Loading spinner with optional message
#[component]
pub fn LoadingSpinner(
    /// Message to display next to spinner (default: "Loading...")
    #[props(default = "Loading...".to_string())]
    message: String,
) -> Element {
    rsx! {
        div { class: "flex justify-center items-center py-12",
            div { class: "animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500" }
            p { class: "ml-4 text-gray-300", "{message}" }
        }
    }
}
