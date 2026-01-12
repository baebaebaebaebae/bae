//! Shared header for mock pages

use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn MockHeader(title: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-3 mb-3",
            Link {
                to: Route::MockIndex {},
                class: "text-gray-400 hover:text-white",
                "←"
            }
            h1 { class: "text-lg font-semibold text-white", "{title}" }
        }
    }
}
