//! App layout wrapper for desktop app
//!
//! Wraps the shared AppLayoutView with desktop-specific components.

use super::now_playing_bar::NowPlayingBar;
use super::queue_sidebar::QueueSidebar;
use super::TitleBar;
use crate::ui::shortcuts::ShortcutsHandler;
use crate::ui::Route;
use bae_ui::AppLayoutView;
use dioxus::prelude::*;

/// Layout component that includes title bar, content, playback bar, and sidebar
#[component]
pub fn AppLayout() -> Element {
    // If we were relaunched after a library switch, navigate to Settings
    use_effect(|| {
        if std::env::var("BAE_OPEN_SETTINGS").is_ok() {
            unsafe { std::env::remove_var("BAE_OPEN_SETTINGS") };
            navigator().replace(Route::Settings {});
        }
    });

    rsx! {
        ShortcutsHandler {
            AppLayoutView {
                title_bar: rsx! {
                    TitleBar {}
                },
                playback_bar: rsx! {
                    NowPlayingBar {}
                },
                queue_sidebar: rsx! {
                    QueueSidebar {}
                },
                Outlet::<Route> {}
            }
        }
    }
}
