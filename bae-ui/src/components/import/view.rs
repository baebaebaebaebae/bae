//! Import view - main import page layout
//!
//! Two-panel layout with resizable left sidebar containing:
//! - Import header and source selector
//! - Workflow-specific sidebar content (releases, CD drives, etc.)

use super::source_selector::{ImportSource, ImportSourceSelectorView};
use super::workflow::{DEFAULT_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH};
use crate::components::{PanelPosition, ResizablePanel, ResizeDirection};
use dioxus::prelude::*;

/// Import page view with resizable sidebar
#[component]
pub fn ImportView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
    /// Workflow-specific sidebar content (release list, CD drives, etc.)
    sidebar: Element,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex flex-grow min-h-0",
            // Left panel - resizable sidebar with header
            ResizablePanel {
                storage_key: "import-sidebar-width",
                min_size: MIN_SIDEBAR_WIDTH,
                max_size: MAX_SIDEBAR_WIDTH,
                default_size: DEFAULT_SIDEBAR_WIDTH,
                grabber_span_ratio: 0.95,
                direction: ResizeDirection::Horizontal,
                position: PanelPosition::Relative,
                div { class: "flex flex-col h-full",
                    // Header with title and source selector
                    div { class: "pt-6 px-5 pb-4 flex items-center gap-4",
                        h1 { class: "text-2xl font-bold text-white", "Import" }
                        ImportSourceSelectorView { selected_source, on_source_select }
                    }
                    // Workflow-specific sidebar content
                    div { class: "flex-1 min-h-0", {sidebar} }
                }
            }
            // Right panel - main content
            div { class: "flex-1 min-h-0 flex flex-col", {children} }
        }
    }
}
