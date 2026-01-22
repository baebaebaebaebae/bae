//! Import view - main import page layout

use super::source_selector::{ImportSource, ImportSourceSelectorView};
use dioxus::prelude::*;

/// Import page view
#[component]
pub fn ImportView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "h-full flex flex-col flex-grow",
            div { class: "pt-6 px-7 pb-2 flex items-center gap-6",
                h1 { class: "text-3xl font-bold text-white", "Import" }
                ImportSourceSelectorView { selected_source, on_source_select }
            }
            div { class: "flex-1 min-h-0 flex flex-col px-3", {children} }
        }
    }
}
