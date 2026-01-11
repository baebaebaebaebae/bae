//! Global dialog view component
//!
//! Pure, props-based dialog component for confirmations and alerts.

use dioxus::prelude::*;

/// Global dialog view - modal confirmation dialog
#[component]
pub fn GlobalDialogView(
    is_open: bool,
    title: String,
    message: String,
    confirm_label: String,
    cancel_label: String,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    if !is_open {
        return rsx! {};
    }

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-[3000]",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
                onclick: move |evt| evt.stop_propagation(),

                h2 { class: "text-xl font-bold text-white mb-4", "{title}" }
                p { class: "text-gray-300 mb-6", "{message}" }

                div { class: "flex gap-3 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg",
                        onclick: move |_| on_cancel.call(()),
                        "{cancel_label}"
                    }
                    button {
                        class: "px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}
