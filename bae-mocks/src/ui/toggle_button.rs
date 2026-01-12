//! Toggle button component

use dioxus::prelude::*;

/// A pill-style toggle button with selected state
#[component]
pub fn ToggleButton(
    selected: bool,
    onclick: EventHandler<()>,
    label: &'static str,
    #[props(default)] tooltip: Option<&'static str>,
) -> Element {
    let class = if selected {
        "px-3 py-1.5 text-sm rounded bg-blue-600 text-white"
    } else {
        "px-3 py-1.5 text-sm rounded bg-gray-700 text-gray-300 hover:bg-gray-600"
    };

    rsx! {
        button {
            class,
            onclick: move |_| onclick.call(()),
            title: tooltip.unwrap_or(""),
            "{label}"
        }
    }
}
