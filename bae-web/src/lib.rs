pub mod api;
pub mod pages;

use dioxus::prelude::*;
use pages::{AlbumDetail, AppLayout, Library};

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
pub const FLOATING_UI_CORE: Asset = asset!("/assets/floating-ui.core.min.js");
pub const FLOATING_UI_DOM: Asset = asset!("/assets/floating-ui.dom.min.js");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id")]
    AlbumDetail { album_id: String },
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: FLOATING_UI_CORE }
        document::Script { src: FLOATING_UI_DOM }
        div { class: "min-h-screen", Router::<Route> {} }
    }
}
