//! Mock index page

use crate::mocks;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "Component Mocks" }
            div { class: "space-y-2",
                Link {
                    to: Route::MockFolderImport {},
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "FolderImportView" }
                    div { class: "text-sm text-gray-400", "Folder import workflow with all phases" }
                }
                Link {
                    to: Route::MockAlbumDetail {},
                    class: "block p-4 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors",
                    div { class: "font-medium", "AlbumDetailView" }
                    div { class: "text-sm text-gray-400", "Album detail page with tracks and controls" }
                }
            }
        }
    }
}

#[component]
pub fn MockFolderImport() -> Element {
    rsx! {
        mocks::FolderImportMock {}
    }
}

#[component]
pub fn MockAlbumDetail() -> Element {
    rsx! {
        mocks::AlbumDetailMock {}
    }
}
