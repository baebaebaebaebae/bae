//! Link card component

use crate::Route;
use dioxus::prelude::*;

/// A card-style navigation link with title and description
#[component]
pub fn LinkCard(to: Route, title: &'static str, description: &'static str) -> Element {
    rsx! {
        Link {
            to,
            class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
            div { class: "font-medium", "{title}" }
            div { class: "text-sm text-gray-400", "{description}" }
        }
    }
}
