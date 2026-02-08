use crate::ui::components::import::ImportWorkflowManager;
use crate::ui::components::*;
use crate::ui::protocol_handler::{handle_protocol_request, ImageServices};
#[cfg(target_os = "macos")]
use crate::ui::window_activation::setup_macos_window_activation;
use crate::ui::AppContext;

use dioxus::desktop::{Config as DioxusConfig, WindowBuilder};
use dioxus::prelude::*;

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
    #[route("/album/:album_id?:release_id")]
    AlbumDetail { album_id: String, release_id: String },
    #[route("/artist/:artist_id")]
    ArtistDetail { artist_id: String },
    #[route("/import")]
    ImportWorkflowManager {},
    #[route("/settings")]
    Settings {},
}

pub fn make_config(context: &AppContext) -> DioxusConfig {
    let services = ImageServices {
        library_manager: context.library_manager.clone(),
        library_dir: context.config.library_dir.clone(),
    };

    DioxusConfig::default()
        .with_window(make_window())
        .with_background_color((0x0f, 0x11, 0x16, 0xff))
        .with_disable_drag_drop_handler(false)
        .with_custom_protocol("bae", move |_webview_id, request| {
            let uri = request.uri().to_string();
            handle_protocol_request(&uri, &services)
        })
}

fn make_window() -> WindowBuilder {
    WindowBuilder::new()
        .with_title("bae")
        .with_always_on_top(false)
        .with_decorations(true)
        .with_inner_size(dioxus::desktop::LogicalSize::new(1200, 800))
        .with_maximized(true)
        .with_transparent(true)
        .with_background_color((0x0f, 0x11, 0x16, 0xff))
}

pub fn launch_app(context: AppContext) {
    #[cfg(target_os = "macos")]
    {
        use crate::ui::window_activation::{setup_app_menu, setup_transparent_titlebar};
        setup_macos_window_activation();
        setup_transparent_titlebar();
        setup_app_menu();
    }

    // Create AppServices from AppContext (these are Send-safe)
    #[cfg(feature = "torrent")]
    let services = super::app_context::AppServices {
        library_manager: context.library_manager.clone(),
        config: context.config.clone(),
        import_handle: context.import_handle.clone(),
        playback_handle: context.playback_handle.clone(),
        cache: context.cache.clone(),
        torrent_manager: context.torrent_manager.clone(),
        key_service: context.key_service.clone(),
    };
    #[cfg(not(feature = "torrent"))]
    let services = super::app_context::AppServices {
        library_manager: context.library_manager.clone(),
        config: context.config.clone(),
        import_handle: context.import_handle.clone(),
        playback_handle: context.playback_handle.clone(),
        cache: context.cache.clone(),
        key_service: context.key_service.clone(),
    };

    LaunchBuilder::desktop()
        .with_cfg(make_config(&context))
        // Provide AppServices (Send-safe) - App struct is created inside component
        .with_context_provider(move || Box::new(services.clone()))
        .launch(App);
}
