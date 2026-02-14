//! Sync section wrapper - reads sync state from Store, manages edit state locally,
//! delegates config persistence to AppService

use crate::ui::app_service::use_app;
use bae_ui::stores::config::CloudProvider;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt, MemberRole, SyncStateStoreExt};
use bae_ui::{CloudProviderOption, SyncBucketConfig, SyncSectionView};
use dioxus::prelude::*;

/// Build the list of cloud provider options from current state.
fn build_cloud_options(
    cloud_provider: &Option<CloudProvider>,
    cloud_home_configured: bool,
    icloud_available: bool,
    cloud_account_display: &Option<String>,
    cloud_home_bucket: &Option<String>,
) -> Vec<CloudProviderOption> {
    let s3_connected = if matches!(cloud_provider, Some(CloudProvider::S3)) && cloud_home_configured
    {
        cloud_home_bucket.clone()
    } else {
        None
    };

    let google_connected =
        if matches!(cloud_provider, Some(CloudProvider::GoogleDrive)) && cloud_home_configured {
            cloud_account_display.clone()
        } else {
            None
        };

    vec![
        CloudProviderOption {
            provider: CloudProvider::ICloud,
            label: "iCloud Drive",
            description: "Automatic sync, no setup needed",
            available: icloud_available,
            connected_account: if matches!(cloud_provider, Some(CloudProvider::ICloud)) {
                Some("iCloud Drive".to_string())
            } else {
                None
            },
        },
        CloudProviderOption {
            provider: CloudProvider::GoogleDrive,
            label: "Google Drive",
            description: "Sign in to sync via Google Drive",
            available: true,
            connected_account: google_connected,
        },
        CloudProviderOption {
            provider: CloudProvider::Dropbox,
            label: "Dropbox",
            description: "Sign in to sync via Dropbox",
            available: true,
            connected_account: if matches!(cloud_provider, Some(CloudProvider::Dropbox))
                && cloud_home_configured
            {
                cloud_account_display.clone()
            } else {
                None
            },
        },
        CloudProviderOption {
            provider: CloudProvider::OneDrive,
            label: "OneDrive",
            description: "Sign in to sync via OneDrive",
            available: true,
            connected_account: if matches!(cloud_provider, Some(CloudProvider::OneDrive))
                && cloud_home_configured
            {
                cloud_account_display.clone()
            } else {
                None
            },
        },
        CloudProviderOption {
            provider: CloudProvider::PCloud,
            label: "pCloud",
            description: "Sign in to sync via pCloud",
            available: true,
            connected_account: if matches!(cloud_provider, Some(CloudProvider::PCloud))
                && cloud_home_configured
            {
                cloud_account_display.clone()
            } else {
                None
            },
        },
        CloudProviderOption {
            provider: CloudProvider::S3,
            label: "S3-compatible",
            description: "For Backblaze B2, Wasabi, MinIO, AWS, etc.",
            available: true,
            connected_account: s3_connected,
        },
    ]
}

/// Sync section - shows sync status, other devices, user identity, cloud provider picker,
/// and membership management.
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

    // --- Invite state from store ---
    let invite_status = app.state.sync().invite_status().read().clone();
    let share_info = app.state.sync().share_info().read().clone();

    // --- Shared releases from store ---
    let shared_releases = app.state.sync().shared_releases().read().clone();

    // --- Local accept form state ---
    let mut accept_grant_text = use_signal(String::new);
    let mut is_accepting_grant = use_signal(|| false);
    let mut accept_grant_error = use_signal(|| Option::<String>::None);

    // Load membership and shared releases on mount
    let app_for_membership = app.clone();
    let app_for_load_shared = app.clone();
    use_effect(move || {
        app_for_membership.load_membership();
        app_for_load_shared.load_shared_releases();
    });

    let copy_pubkey = {
        let user_pubkey = user_pubkey.clone();
        move |_| {
            if let Some(ref pk) = user_pubkey {
                let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(pk));
            }
        }
    };

    // --- Cloud provider state from store ---
    let cloud_provider = app.state.config().cloud_provider().read().clone();
    let cloud_account_display = app.state.config().cloud_account_display().read().clone();
    let cloud_home_bucket = app.state.sync().cloud_home_bucket().read().clone();
    let cloud_home_configured = *app.state.sync().cloud_home_configured().read();
    let icloud_available = *app.state.sync().icloud_available().read();
    let signing_in = *app.state.sync().signing_in().read();
    let sign_in_error = app.state.sync().sign_in_error().read().clone();

    let cloud_options = build_cloud_options(
        &cloud_provider,
        cloud_home_configured,
        icloud_available,
        &cloud_account_display,
        &cloud_home_bucket,
    );

    // --- Local S3 edit state ---
    let mut is_editing = use_signal(|| false);
    let mut edit_bucket = use_signal(String::new);
    let mut edit_region = use_signal(String::new);
    let mut edit_endpoint = use_signal(String::new);
    let mut edit_access_key = use_signal(String::new);
    let mut edit_secret_key = use_signal(String::new);

    // --- Local invite form state ---
    let mut show_invite_form = use_signal(|| false);
    let mut invite_pubkey = use_signal(String::new);
    let mut invite_role = use_signal(|| MemberRole::Member);

    // --- Remove member state from store ---
    let is_removing_member = *app.state.sync().removing_member().read();
    let removing_member_error = app.state.sync().remove_member_error().read().clone();

    // Clone app for each closure that needs it
    let app_for_sync = app.clone();
    let app_for_edit = app.clone();
    let app_for_save = app.clone();
    let app_for_invite = app.clone();
    let app_for_dismiss = app.clone();
    let app_for_remove = app.clone();
    let app_for_accept = app.clone();
    let app_for_revoke = app.clone();
    let app_for_sign_in = app.clone();
    let app_for_disconnect = app.clone();
    let app_for_select = app.clone();

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
            on_remove_member: move |pubkey: String| {
                app_for_remove.remove_member(pubkey);
            },
            is_removing_member,
            removing_member_error,
            on_sync_now: move |_| app_for_sync.trigger_sync(),

            // Cloud home configured
            cloud_home_configured,

            // Cloud provider picker
            cloud_provider: cloud_provider.clone(),
            cloud_options,
            signing_in,
            sign_in_error,
            on_select_provider: move |provider: CloudProvider| {
                app_for_select.select_cloud_provider(provider);
            },
            on_sign_in: move |provider: CloudProvider| {
                app_for_sign_in.sign_in_cloud_provider(provider);
            },
            on_disconnect_provider: move |_| {
                app_for_disconnect.disconnect_cloud_provider();
            },
            on_use_icloud: // iCloud not yet implemented
            |_| {},

            // S3 edit state
            is_editing: *is_editing.read(),
            edit_bucket: edit_bucket.read().clone(),
            edit_region: edit_region.read().clone(),
            edit_endpoint: edit_endpoint.read().clone(),
            edit_access_key: edit_access_key.read().clone(),
            edit_secret_key: edit_secret_key.read().clone(),

            // S3 callbacks
            on_edit_start: move |_| {
                edit_bucket
                    .set(
                        app_for_edit
                            .state
                            .sync()
                            .cloud_home_bucket()
                            .read()
                            .clone()
                            .unwrap_or_default(),
                    );
                edit_region
                    .set(
                        app_for_edit
                            .state
                            .sync()
                            .cloud_home_region()
                            .read()
                            .clone()
                            .unwrap_or_default(),
                    );
                edit_endpoint
                    .set(
                        app_for_edit
                            .state
                            .sync()
                            .cloud_home_endpoint()
                            .read()
                            .clone()
                            .unwrap_or_default(),
                    );
                edit_access_key
                    .set(
                        app_for_edit.key_service.get_cloud_home_access_key().unwrap_or_default(),
                    );
                edit_secret_key
                    .set(
                        app_for_edit.key_service.get_cloud_home_secret_key().unwrap_or_default(),
                    );
                is_editing.set(true);
            },
            on_cancel_edit: move |_| {
                is_editing.set(false);
            },
            on_save_config: move |config: SyncBucketConfig| {
                let app = app_for_save.clone();
                spawn(async move {
                    match app.save_sync_config(config) {
                        Ok(()) => {
                            is_editing.set(false);
                        }
                        Err(e) => {
                            tracing::error!("Failed to save sync config: {e}");
                        }
                    }
                });
            },
            on_bucket_change: move |v| edit_bucket.set(v),
            on_region_change: move |v| edit_region.set(v),
            on_endpoint_change: move |v| edit_endpoint.set(v),
            on_access_key_change: move |v| edit_access_key.set(v),
            on_secret_key_change: move |v| edit_secret_key.set(v),

            // Invite state
            show_invite_form: *show_invite_form.read(),
            invite_pubkey: invite_pubkey.read().clone(),
            invite_role: invite_role.read().clone(),
            invite_status,
            share_info,

            // Invite callbacks
            on_toggle_invite_form: move |_| {
                let currently_open = *show_invite_form.read();
                if currently_open {
                    show_invite_form.set(false);
                    invite_pubkey.set(String::new());
                    invite_role.set(MemberRole::Member);
                    app_for_invite.state.sync().invite_status().set(None);
                } else {
                    show_invite_form.set(true);
                }
            },
            on_invite_pubkey_change: move |v| invite_pubkey.set(v),
            on_invite_role_change: move |v| invite_role.set(v),
            on_invite_member: {
                let app = app.clone();
                move |(pubkey, role): (String, MemberRole)| {
                    app.invite_member(pubkey, role);
                }
            },
            on_copy_share_info: move |text: String| {
                let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&text));
            },
            on_dismiss_share_info: move |_| {
                app_for_dismiss.state.sync().share_info().set(None);
                app_for_dismiss.state.sync().invite_status().set(None);
                show_invite_form.set(false);
                invite_pubkey.set(String::new());
                invite_role.set(MemberRole::Member);
            },

            // Shared releases
            shared_releases,
            accept_grant_text: accept_grant_text.read().clone(),
            is_accepting_grant: *is_accepting_grant.read(),
            accept_grant_error: accept_grant_error.read().clone(),
            on_accept_grant_text_change: move |v| accept_grant_text.set(v),
            on_accept_grant: move |json: String| {
                let app = app_for_accept.clone();
                is_accepting_grant.set(true);
                accept_grant_error.set(None);

                spawn(async move {
                    let result: Result<(), String> = async {
                        let keypair = app
                            .user_keypair
                            .as_ref()
                            .ok_or_else(|| "No user keypair available".to_string())?;

                        let grant: bae_core::sync::share_grant::ShareGrant = serde_json::from_str(
                                &json,
                            )
                            .map_err(|e| format!("Invalid JSON: {e}"))?;
                        bae_core::sync::shared_release::accept_and_store_grant(
                                app.library_manager.get().database(),
                                &grant,
                                keypair,
                            )
                            .await
                            .map_err(|e| format!("{e}"))?;
                        Ok(())
                    }
                        .await;
                    match result {
                        Ok(()) => {
                            accept_grant_text.set(String::new());
                            app.load_shared_releases();
                        }
                        Err(e) => {
                            accept_grant_error.set(Some(e));
                        }
                    }
                    is_accepting_grant.set(false);
                });
            },
            on_revoke_shared_release: move |grant_id: String| {
                app_for_revoke.revoke_shared_release(grant_id);
            },
        }
    }
}
