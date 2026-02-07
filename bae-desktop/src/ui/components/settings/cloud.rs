//! Cloud sync section wrapper - handles config state and keyring, delegates UI to CloudSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, CloudSyncStatus, ConfigStateStoreExt};
use bae_ui::CloudSectionView;
use dioxus::prelude::*;

/// Cloud sync section - S3 config and credential management
#[component]
pub fn CloudSection() -> Element {
    let app = use_app();

    let encryption_configured = *app.state.config().encryption_key_stored().read();
    let store_enabled = *app.state.config().cloud_sync_enabled().read();
    let last_upload = app.state.config().cloud_sync_last_upload().read().clone();
    let sync_status = app.state.config().cloud_sync_status().read().clone();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut edit_enabled = use_signal(move || store_enabled);
    let mut edit_bucket = use_signal(move || {
        app.state
            .config()
            .cloud_sync_bucket()
            .read()
            .clone()
            .unwrap_or_default()
    });
    let mut edit_region = use_signal(move || {
        app.state
            .config()
            .cloud_sync_region()
            .read()
            .clone()
            .unwrap_or_default()
    });
    let mut edit_endpoint = use_signal(move || {
        app.state
            .config()
            .cloud_sync_endpoint()
            .read()
            .clone()
            .unwrap_or_default()
    });
    let mut edit_access_key = use_signal(String::new);
    let mut edit_secret_key = use_signal(String::new);

    let has_changes = *edit_enabled.read() != store_enabled
        || !edit_bucket.read().is_empty()
        || !edit_region.read().is_empty()
        || !edit_access_key.read().is_empty()
        || !edit_secret_key.read().is_empty();

    let on_edit_start = {
        let app = app.clone();
        move |_| {
            // Lazy read keyring on edit start
            let access_key = app
                .key_service
                .get_cloud_sync_access_key()
                .unwrap_or_default();
            let secret_key = app
                .key_service
                .get_cloud_sync_secret_key()
                .unwrap_or_default();

            edit_enabled.set(store_enabled);
            edit_bucket.set(
                app.state
                    .config()
                    .cloud_sync_bucket()
                    .read()
                    .clone()
                    .unwrap_or_default(),
            );
            edit_region.set(
                app.state
                    .config()
                    .cloud_sync_region()
                    .read()
                    .clone()
                    .unwrap_or_default(),
            );
            edit_endpoint.set(
                app.state
                    .config()
                    .cloud_sync_endpoint()
                    .read()
                    .clone()
                    .unwrap_or_default(),
            );
            edit_access_key.set(access_key);
            edit_secret_key.set(secret_key);
            is_editing.set(true);
        }
    };

    let save_changes = {
        let app = app.clone();
        move |_| {
            is_saving.set(true);
            save_error.set(None);

            let new_enabled = *edit_enabled.read();
            let new_bucket = edit_bucket.read().clone();
            let new_region = edit_region.read().clone();
            let new_endpoint = edit_endpoint.read().clone();
            let new_access_key = edit_access_key.read().clone();
            let new_secret_key = edit_secret_key.read().clone();

            // Save secrets to keyring
            if !new_access_key.is_empty() {
                if let Err(e) = app.key_service.set_cloud_sync_access_key(&new_access_key) {
                    save_error.set(Some(format!("Failed to save access key: {}", e)));
                    is_saving.set(false);
                    return;
                }
            }
            if !new_secret_key.is_empty() {
                if let Err(e) = app.key_service.set_cloud_sync_secret_key(&new_secret_key) {
                    save_error.set(Some(format!("Failed to save secret key: {}", e)));
                    is_saving.set(false);
                    return;
                }
            }

            // Save non-secret config
            app.save_config(move |config| {
                config.cloud_sync_enabled = new_enabled;
                config.cloud_sync_bucket = if new_bucket.is_empty() {
                    None
                } else {
                    Some(new_bucket)
                };
                config.cloud_sync_region = if new_region.is_empty() {
                    None
                } else {
                    Some(new_region)
                };
                config.cloud_sync_endpoint = if new_endpoint.is_empty() {
                    None
                } else {
                    Some(new_endpoint)
                };
            });

            is_saving.set(false);
            is_editing.set(false);
        }
    };

    let cancel_edit = move |_| {
        is_editing.set(false);
        save_error.set(None);
    };

    let on_sync_now = {
        let app = app.clone();
        move |_| {
            app.state
                .config()
                .cloud_sync_status()
                .set(CloudSyncStatus::Syncing);

            let app = app.clone();
            spawn(async move {
                match crate::ui::app_service::cloud_sync_upload(&app).await {
                    Ok(timestamp) => {
                        app.save_config(|c| {
                            c.cloud_sync_last_upload = Some(timestamp.clone());
                        });
                        app.state
                            .config()
                            .cloud_sync_last_upload()
                            .set(Some(timestamp));
                        app.state
                            .config()
                            .cloud_sync_status()
                            .set(CloudSyncStatus::Idle);
                    }
                    Err(e) => {
                        tracing::error!("Cloud sync failed: {}", e);

                        app.state
                            .config()
                            .cloud_sync_status()
                            .set(CloudSyncStatus::Error(e.to_string()));
                    }
                }
            });
        }
    };

    rsx! {
        CloudSectionView {
            encryption_configured,
            enabled: store_enabled,
            last_upload,
            sync_status,
            is_editing: *is_editing.read(),
            edit_enabled: *edit_enabled.read(),
            edit_bucket: edit_bucket.read().clone(),
            edit_region: edit_region.read().clone(),
            edit_endpoint: edit_endpoint.read().clone(),
            edit_access_key: edit_access_key.read().clone(),
            edit_secret_key: edit_secret_key.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start,
            on_cancel: cancel_edit,
            on_save: save_changes,
            on_sync_now,
            on_enabled_change: move |val| edit_enabled.set(val),
            on_bucket_change: move |val| edit_bucket.set(val),
            on_region_change: move |val| edit_region.set(val),
            on_endpoint_change: move |val| edit_endpoint.set(val),
            on_access_key_change: move |val| edit_access_key.set(val),
            on_secret_key_change: move |val| edit_secret_key.set(val),
        }
    }
}
