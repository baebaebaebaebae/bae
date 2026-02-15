//! Settings view - tabbed layout shell

use crate::components::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

/// Available settings tabs
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsTab {
    Library,
    Sync,
    Discogs,
    BitTorrent,
    Subsonic,
    About,
}

impl SettingsTab {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::Library => "Library",
            SettingsTab::Sync => "Sync",
            SettingsTab::Discogs => "Discogs",
            SettingsTab::BitTorrent => "BitTorrent",
            SettingsTab::Subsonic => "Subsonic",
            SettingsTab::About => "About",
        }
    }

    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Library,
            SettingsTab::Sync,
            SettingsTab::Discogs,
            #[cfg(feature = "torrent")]
            SettingsTab::BitTorrent,
            SettingsTab::Subsonic,
            SettingsTab::About,
        ]
    }
}

/// Settings page view with tabbed navigation
#[component]
pub fn SettingsView(
    active_tab: SettingsTab,
    on_tab_change: EventHandler<SettingsTab>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex flex-col w-full h-full min-h-0",
            div { class: "px-6 pt-6 pb-2",
                h1 { class: "text-2xl font-bold text-white", "Settings" }
            }
            div { class: "flex flex-1 min-h-0 overflow-clip",
                nav { class: "w-56 p-4 flex-shrink-0",
                    ul { class: "space-y-1",
                        for tab in SettingsTab::all() {
                            li {
                                Button {
                                    variant: if active_tab == *tab { ButtonVariant::Primary } else { ButtonVariant::Ghost },
                                    size: ButtonSize::Medium,
                                    class: Some("w-full justify-start".to_string()),
                                    onclick: {
                                        let tab = *tab;
                                        move |_| on_tab_change.call(tab)
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex-1 overflow-y-auto p-6", {children} }
            }
        }
    }
}
