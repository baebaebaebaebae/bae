//! Loading indicator component for import workflows

use crate::components::LoaderIcon;
use dioxus::prelude::*;

/// A centered loading indicator with spinner and message.
/// Used for disc ID lookup and manual search loading states.
#[component]
pub fn LoadingIndicator(message: String) -> Element {
    rsx! {
        p { class: "text-sm text-gray-300 flex items-center justify-center gap-2",
            LoaderIcon { class: "w-5 h-5 animate-spin" }
            "{message}"
        }
    }
}
