//! Export error toast notification

use dioxus::prelude::*;

#[component]
pub fn ExportErrorToast(error: String, on_dismiss: EventHandler<()>) -> Element {
    rsx! {
        div { class: "fixed bottom-4 right-4 bg-red-600 text-white px-4 py-3 rounded-lg shadow-lg flex items-center gap-3 z-50",
            div { class: "flex-1",
                p { class: "font-medium", "Export Failed" }
                p { class: "text-sm text-red-100", "{error}" }
            }
            button {
                class: "text-red-200 hover:text-white",
                onclick: move |_| on_dismiss.call(()),
                "✕"
            }
        }
    }
}
