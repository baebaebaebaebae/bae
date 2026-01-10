use super::dialog::GlobalDialog;
use super::now_playing_bar::NowPlayingBar;
use super::queue_sidebar::QueueSidebar;
#[cfg(all(target_os = "macos", feature = "desktop"))]
use super::TitleBar;
use crate::ui::Route;
use dioxus::prelude::*;

/// Layout component that includes title bar and content
#[component]
pub fn Navbar() -> Element {
    rsx! {
        {
            #[cfg(all(target_os = "macos", feature = "desktop"))]
            {
                rsx! {
                    TitleBar {}
                }
            }
            #[cfg(not(all(target_os = "macos", feature = "desktop")))]
            {
                rsx! {}
            }
        }
        Outlet::<Route> {}
        NowPlayingBar {}
        QueueSidebar {}
        GlobalDialog {}
    }
}
