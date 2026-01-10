//! Page container component

use dioxus::prelude::*;

/// Standard page container with consistent padding
#[component]
pub fn PageContainer(children: Element) -> Element {
    rsx! {
        div { class: "container mx-auto p-6", {children} }
    }
}
