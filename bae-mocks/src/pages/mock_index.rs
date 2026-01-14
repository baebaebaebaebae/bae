//! Mock pages with URL state persistence

use crate::mocks::framework::MockPage;
use crate::mocks::{AlbumDetailMock, FolderImportMock, LibraryMock, TitleBarMock};
use crate::ui::LinkCard;
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "Component mocks" }
            div { class: "space-y-2",
                for page in MockPage::ALL {
                    LinkCard {
                        to: page.to_route(None),
                        title: page.label(),
                        description: page.description(),
                    }
                }
            }
        }
    }
}

// ============================================================================
// FolderImport page wrapper
// ============================================================================

#[component]
pub fn MockFolderImport(state: Option<String>) -> Element {
    rsx! {
        FolderImportMock { initial_state: state }
    }
}

// ============================================================================
// AlbumDetail page wrapper
// ============================================================================

#[component]
pub fn MockAlbumDetail(state: Option<String>) -> Element {
    rsx! {
        AlbumDetailMock { initial_state: state }
    }
}

// ============================================================================
// Library page wrapper
// ============================================================================

#[component]
pub fn MockLibrary(state: Option<String>) -> Element {
    rsx! {
        LibraryMock { initial_state: state }
    }
}

// ============================================================================
// TitleBar page wrapper
// ============================================================================

#[component]
pub fn MockTitleBar(state: Option<String>) -> Element {
    rsx! {
        TitleBarMock { initial_state: state }
    }
}
