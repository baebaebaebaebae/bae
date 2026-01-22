//! API Keys section wrapper - handles config state, delegates UI to ApiKeysSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt};
use bae_ui::ApiKeysSectionView;
use dioxus::prelude::*;

/// API Keys section - Discogs key management
#[component]
pub fn ApiKeysSection() -> Element {
    let app = use_app();

    // Read config from Store
    let config_store = app.state.config();
    let store_discogs_key = config_store.discogs_api_key().read().clone();

    let initial_key = store_discogs_key.clone();
    let mut discogs_key = use_signal(move || initial_key.clone());
    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let has_changes = *discogs_key.read() != store_discogs_key;
    let discogs_configured = store_discogs_key.is_some();

    let save_changes = {
        let app = app.clone();
        move |_| {
            let new_key = discogs_key.read().clone();

            is_saving.set(true);
            save_error.set(None);

            app.save_config(move |config| {
                config.discogs_api_key = new_key;
            });

            is_saving.set(false);
            is_editing.set(false);
        }
    };

    let cancel_edit = {
        let store_discogs_key = store_discogs_key.clone();
        move |_| {
            discogs_key.set(store_discogs_key.clone());
            is_editing.set(false);
            save_error.set(None);
        }
    };

    rsx! {
        ApiKeysSectionView {
            discogs_configured,
            discogs_key_value: discogs_key.read().clone().unwrap_or_default(),
            is_editing: *is_editing.read(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start: move |_| is_editing.set(true),
            on_key_change: move |val: String| {
                discogs_key.set(if val.is_empty() { None } else { Some(val) });
            },
            on_save: save_changes,
            on_cancel: cancel_edit,
        }
    }
}
