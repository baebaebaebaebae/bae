//! Sync section wrapper - reads sync state from Store, manages edit state locally,
//! delegates config persistence to AppService

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, MemberRole, SyncStateStoreExt};
use bae_ui::{SyncBucketConfig, SyncSectionView};
use dioxus::prelude::*;

/// Sync section - shows sync status, other devices, user identity, and sync bucket configuration
#[component]
pub fn SyncSection() -> Element {
    let app = use_app();

    // --- Status from store ---
    let last_sync_time = app.state.sync().last_sync_time().read().clone();
    let other_devices = app.state.sync().other_devices().read().clone();
    let syncing = *app.state.sync().syncing().read();
    let error = app.state.sync().error().read().clone();
    let user_pubkey = app.state.sync().user_pubkey().read().clone();

    // --- Members from store ---
    let members = app.state.sync().members().read().clone();
    let is_owner = members
        .iter()
        .any(|m| m.is_self && m.role == MemberRole::Owner);

    // Load membership on mount
    let app_for_membership = app.clone();
    use_effect(move || {
        app_for_membership.load_membership();
    });

    let copy_pubkey = {
        let user_pubkey = user_pubkey.clone();
        move |_| {
            if let Some(ref pk) = user_pubkey {
                let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(pk));
            }
        }
    };

    // --- Config display from store ---
    let sync_bucket = app.state.sync().sync_bucket().read().clone();
    let sync_region = app.state.sync().sync_region().read().clone();
    let sync_endpoint = app.state.sync().sync_endpoint().read().clone();
    let sync_configured = *app.state.sync().sync_configured().read();

    // --- Local edit state ---
    let mut is_editing = use_signal(|| false);
    let mut edit_bucket = use_signal(String::new);
    let mut edit_region = use_signal(String::new);
    let mut edit_endpoint = use_signal(String::new);
    let mut edit_access_key = use_signal(String::new);
    let mut edit_secret_key = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    // --- Test connection state ---
    let mut is_testing = use_signal(|| false);
    let mut test_success = use_signal(|| Option::<String>::None);
    let mut test_error = use_signal(|| Option::<String>::None);

    // Clone app for each closure that needs it
    let app_for_sync = app.clone();
    let app_for_edit = app.clone();
    let app_for_save = app.clone();
    let app_for_test = app.clone();

    rsx! {
        SyncSectionView {
            // Status
            last_sync_time,
            other_devices,
            syncing,
            error,
            user_pubkey,
            on_copy_pubkey: copy_pubkey,
            members,
            is_owner,
            on_remove_member: // Phase 6e: remove member from membership chain
            |_pubkey: String| {},
            on_sync_now: move |_| app_for_sync.trigger_sync(),

            // Config display
            sync_bucket: sync_bucket.clone(),
            sync_region: sync_region.clone(),
            sync_endpoint: sync_endpoint.clone(),
            sync_configured,

            // Edit state
            is_editing: *is_editing.read(),
            edit_bucket: edit_bucket.read().clone(),
            edit_region: edit_region.read().clone(),
            edit_endpoint: edit_endpoint.read().clone(),
            edit_access_key: edit_access_key.read().clone(),
            edit_secret_key: edit_secret_key.read().clone(),
            is_saving: *is_saving.read(),
            save_error: save_error.read().clone(),

            // Test state
            is_testing: *is_testing.read(),
            test_success: test_success.read().clone(),
            test_error: test_error.read().clone(),

            // Callbacks
            on_edit_start: move |_| {
                // Populate edit fields from current config
                edit_bucket.set(sync_bucket.clone().unwrap_or_default());
                edit_region.set(sync_region.clone().unwrap_or_default());
                edit_endpoint.set(sync_endpoint.clone().unwrap_or_default());
                // Read credentials from keyring for editing
                edit_access_key
                    .set(app_for_edit.key_service.get_sync_access_key().unwrap_or_default());
                edit_secret_key
                    .set(app_for_edit.key_service.get_sync_secret_key().unwrap_or_default());
                save_error.set(None);
                test_success.set(None);
                test_error.set(None);
                is_editing.set(true);
            },
            on_cancel_edit: move |_| {
                is_editing.set(false);
                save_error.set(None);
                test_success.set(None);
                test_error.set(None);
            },
            on_save_config: move |config: SyncBucketConfig| {
                is_saving.set(true);
                save_error.set(None);
                let app = app_for_save.clone();
                spawn(async move {
                    match app.save_sync_config(config) {
                        Ok(()) => {
                            is_editing.set(false);
                        }
                        Err(e) => {
                            save_error.set(Some(e));
                        }
                    }
                    is_saving.set(false);
                });
            },
            on_test_connection: move |_| {
                let library_manager = app_for_test.library_manager.clone();
                let bucket = edit_bucket.read().clone();
                let region = edit_region.read().clone();
                let endpoint = edit_endpoint.read().clone();
                let access_key = edit_access_key.read().clone();
                let secret_key = edit_secret_key.read().clone();

                is_testing.set(true);
                test_success.set(None);
                test_error.set(None);

                spawn(async move {
                    let encryption_service = library_manager.get().encryption_service().cloned();

                    let result: Result<usize, String> = async {
                        let encryption = encryption_service

                            .ok_or_else(|| {
                                "Encryption is not configured. Enable encryption first."
                                    .to_string()
                            })?;
                        let ep = if endpoint.is_empty() { None } else { Some(endpoint) };
                        let client = bae_core::sync::s3_bucket::S3SyncBucketClient::new(
                                bucket,
                                region,
                                ep,
                                access_key,
                                secret_key,
                                encryption,
                            )
                            .await
                            .map_err(|e| format!("Failed to create S3 client: {}", e))?;
                        use bae_core::sync::bucket::SyncBucketClient;
                        let heads = client.list_heads().await.map_err(|e| format!("{}", e))?;
                        Ok(heads.len())
                    }
                        .await;
                    match result {
                        Ok(count) => {
                            let msg = if count == 0 {
                                "Connected successfully. No other devices syncing yet."
                                    .to_string()
                            } else {
                                format!(
                                    "Connected successfully. Found {} device head(s).",
                                    count,
                                )
                            };
                            test_success.set(Some(msg));
                            test_error.set(None);
                        }
                        Err(e) => {
                            test_error.set(Some(e));
                            test_success.set(None);
                        }
                    }
                    is_testing.set(false);
                });
            },
            on_bucket_change: move |v| edit_bucket.set(v),
            on_region_change: move |v| edit_region.set(v),
            on_endpoint_change: move |v| edit_endpoint.set(v),
            on_access_key_change: move |v| edit_access_key.set(v),
            on_secret_key_change: move |v| edit_secret_key.set(v),
        }
    }
}
