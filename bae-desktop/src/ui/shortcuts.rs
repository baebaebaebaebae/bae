//! App-level keyboard shortcuts
//!
//! Maps Cmd+N (macOS) / Ctrl+N (Windows/Linux) to navigation actions.
//! Also provides a mechanism for native menus to request navigation.

use crate::ui::app_service::use_app;
use crate::ui::Route;
#[cfg(target_os = "macos")]
use bae_core::playback::RepeatMode;
use bae_ui::stores::{
    AppStateStoreExt, PlaybackUiStateStoreExt, SidebarStateStoreExt, UiStateStoreExt,
};
use dioxus::prelude::*;
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Navigation actions that can be triggered by shortcuts or menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Back,
    Forward,
    GoTo(NavTarget),
    GoToNowPlaying,
    ToggleQueueSidebar,
}

/// Navigation targets for direct routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    Library,
    Import,
    Settings,
}

impl NavTarget {
    pub fn to_route(self) -> Route {
        match self {
            NavTarget::Library => Route::Library {},
            NavTarget::Import => Route::ImportWorkflowManager {},
            NavTarget::Settings => Route::Settings {},
        }
    }
}

static NAV_SENDER: OnceLock<broadcast::Sender<NavAction>> = OnceLock::new();

#[cfg(target_os = "macos")]
static PLAYBACK_SENDER: OnceLock<broadcast::Sender<PlaybackAction>> = OnceLock::new();

static URL_SENDER: OnceLock<broadcast::Sender<String>> = OnceLock::new();

/// Initialize the navigation channel. Call once at startup.
pub fn init_nav_channel() {
    let (tx, _rx) = broadcast::channel(16);
    NAV_SENDER.set(tx).expect("nav channel already initialized");
}

/// Initialize the playback action channel. Call once at startup.
#[cfg(target_os = "macos")]
pub fn init_playback_channel() {
    let (tx, _rx) = broadcast::channel(16);
    PLAYBACK_SENDER
        .set(tx)
        .expect("playback channel already initialized");
}

/// Subscribe to navigation actions. Can be called multiple times (survives remounts).
pub fn subscribe_nav() -> broadcast::Receiver<NavAction> {
    NAV_SENDER
        .get()
        .expect("nav channel not initialized")
        .subscribe()
}

/// Subscribe to playback actions.
#[cfg(target_os = "macos")]
pub fn subscribe_playback_actions() -> broadcast::Receiver<PlaybackAction> {
    PLAYBACK_SENDER
        .get()
        .expect("playback channel not initialized")
        .subscribe()
}

/// Initialize the URL channel. Call once at startup.
pub fn init_url_channel() {
    let (tx, _rx) = broadcast::channel(8);
    URL_SENDER.set(tx).expect("url channel already initialized");
}

/// Send a URL received from the OS (called from Apple Event handler or CLI args).
pub fn send_url(url: String) {
    if let Some(tx) = URL_SENDER.get() {
        let _ = tx.send(url);
    }
}

/// Subscribe to incoming URLs.
pub fn subscribe_url() -> broadcast::Receiver<String> {
    URL_SENDER
        .get()
        .expect("url channel not initialized")
        .subscribe()
}

/// Request a navigation action (called from native menu handlers).
/// On macOS, dispatches to main thread via GCD.
#[cfg(target_os = "macos")]
pub fn request_nav(action: NavAction) {
    dispatch::Queue::main().exec_async(move || {
        if let Some(tx) = NAV_SENDER.get() {
            let _ = tx.send(action);
        }
    });
}

/// Request a playback action (called from native menu handlers).
/// On macOS, dispatches to main thread via GCD.
#[cfg(target_os = "macos")]
pub fn request_playback_action(action: PlaybackAction) {
    dispatch::Queue::main().exec_async(move || {
        if let Some(tx) = PLAYBACK_SENDER.get() {
            let _ = tx.send(action);
        }
    });
}

/// Playback actions that can be triggered by native menus.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAction {
    SetRepeatMode(RepeatMode),
    TogglePlayPause,
    Next,
    Previous,
}

/// Check if the platform modifier key is pressed (Cmd on macOS, Ctrl elsewhere).
#[cfg(not(target_os = "macos"))]
fn has_platform_modifier(evt: &KeyboardEvent) -> bool {
    let mods = evt.modifiers();
    mods.ctrl() && !mods.meta() && !mods.alt() && !mods.shift()
}

/// Check if platform modifier + Shift is pressed (Cmd+Shift on macOS, Ctrl+Shift elsewhere).
#[cfg(not(target_os = "macos"))]
fn has_platform_modifier_with_shift(evt: &KeyboardEvent) -> bool {
    let mods = evt.modifiers();
    mods.ctrl() && mods.shift() && !mods.meta() && !mods.alt()
}

/// Try to handle a keyboard event as an app shortcut.
/// Returns `Some(NavAction)` if the event matches a shortcut, `None` otherwise.
pub fn handle_shortcut(evt: &KeyboardEvent) -> Option<NavAction> {
    // On macOS, all shortcuts are handled by the native menu and won't reach the webview.
    #[cfg(target_os = "macos")]
    let _ = evt;
    #[cfg(not(target_os = "macos"))]
    {
        if has_platform_modifier_with_shift(evt) {
            match evt.key() {
                Key::Character(c) if c == "S" => {
                    return Some(NavAction::ToggleQueueSidebar);
                }
                _ => {}
            }
        }

        if has_platform_modifier(evt) {
            match evt.key() {
                Key::Character(c) if c == "1" => return Some(NavAction::GoTo(NavTarget::Library)),
                Key::Character(c) if c == "2" => return Some(NavAction::GoTo(NavTarget::Import)),
                Key::Character(c) if c == "3" => return Some(NavAction::GoTo(NavTarget::Settings)),
                Key::Character(c) if c == "l" => return Some(NavAction::GoToNowPlaying),
                Key::Character(c) if c == "[" => return Some(NavAction::Back),
                Key::Character(c) if c == "]" => return Some(NavAction::Forward),
                _ => {}
            }
        }
    }

    None
}

fn execute_nav_action(action: NavAction) {
    match action {
        NavAction::Back => navigator().go_back(),
        NavAction::Forward => navigator().go_forward(),
        NavAction::GoTo(target) => {
            let _ = navigator().push(target.to_route());
        }
        // Handled in ShortcutsHandler where we have access to app state
        NavAction::GoToNowPlaying | NavAction::ToggleQueueSidebar => {}
    }
}

#[component]
pub fn ShortcutsHandler(children: Element) -> Element {
    let app = use_app();

    // Listen for menu-triggered navigation (subscribes fresh on each mount)
    use_hook(|| {
        let library_manager = app.library_manager.clone();
        let playback = app.state.playback();
        let mut sidebar_is_open = app.state.ui().sidebar().is_open();
        let mut rx = subscribe_nav();
        spawn(async move {
            while let Ok(action) = rx.recv().await {
                match action {
                    NavAction::GoToNowPlaying => {
                        let release_id = playback.current_release_id().read().clone();
                        go_to_now_playing(&library_manager, release_id);
                    }
                    NavAction::ToggleQueueSidebar => {
                        let current = *sidebar_is_open.read();
                        sidebar_is_open.set(!current);
                    }
                    other => execute_nav_action(other),
                }
            }
        });
    });

    // "/" shortcut: register on document so it works regardless of focus.
    // The div-level onkeydown only fires when a descendant has focus,
    // which often isn't the case (focus defaults to body).
    use_hook(|| {
        let js = format!(
            r#"
            document.addEventListener('keydown', function(e) {{
                if (e.key === '/' && !e.metaKey && !e.ctrlKey && !e.altKey) {{
                    const tag = document.activeElement?.tagName;
                    if (tag !== 'INPUT' && tag !== 'TEXTAREA') {{
                        e.preventDefault();
                        const el = document.getElementById('{}');
                        if (el) {{ el.focus(); }}
                    }}
                }}
            }});
            "#,
            bae_ui::SEARCH_INPUT_ID,
        );
        dioxus::document::eval(&js);
    });

    let mut sidebar_is_open = app.state.ui().sidebar().is_open();

    let onkeydown = move |evt: KeyboardEvent| {
        if let Some(action) = handle_shortcut(&evt) {
            evt.prevent_default();
            match action {
                NavAction::GoToNowPlaying => {
                    let release_id = app.state.playback().current_release_id().read().clone();
                    go_to_now_playing(&app.library_manager, release_id);
                }
                NavAction::ToggleQueueSidebar => {
                    let current = *sidebar_is_open.read();
                    sidebar_is_open.set(!current);
                }
                other => execute_nav_action(other),
            }
        }
    };

    rsx! {
        div { class: "contents", onkeydown, {children} }
    }
}

/// Navigate to the album page of the currently playing track.
fn go_to_now_playing(
    library_manager: &bae_core::library::SharedLibraryManager,
    release_id: Option<String>,
) {
    if let Some(release_id) = release_id {
        let library_manager = library_manager.clone();
        spawn(async move {
            if let Ok(album_id) = library_manager
                .get()
                .get_album_id_for_release(&release_id)
                .await
            {
                navigator().push(Route::AlbumDetail {
                    album_id,
                    release_id,
                });
            }
        });
    }
}
