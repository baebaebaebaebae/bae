//! Detecting metadata view

use dioxus::prelude::*;

/// Loading indicator while detecting metadata from files
#[component]
pub fn DetectingMetadataView(message: String, on_skip: EventHandler<()>) -> Element {
    rsx! {
        div { class: "text-center space-y-2",
            p { class: "text-sm text-gray-400", {message} }
            button {
                class: "px-4 py-2 text-sm font-medium text-gray-200 bg-white/5 hover:bg-white/10 rounded-md transition-colors",
                onclick: move |_| on_skip.call(()),
                "Skip and search manually"
            }
        }
    }
}
