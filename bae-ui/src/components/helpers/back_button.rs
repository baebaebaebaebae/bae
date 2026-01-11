//! Back button component

use dioxus::prelude::*;

/// Back button with customizable text and callback
#[component]
pub fn BackButton(
    /// Text to display (default: "Back to Library")
    #[props(default = "Back to Library".to_string())]
    text: String,
    /// Callback when button is clicked
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "mb-6",
            button {
                class: "inline-flex items-center text-gray-400 hover:text-white transition-colors",
                "data-testid": "back-button",
                onclick: move |_| on_click.call(()),
                svg {
                    class: "w-5 h-5 mr-2",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M15 19l-7-7 7-7",
                    }
                }
                "{text}"
            }
        }
    }
}
