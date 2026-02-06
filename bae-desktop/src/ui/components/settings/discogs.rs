//! Discogs section wrapper - handles config state, delegates UI to DiscogsSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt};
use bae_ui::DiscogsSectionView;
use dioxus::prelude::*;

/// Discogs section - API key management
#[component]
pub fn DiscogsSection() -> Element {
    let app = use_app();

    let read_only = app.key_service.is_dev_mode();
    let discogs_configured = *app.state.config().discogs_key_stored().read();

    let mut discogs_key = use_signal(|| Option::<String>::None);
    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let editing_key = discogs_key.read().clone();
    let has_changes = editing_key.is_some();

    let on_edit_start = {
        let app = app.clone();
        move |_| {
            // Lazy read: only touch the keyring when the user clicks Edit
            let current = app.key_service.get_discogs_key();
            discogs_key.set(current);
            is_editing.set(true);
        }
    };

    let save_changes = {
        let app = app.clone();
        move |_| {
            let new_key = discogs_key.read().clone();

            is_saving.set(true);
            save_error.set(None);

            if let Some(ref key) = new_key {
                if !key.is_empty() {
                    if let Err(e) = app.key_service.set_discogs_key(key) {
                        save_error.set(Some(format!("{}", e)));
                        is_saving.set(false);
                        return;
                    }

                    app.save_config(|c| c.discogs_key_stored = true);
                }
            }

            is_saving.set(false);
            is_editing.set(false);
        }
    };

    let cancel_edit = move |_| {
        discogs_key.set(None);
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        DiscogsSectionView {
            discogs_configured,
            discogs_key_value: editing_key.unwrap_or_default(),
            is_editing: *is_editing.read() && !read_only,
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start,
            on_key_change: move |val: String| {
                discogs_key.set(if val.is_empty() { None } else { Some(val) });
            },
            on_save: save_changes,
            on_cancel: cancel_edit,
        }
    }
}
