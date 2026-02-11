//! Sync section wrapper - reads sync state from Store, delegates UI to SyncSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, SyncStateStoreExt};
use bae_ui::SyncSectionView;
use dioxus::prelude::*;

/// Sync section - shows sync status, other devices, and user identity
#[component]
pub fn SyncSection() -> Element {
    let app = use_app();

    let last_sync_time = app.state.sync().last_sync_time().read().clone();
    let other_devices = app.state.sync().other_devices().read().clone();
    let syncing = *app.state.sync().syncing().read();
    let error = app.state.sync().error().read().clone();
    let user_pubkey = app.state.sync().user_pubkey().read().clone();

    let copy_pubkey = {
        let user_pubkey = user_pubkey.clone();
        move |_| {
            if let Some(ref pk) = user_pubkey {
                let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(pk));
            }
        }
    };

    rsx! {
        SyncSectionView {
            last_sync_time,
            other_devices,
            syncing,
            error,
            user_pubkey,
            on_copy_pubkey: copy_pubkey,
        }
    }
}
