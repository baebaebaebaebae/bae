//! Imports dropdown wrapper
//!
//! Thin wrapper that bridges App state to ImportsDropdownView.

use crate::ui::app_service::use_app;
use crate::ui::Route;
use bae_ui::display_types::{ActiveImport as DisplayActiveImport, ImportStatus};
use bae_ui::stores::{ActiveImportsUiStateStoreExt, AppStateStoreExt, ImportOperationStatus};
use bae_ui::ImportsDropdownView;
use dioxus::prelude::*;

/// Dropdown content showing list of active imports with progress
#[component]
pub fn ImportsDropdown(dropdown_open: Signal<bool>) -> Element {
    let app = use_app();
    let active_imports_store = app.state.active_imports();
    let imports_store = active_imports_store.imports();
    let imports = imports_store.read();

    // Convert to display types
    let display_imports: Vec<DisplayActiveImport> = imports
        .iter()
        .map(|i| {
            let cover_url = i.cover_art_url.clone();

            DisplayActiveImport {
                import_id: i.import_id.clone(),
                album_title: i.album_title.clone(),
                artist_name: i.artist_name.clone(),
                status: match i.status {
                    ImportOperationStatus::Pending => ImportStatus::Preparing,
                    ImportOperationStatus::Preparing => ImportStatus::Preparing,
                    ImportOperationStatus::Importing => ImportStatus::Importing,
                    ImportOperationStatus::Complete => ImportStatus::Complete,
                    ImportOperationStatus::Failed => ImportStatus::Failed,
                },
                current_step_text: i.current_step.map(|s| format!("{:?}", s)),
                progress_percent: i.progress_percent,
                release_id: i.release_id.clone(),
                cover_url,
            }
        })
        .collect();

    // Build a map from import_id to release_id for navigation
    let release_ids: std::collections::HashMap<String, Option<String>> = imports
        .iter()
        .map(|i| (i.import_id.clone(), i.release_id.clone()))
        .collect();

    rsx! {
        ImportsDropdownView {
            imports: display_imports,
            on_import_click: {
                let release_ids = release_ids.clone();
                let app = app.clone();
                move |import_id: String| {
                    if let Some(Some(rid)) = release_ids.get(&import_id) {
                        let rid = rid.clone();
                        let library_manager = app.library_manager.clone();
                        let mut dropdown_open = dropdown_open;
                        spawn(async move {
                            if let Ok(album_id) = library_manager
                                .get()
                                .get_album_id_for_release(&rid)
                                .await
                            {
                                dropdown_open.set(false);
                                navigator()
                                    .push(Route::AlbumDetail {
                                        album_id,
                                        release_id: rid,
                                    });
                            }
                        });
                    }
                }
            },
            on_import_dismiss: {
                let app = app.clone();
                move |import_id: String| {
                    // Remove from UI state, also delete from DB so it doesn't reappear after restart
                    app.state
                        .active_imports()
                        .imports()
                        .with_mut(|list| {
                            list.retain(|i| i.import_id != import_id);
                        });
                    let library_manager = app.library_manager.clone();
                    spawn(async move {
                        if let Err(e) = library_manager.get().delete_import(&import_id).await {
                            tracing::warn!("Failed to delete import from DB: {}", e);
                        }
                    });
                }
            },
            on_clear_all: {
                let app = app.clone();
                move |_| {
                    // Collect IDs before clearing the list
                    let mut imports_store = app.state.active_imports().imports();
                    let import_ids: Vec<String> = imports_store
                        .read()
                        .iter()
                        .map(|i| i.import_id.clone())
                        .collect();
                    imports_store.with_mut(|list| list.clear());

                    // Delete all from DB
                    let library_manager = app.library_manager.clone();
                    spawn(async move {
                        for id in import_ids {
                            if let Err(e) = library_manager.get().delete_import(&id).await {
                                tracing::warn!("Failed to delete import {} from DB: {}", id, e);
                            }
                        }
                    });
                }
            },
        }
    }
}
