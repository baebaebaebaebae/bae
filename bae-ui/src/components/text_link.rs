//! Clickable text link component — no underline by design

use dioxus::prelude::*;

/// Clickable text span with hover highlight. No underline.
#[component]
pub fn TextLink(
    #[props(default)] class: Option<String>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let extra = class.as_deref().unwrap_or("");

    rsx! {
        span {
            class: "hover:text-white transition-colors cursor-pointer {extra}",
            onclick: move |evt| onclick.call(evt),
            {children}
        }
    }
}
