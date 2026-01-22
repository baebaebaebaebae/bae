//! Overlay renderer - renders the stack of overlays from Store
//!
//! This component reads from `app.state.ui().overlays()` and renders each
//! overlay layer in the stack. It handles:
//! - Confirmation dialogs
//! - Modals (release info, etc.)
//! - Dropdown menus
//!
//! Overlays can configure:
//! - Whether clicking backdrop dismisses them
//! - Whether backdrop blocks pointer events

use crate::ui::app_service::use_app;
use bae_ui::stores::{
    AppStateStoreExt, ConfirmAction, DropdownMenuType, OverlayContent, OverlayLayer,
    UiStateStoreExt,
};
use dioxus::prelude::*;
use tracing::error;

/// Renders the overlay stack from Store
#[component]
pub fn OverlayRenderer() -> Element {
    let app = use_app();
    let overlays = app.state.ui().overlays().read().clone();

    rsx! {
        for layer in overlays {
            OverlayLayerRenderer { key: "{layer.id}", layer: layer.clone() }
        }
    }
}

/// Renders a single overlay layer
#[component]
fn OverlayLayerRenderer(layer: OverlayLayer) -> Element {
    let app = use_app();
    let layer_id = layer.id.clone();

    let backdrop_class = if layer.blocks_pointer_events {
        "fixed inset-0 bg-black/50 flex items-center justify-center z-[3000]"
    } else {
        "fixed inset-0 z-[3000]"
    };

    let on_backdrop_click = {
        let app = app.clone();
        let layer_id = layer_id.clone();
        let dismiss_on_backdrop = layer.dismiss_on_backdrop;
        move |_| {
            if dismiss_on_backdrop {
                app.pop_overlay_by_id(&layer_id);
            }
        }
    };

    match &layer.content {
        OverlayContent::ConfirmDialog {
            title,
            message,
            confirm_label,
            cancel_label,
            on_confirm_action,
        } => {
            let title = title.clone();
            let message = message.clone();
            let confirm_label = confirm_label.clone();
            let cancel_label = cancel_label.clone();
            let action = on_confirm_action.clone();

            rsx! {
                div { class: "{backdrop_class}", onclick: on_backdrop_click,
                    ConfirmDialogContent {
                        layer_id: layer_id.clone(),
                        title,
                        message,
                        confirm_label,
                        cancel_label,
                        action,
                    }
                }
            }
        }
        OverlayContent::ReleaseInfoModal { release_id } => {
            let release_id = release_id.clone();
            rsx! {
                div { class: "{backdrop_class}", onclick: on_backdrop_click,
                    ReleaseInfoModalContent { layer_id: layer_id.clone(), release_id }
                }
            }
        }
        OverlayContent::Dropdown {
            anchor_id,
            menu_type,
        } => {
            let anchor_id = anchor_id.clone();
            let menu_type = menu_type.clone();
            rsx! {
                div {
                    class: "fixed inset-0 z-[3000]",
                    onclick: on_backdrop_click,
                    DropdownContent {
                        layer_id: layer_id.clone(),
                        anchor_id,
                        menu_type,
                    }
                }
            }
        }
    }
}

/// Confirmation dialog content
#[component]
fn ConfirmDialogContent(
    layer_id: String,
    title: String,
    message: String,
    confirm_label: String,
    cancel_label: String,
    action: ConfirmAction,
) -> Element {
    let app = use_app();
    let is_processing = use_signal(|| false);

    let on_cancel = {
        let app = app.clone();
        let layer_id = layer_id.clone();
        move |_| {
            app.pop_overlay_by_id(&layer_id);
        }
    };

    let on_confirm = {
        let app = app.clone();
        let layer_id = layer_id.clone();
        let action = action.clone();
        let mut is_processing = is_processing;
        move |_| {
            if is_processing() {
                return;
            }
            is_processing.set(true);

            let app = app.clone();
            let layer_id = layer_id.clone();
            let action = action.clone();

            spawn(async move {
                match action {
                    ConfirmAction::DeleteAlbum { album_id } => {
                        if let Err(e) = app.library_manager.get().delete_album(&album_id).await {
                            error!("Failed to delete album: {}", e);
                        }
                    }
                    ConfirmAction::DeleteRelease { release_id } => {
                        if let Err(e) = app.library_manager.get().delete_release(&release_id).await
                        {
                            error!("Failed to delete release: {}", e);
                        }
                    }
                    ConfirmAction::DeleteStorageProfile { profile_id } => {
                        if let Err(e) = app
                            .library_manager
                            .delete_storage_profile(&profile_id)
                            .await
                        {
                            error!("Failed to delete storage profile: {}", e);
                        }
                    }
                }
                app.pop_overlay_by_id(&layer_id);
            });
        }
    };

    rsx! {
        div {
            class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
            onclick: move |evt| evt.stop_propagation(),

            h2 { class: "text-xl font-bold text-white mb-4", "{title}" }
            p { class: "text-gray-300 mb-6", "{message}" }

            div { class: "flex gap-3 justify-end",
                button {
                    class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg",
                    disabled: is_processing(),
                    onclick: on_cancel,
                    "{cancel_label}"
                }
                button {
                    class: "px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg",
                    disabled: is_processing(),
                    onclick: on_confirm,
                    if is_processing() {
                        "Processing..."
                    } else {
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

/// Release info modal content - placeholder, will be migrated from view.rs
#[component]
fn ReleaseInfoModalContent(layer_id: String, release_id: String) -> Element {
    let app = use_app();

    let on_close = {
        let app = app.clone();
        let layer_id = layer_id.clone();
        move |_| {
            app.pop_overlay_by_id(&layer_id);
        }
    };

    // TODO: Load release data and render ReleaseInfoModal from bae-ui
    // For now, render a placeholder
    rsx! {
        div {
            class: "bg-gray-800 rounded-lg p-6 max-w-2xl w-full mx-4",
            onclick: move |evt| evt.stop_propagation(),

            div { class: "flex justify-between items-center mb-4",
                h2 { class: "text-xl font-bold text-white", "Release Info" }
                button {
                    class: "text-gray-400 hover:text-white",
                    onclick: on_close,
                    "Close"
                }
            }
            p { class: "text-gray-300", "Release ID: {release_id}" }
        }
    }
}

/// Dropdown content - placeholder for now
#[component]
fn DropdownContent(layer_id: String, anchor_id: String, menu_type: DropdownMenuType) -> Element {
    let app = use_app();

    let on_close = {
        let app = app.clone();
        let layer_id = layer_id.clone();
        move |_| {
            app.pop_overlay_by_id(&layer_id);
        }
    };

    // TODO: Position dropdown relative to anchor and render appropriate menu
    // For now, render a placeholder
    rsx! {
        div {
            class: "fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg p-4 shadow-lg",
            onclick: move |evt| evt.stop_propagation(),
            p { class: "text-white mb-2", "Dropdown for anchor: {anchor_id}" }
            button { class: "text-gray-400 hover:text-white", onclick: on_close, "Close" }
        }
    }
}
