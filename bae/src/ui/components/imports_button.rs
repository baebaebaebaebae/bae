use super::active_imports_context::use_active_imports;
use dioxus::prelude::*;

/// Button in title bar that shows active imports count and toggles dropdown
#[component]
pub fn ImportsButton(mut is_open: Signal<bool>) -> Element {
    let active_imports = use_active_imports();
    let count = active_imports.imports.read().len();

    // Don't render if no active imports
    if count == 0 {
        return rsx! {};
    }

    let has_preparing = active_imports.imports.read().iter().any(|i| {
        i.status == crate::db::ImportOperationStatus::Preparing
            || i.status == crate::db::ImportOperationStatus::Importing
    });

    rsx! {
        button {
            class: "relative px-3 py-1.5 text-sm font-medium text-gray-300 hover:text-white hover:bg-gray-700/50 rounded-md transition-colors flex items-center gap-2",
            onclick: move |_| {
                let current = *is_open.read();
                is_open.set(!current);
            },

            // Spinner when actively importing
            if has_preparing {
                div { class: "animate-spin h-3.5 w-3.5 border-2 border-gray-400 border-t-transparent rounded-full" }
            } else {
                // Download icon when complete
                svg {
                    class: "h-4 w-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                    }
                }
            }

            "Imports"

            // Count badge
            span {
                class: "absolute -top-1 -right-1 bg-indigo-500 text-white text-xs font-bold rounded-full h-5 min-w-5 flex items-center justify-center px-1",
                "{count}"
            }
        }
    }
}
