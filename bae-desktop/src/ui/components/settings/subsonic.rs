//! Subsonic section wrapper - handles config state, delegates UI to SubsonicSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt};
use bae_ui::SubsonicSectionView;
use dioxus::prelude::*;

fn expiry_to_string(days: Option<u32>) -> String {
    match days {
        Some(d) => d.to_string(),
        None => "never".to_string(),
    }
}

fn string_to_expiry(val: &str) -> Option<u32> {
    match val {
        "never" | "" => None,
        s => s.parse().ok(),
    }
}

#[component]
pub fn SubsonicSection() -> Element {
    let app = use_app();

    // Read config from Store
    let config_store = app.state.config();
    let store_enabled = *config_store.subsonic_enabled().read();
    let store_port = *config_store.subsonic_port().read();
    let store_share_base_url = config_store.share_base_url().read().clone();
    let store_share_expiry = *config_store.share_default_expiry_days().read();
    let store_share_version = *config_store.share_signing_key_version().read();
    let store_auth_enabled = *config_store.subsonic_auth_enabled().read();
    let store_username = config_store.subsonic_username().read().clone();
    let store_password_set = app.key_service.get_subsonic_password().is_some();

    // Server settings edit state
    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);
    let mut enabled = use_signal(move || store_enabled);
    let mut port = use_signal(move || store_port.to_string());
    let store_username_for_init = store_username.clone();
    let mut auth_enabled = use_signal(move || store_auth_enabled);
    let mut username = use_signal(move || store_username_for_init.clone().unwrap_or_default());
    let mut password = use_signal(String::new);
    let mut password_confirm = use_signal(String::new);

    let store_username_str = store_username.as_deref().unwrap_or("").to_string();
    let has_changes = *enabled.read() != store_enabled
        || *port.read() != store_port.to_string()
        || *auth_enabled.read() != store_auth_enabled
        || *username.read() != store_username_str
        || !password.read().is_empty();

    // Share link settings edit state
    let mut is_editing_share = use_signal(|| false);
    let mut is_saving_share = use_signal(|| false);
    let mut share_save_error = use_signal(|| Option::<String>::None);
    let initial_url = store_share_base_url.clone().unwrap_or_default();
    let mut edit_share_base_url = use_signal(move || initial_url.clone());
    let mut edit_share_expiry = use_signal(move || expiry_to_string(store_share_expiry));

    let current_url = store_share_base_url.clone().unwrap_or_default();
    let current_expiry = expiry_to_string(store_share_expiry);
    let has_share_changes =
        *edit_share_base_url.read() != current_url || *edit_share_expiry.read() != current_expiry;

    // Server settings save
    let save_changes = {
        let app = app.clone();
        move |_| {
            let new_enabled = *enabled.read();
            let new_port: u16 = port.read().parse().unwrap_or(4533);
            let new_auth_enabled = *auth_enabled.read();
            let new_username = username.read().clone();
            let new_password = password.read().clone();

            is_saving.set(true);
            save_error.set(None);

            // Save password to keyring if changed
            if !new_password.is_empty() {
                if let Err(e) = app.key_service.set_subsonic_password(&new_password) {
                    save_error.set(Some(format!("Failed to save password: {}", e)));
                    is_saving.set(false);
                    return;
                }
            }

            // If auth is being disabled, clean up the password from keyring
            if !new_auth_enabled && store_auth_enabled {
                if let Err(e) = app.key_service.delete_subsonic_password() {
                    tracing::warn!("Failed to delete subsonic password: {}", e);
                }
            }

            app.save_config(move |config| {
                config.subsonic_enabled = new_enabled;
                config.subsonic_port = new_port;
                config.subsonic_auth_enabled = new_auth_enabled;
                config.subsonic_username = if new_auth_enabled && !new_username.is_empty() {
                    Some(new_username)
                } else {
                    None
                };
            });

            is_saving.set(false);
            is_editing.set(false);
            password.set(String::new());
            password_confirm.set(String::new());
        }
    };

    let store_username_for_cancel = store_username_str.clone();
    let cancel_edit = move |_| {
        enabled.set(store_enabled);
        port.set(store_port.to_string());
        auth_enabled.set(store_auth_enabled);
        username.set(store_username_for_cancel.clone());
        password.set(String::new());
        password_confirm.set(String::new());
        is_editing.set(false);
        save_error.set(None);
    };

    // Share link settings save
    let save_share_changes = {
        let app = app.clone();
        move |_| {
            let new_url = edit_share_base_url.read().clone();
            let new_expiry = string_to_expiry(&edit_share_expiry.read());

            is_saving_share.set(true);
            share_save_error.set(None);

            let url_option = if new_url.is_empty() {
                None
            } else {
                Some(new_url)
            };

            app.save_config(move |config| {
                config.share_base_url = url_option;
                config.share_default_expiry_days = new_expiry;
            });

            is_saving_share.set(false);
            is_editing_share.set(false);
        }
    };

    let cancel_url = store_share_base_url.clone().unwrap_or_default();
    let cancel_share_edit = move |_| {
        edit_share_base_url.set(cancel_url.clone());
        edit_share_expiry.set(expiry_to_string(store_share_expiry));
        is_editing_share.set(false);
        share_save_error.set(None);
    };

    // Invalidate all share links
    let invalidate_links = {
        let app = app.clone();
        move |_| {
            app.save_config(move |config| {
                config.share_signing_key_version += 1;
            });
        }
    };

    let display_url = store_share_base_url.unwrap_or_default();

    rsx! {
        SubsonicSectionView {
            enabled: store_enabled,
            port: store_port,
            auth_enabled: store_auth_enabled,
            auth_username: store_username,
            auth_password_set: store_password_set,
            is_editing: *is_editing.read(),
            edit_enabled: *enabled.read(),
            edit_port: port.read().clone(),
            edit_auth_enabled: *auth_enabled.read(),
            edit_username: username.read().clone(),
            edit_password: password.read().clone(),
            edit_password_confirm: password_confirm.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start: move |_| is_editing.set(true),
            on_cancel: cancel_edit,
            on_save: save_changes,
            on_enabled_change: move |val| enabled.set(val),
            on_port_change: move |val| port.set(val),
            // Share link props
            share_base_url: display_url,
            share_default_expiry: expiry_to_string(store_share_expiry),
            share_signing_key_version: store_share_version,
            is_editing_share: *is_editing_share.read(),
            edit_share_base_url: edit_share_base_url.read().clone(),
            edit_share_expiry: edit_share_expiry.read().clone(),
            is_saving_share: *is_saving_share.read(),
            has_share_changes,
            share_save_error: share_save_error.read().clone(),
            on_share_edit_start: move |_| is_editing_share.set(true),
            on_share_cancel: cancel_share_edit,
            on_share_save: save_share_changes,
            on_share_base_url_change: move |val| edit_share_base_url.set(val),
            on_share_expiry_change: move |val| edit_share_expiry.set(val),
            on_invalidate_links: invalidate_links,
            on_auth_enabled_change: move |val| auth_enabled.set(val),
            on_username_change: move |val| username.set(val),
            on_password_change: move |val| password.set(val),
            on_password_confirm_change: move |val| password_confirm.set(val),
        }
    }
}
