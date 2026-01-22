//! About section wrapper - handles library stats, delegates UI to AboutSectionView

use crate::ui::app_service::use_app;
use crate::updater;
use bae_ui::stores::{AppStateStoreExt, LibraryStateStoreExt};
use bae_ui::AboutSectionView;
use dioxus::prelude::*;

const VERSION: &str = env!("BAE_VERSION");

/// About section - version info and library stats
#[component]
pub fn AboutSection() -> Element {
    let app = use_app();

    // Read album count from Store
    let album_count = use_memo(move || app.state.library().albums().read().len());

    rsx! {
        AboutSectionView {
            version: VERSION.to_string(),
            album_count: album_count(),
            on_check_updates: move |_| {
                updater::check_for_updates();
            },
        }
    }
}
