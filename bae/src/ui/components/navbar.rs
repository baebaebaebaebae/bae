use super::dialog::GlobalDialog;
#[cfg(not(feature = "demo"))]
use super::now_playing_bar::NowPlayingBar;
#[cfg(not(feature = "demo"))]
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
        {
            #[cfg(not(feature = "demo"))]
            {
                rsx! {
                    NowPlayingBar {}
                    QueueSidebar {}
                }
            }
        }
        GlobalDialog {}
    }
}
