//! Detecting metadata view

use crate::components::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

/// Loading indicator while detecting metadata from files
#[component]
pub fn DetectingMetadataView(message: String, on_skip: EventHandler<()>) -> Element {
    rsx! {
        div { class: "text-center space-y-2",
            p { class: "text-sm text-gray-400", {message} }
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Medium,
                onclick: move |_| on_skip.call(()),
                "Skip and search manually"
            }
        }
    }
}
