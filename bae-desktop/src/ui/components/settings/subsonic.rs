//! Subsonic section wrapper - handles config state, delegates UI to SubsonicSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt};
use bae_ui::SubsonicSectionView;
use dioxus::prelude::*;

#[component]
pub fn SubsonicSection() -> Element {
    let app = use_app();

    // Read config from Store
    let config_store = app.state.config();
    let store_enabled = *config_store.subsonic_enabled().read();
    let store_port = *config_store.subsonic_port().read();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut enabled = use_signal(move || store_enabled);
    let mut port = use_signal(move || store_port.to_string());

    let has_changes = *enabled.read() != store_enabled || *port.read() != store_port.to_string();

    let save_changes = {
        let app = app.clone();
        move |_| {
            let new_enabled = *enabled.read();
            let new_port: u16 = port.read().parse().unwrap_or(4533);

            is_saving.set(true);
            save_error.set(None);

            app.save_config(move |config| {
                config.subsonic_enabled = new_enabled;
                config.subsonic_port = new_port;
            });

            is_saving.set(false);
            is_editing.set(false);
        }
    };

    let cancel_edit = move |_| {
        enabled.set(store_enabled);
        port.set(store_port.to_string());
        is_editing.set(false);
        save_error.set(None);
    };

    // Share link settings
    let store_share_base_url = config_store
        .share_base_url()
        .read()
        .clone()
        .unwrap_or_default();
    let store_share_expiry = *config_store.share_default_expiry_days().read();
    let store_share_key_version = *config_store.share_signing_key_version().read();

    let mut share_is_editing = use_signal(|| false);
    let mut share_is_saving = use_signal(|| false);
    let mut share_save_error = use_signal(|| Option::<String>::None);

    let mut share_edit_base_url = use_signal(move || store_share_base_url.clone());
    let mut share_edit_expiry = use_signal(move || store_share_expiry);

    let share_has_changes = *share_edit_base_url.read()
        != config_store
            .share_base_url()
            .read()
            .clone()
            .unwrap_or_default()
        || *share_edit_expiry.read() != store_share_expiry;

    let share_save = {
        let app = app.clone();
        move |_| {
            let new_base_url = share_edit_base_url.read().clone();
            let new_expiry = *share_edit_expiry.read();

            share_is_saving.set(true);
            share_save_error.set(None);

            app.save_config(move |config| {
                config.share_base_url = if new_base_url.is_empty() {
                    None
                } else {
                    Some(new_base_url)
                };
                config.share_default_expiry_days = new_expiry;
            });

            share_is_saving.set(false);
            share_is_editing.set(false);
        }
    };

    let share_cancel = {
        let store_base = config_store
            .share_base_url()
            .read()
            .clone()
            .unwrap_or_default();
        move |_| {
            share_edit_base_url.set(store_base.clone());
            share_edit_expiry.set(store_share_expiry);
            share_is_editing.set(false);
            share_save_error.set(None);
        }
    };

    let share_rotate_key = {
        let app = app.clone();
        move |_| {
            let current_version = *config_store.share_signing_key_version().read();
            app.save_config(move |config| {
                config.share_signing_key_version = current_version + 1;
            });
        }
    };

    rsx! {
        SubsonicSectionView {
            enabled: store_enabled,
            port: store_port,
            is_editing: *is_editing.read(),
            edit_enabled: *enabled.read(),
            edit_port: port.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start: move |_| is_editing.set(true),
            on_cancel: cancel_edit,
            on_save: save_changes,
            on_enabled_change: move |val| enabled.set(val),
            on_port_change: move |val| port.set(val),
            share_base_url: config_store.share_base_url().read().clone().unwrap_or_default(),
            share_is_editing: *share_is_editing.read(),
            share_edit_base_url: share_edit_base_url.read().clone(),
            share_default_expiry_days: store_share_expiry,
            share_edit_expiry_days: *share_edit_expiry.read(),
            share_signing_key_version: store_share_key_version,
            share_is_saving: *share_is_saving.read(),
            share_has_changes,
            share_save_error: share_save_error.read().clone(),
            on_share_edit_start: move |_| share_is_editing.set(true),
            on_share_cancel: share_cancel,
            on_share_save: share_save,
            on_share_base_url_change: move |val| share_edit_base_url.set(val),
            on_share_expiry_change: move |val| share_edit_expiry.set(val),
            on_share_rotate_key: share_rotate_key,
        }
    }
}
