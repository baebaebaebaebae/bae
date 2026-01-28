#[cfg(feature = "cd-rip")]
use super::cd_import::CdImport;
use super::folder_import::FolderImport;
#[cfg(feature = "torrent")]
use super::torrent_import::TorrentImport;
use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{has_unclean_state, load_selected_release};
use bae_ui::stores::AppStateStoreExt;
use bae_ui::{ConfirmDialogView, ImportSource, ImportView};
use dioxus::prelude::*;
use tracing::warn;

#[component]
pub fn ImportPage() -> Element {
    let app = use_app();
    let import_store = app.state.import();
    let selected_source = import_store.read().selected_import_source;

    // Local state for switch confirmation dialog
    let mut pending_switch: Signal<Option<ImportSource>> = use_signal(|| None);
    let show_dialog = use_memo(move || pending_switch().is_some());
    let is_dialog_open: ReadSignal<bool> = show_dialog.into();

    let on_source_select = {
        let app = app.clone();
        move |source: ImportSource| {
            let current_source = app.state.import().read().selected_import_source;
            if current_source == source {
                return;
            }

            if has_unclean_state(&app) {
                // Show confirmation dialog
                pending_switch.set(Some(source));
            } else {
                // Switch directly
                let mut import_store = app.state.import();
                let mut state = import_store.write();
                state.selected_import_source = source;
                state.reset();
            }
        }
    };

    let on_confirm_switch = {
        let app = app.clone();
        move |_| {
            if let Some(source) = pending_switch() {
                let mut import_store = app.state.import();
                let mut state = import_store.write();
                state.selected_import_source = source;
                state.reset();
            }
            pending_switch.set(None);
        }
    };

    let on_cancel_switch = move |_| {
        pending_switch.set(None);
    };

    // Sidebar handlers
    let on_add_folder = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                if let Some(path) = rfd::AsyncFileDialog::new().pick_folder().await {
                    let path_str = path.path().to_string_lossy().to_string();
                    let import_handle = app.import_handle.clone();

                    {
                        let mut import_store = app.state.import();
                        if import_store.read().detected_candidates.is_empty() {
                            import_store.write().reset();
                        }
                        import_store.write().is_scanning_candidates = true;
                    }

                    if let Err(e) =
                        import_handle.enqueue_folder_scan(std::path::PathBuf::from(path_str))
                    {
                        warn!("Failed to add folder to scan: {}", e);
                    }
                }
            });
        }
    };

    let on_candidate_select = {
        let app = app.clone();
        move |index: usize| {
            let app = app.clone();
            let detected = app.state.import().read().detected_candidates.clone();
            spawn(async move {
                if let Err(e) = load_selected_release(&app, index, &detected).await {
                    warn!("Failed to switch to release: {}", e);
                }
            });
        }
    };

    let on_remove_candidate = {
        let app = app.clone();
        move |index: usize| {
            app.state.import().write().remove_detected_release(index);
        }
    };

    let on_clear_all = {
        let app = app.clone();
        move |_| {
            let mut store = app.state.import();
            let mut state = store.write();
            state.detected_candidates.clear();
            state.candidate_states.clear();
            state.loading_candidates.clear();
            state.discid_lookup_attempted.clear();
            state.switch_candidate(None);
        }
    };

    rsx! {
        ImportView {
            selected_source,
            on_source_select,
            state: import_store,
            on_candidate_select,
            on_add_folder,
            on_remove_candidate,
            on_clear_all,

            match selected_source {
                ImportSource::Folder => rsx! {
                    FolderImport {}
                },
                #[cfg(feature = "torrent")]
                ImportSource::Torrent => rsx! {
                    TorrentImport {}
                },
                #[cfg(feature = "cd-rip")]
                ImportSource::Cd => rsx! {
                    CdImport {}
                },
                #[cfg(not(all(feature = "torrent", feature = "cd-rip")))]
                _ => rsx! {
                    div { class: "p-4 text-red-500", "This import source is not available" }
                },
            }
        }

        // Switch confirmation dialog
        ConfirmDialogView {
            is_open: is_dialog_open,
            title: "Watch out!".to_string(),
            message: "You have unsaved work. Navigating away will discard your current progress."
                .to_string(),
            confirm_label: "Switch Tab".to_string(),
            cancel_label: "Cancel".to_string(),
            is_destructive: true,
            on_confirm: on_confirm_switch,
            on_cancel: on_cancel_switch,
        }
    }
}
