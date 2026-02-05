mod about;
mod bittorrent;
mod discogs;
mod storage_profiles;
mod subsonic;

use bae_ui::SettingsTab;
use bae_ui::SettingsView;
use dioxus::prelude::*;

/// Settings page with tabbed navigation
#[component]
pub fn Settings() -> Element {
    let mut active_tab = use_signal(|| SettingsTab::Storage);

    rsx! {
        SettingsView {
            active_tab: *active_tab.read(),
            on_tab_change: move |tab| active_tab.set(tab),
            match *active_tab.read() {
                SettingsTab::Storage => rsx! {
                    storage_profiles::StorageProfilesSection {}
                },
                SettingsTab::Discogs => rsx! {
                    discogs::DiscogsSection {}
                },
                SettingsTab::BitTorrent => rsx! {
                    bittorrent::BitTorrentSection {}
                },
                SettingsTab::Subsonic => rsx! {
                    subsonic::SubsonicSection {}
                },
                SettingsTab::About => rsx! {
                    about::AboutSection {}
                },
            }
        }
    }
}
