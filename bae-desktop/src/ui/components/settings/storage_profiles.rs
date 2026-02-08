//! Storage Profiles section wrapper - handles persistence, delegates UI to StorageProfilesSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt, StorageProfilesStateStoreExt};
use bae_ui::{StorageProfile, StorageProfilesSectionView};
use dioxus::prelude::*;

/// Storage Profiles section - CRUD for profiles
#[component]
pub fn StorageProfilesSection() -> Element {
    let app = use_app();

    // Pass lenses directly - don't read here!
    let store = app.state.storage_profiles();
    let profiles = store.profiles();
    let is_loading = store.loading();

    // Encryption status from store (no keyring read — actual key only read on copy)
    let encryption_configured = *app.state.config().encryption_key_stored().read();
    let fingerprint = app
        .state
        .config()
        .encryption_key_fingerprint()
        .read()
        .clone();
    let encryption_key_fingerprint = fingerprint.unwrap_or_default();

    // Local UI state for editing
    let mut editing_profile = use_signal(|| Option::<StorageProfile>::None);
    let mut is_creating = use_signal(|| false);

    // Directory picker state
    let mut browsed_directory = use_signal(|| Option::<String>::None);

    let display_editing = editing_profile.read().clone();

    // Handle save from the view
    let handle_save = {
        let app = app.clone();
        move |profile: StorageProfile| {
            app.save_storage_profile(profile);
            is_creating.set(false);
            editing_profile.set(None);
        }
    };

    let handle_delete = {
        let app = app.clone();
        move |profile_id: String| {
            app.delete_storage_profile(&profile_id);
        }
    };

    let handle_set_default = {
        let app = app.clone();
        move |profile_id: String| {
            app.set_default_storage_profile(&profile_id);
        }
    };

    let handle_edit = move |profile: StorageProfile| {
        editing_profile.set(Some(profile));
        is_creating.set(false);
    };

    rsx! {
        StorageProfilesSectionView {
            profiles,
            is_loading,
            editing_profile: display_editing,
            is_creating: *is_creating.read(),
            encryption_configured,
            encryption_key_fingerprint,
            on_copy_key: {
                let app = app.clone();
                move |_| {
                    if let Some(key) = app.key_service.get_encryption_key() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(key);
                        }
                    }
                }
            },
            on_import_key: {
                let app = app.clone();
                move |key: String| {
                    if let Err(e) = app.key_service.set_encryption_key(&key) {
                        tracing::error!("Failed to save encryption key: {e}");
                        return;
                    }
                    let fp = bae_core::encryption::compute_key_fingerprint(&key);
                    app.save_config(|config| {
                        config.encryption_key_stored = true;
                        config.encryption_key_fingerprint = fp.clone();
                    });
                }
            },
            on_create: move |_| {
                is_creating.set(true);
                editing_profile.set(None);
            },
            on_edit: handle_edit,
            on_delete: handle_delete,
            on_set_default: handle_set_default,
            on_save: handle_save,
            on_cancel_edit: move |_| {
                editing_profile.set(None);
                is_creating.set(false);
            },
            on_browse_directory: move |_| {
                spawn(async move {
                    let folder = rfd::AsyncFileDialog::new().pick_folder().await;
                    if let Some(handle) = folder {
                        browsed_directory.set(Some(handle.path().to_string_lossy().to_string()));
                    }
                });
            },
            browsed_directory,
        }
    }
}
