//! Checkbox component

use dioxus::prelude::*;

/// A styled checkbox with label and optional tooltip
#[component]
pub fn Checkbox(
    checked: bool,
    onchange: EventHandler<bool>,
    label: &'static str,
    #[props(default)] tooltip: Option<&'static str>,
) -> Element {
    rsx! {
        label {
            class: "flex items-center gap-2 text-gray-400",
            title: tooltip.unwrap_or(""),
            input {
                r#type: "checkbox",
                checked,
                onchange: move |e| onchange.call(e.checked()),
            }
            "{label}"
            if tooltip.is_some() {
                span { class: "text-gray-600", "ⓘ" }
            }
        }
    }
}
