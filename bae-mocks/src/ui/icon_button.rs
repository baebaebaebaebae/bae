//! Icon button component

use dioxus::prelude::*;

/// A minimal button for icon-only actions
#[component]
pub fn IconButton(onclick: EventHandler<()>, children: Element) -> Element {
    rsx! {
        button {
            class: "text-gray-400 hover:text-white hover:bg-gray-700 rounded p-1",
            onclick: move |_| onclick.call(()),
            {children}
        }
    }
}
