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
        }
    }
}
