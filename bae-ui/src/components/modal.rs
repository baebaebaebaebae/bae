//! Modal component using native HTML `<dialog>` element
//!
//! Uses `showModal()` for browser-native:
//! - Top-layer rendering (no z-index needed)
//! - Focus trap
//! - Escape key to close
//! - `::backdrop` styling
//!
//! The native `<dialog>` element handles its own visibility (display: none when closed,
//! block when open via showModal). We don't override display - instead we use an inner
//! fixed container for layout.
//!
//! Note: Unlike popover's `ontoggle`, dialog's `oncancel` only fires from user actions
//! (Escape key), not from programmatic `close()` calls, so we don't need the same
//! complexity as Dropdown. However, `showModal()` throws if already open, so we check
//! the `open` attribute for idempotency.

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use wasm_bindgen_x::JsCast;

/// Counter for generating unique modal IDs
static MODAL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Modal component that wraps content in a native `<dialog>` element
#[component]
pub fn Modal(
    /// Controls whether the modal is open
    is_open: ReadSignal<bool>,
    /// Called when the modal should close (Escape key or backdrop click)
    on_close: EventHandler<()>,
    /// Modal content
    children: Element,
    /// Optional CSS class for the dialog element
    #[props(default)]
    class: Option<String>,
) -> Element {
    // Generate unique ID for this dialog instance using atomic counter
    let dialog_id = use_hook(|| {
        let id = MODAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("modal-{}", id)
    });
    let dialog_id_for_effect = dialog_id.clone();
    let dialog_id_for_rsx = dialog_id.clone();

    // Control dialog open/close state via showModal()/close()
    use_effect(move || {
        let is_open = is_open();

        let Some(window) = web_sys_x::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(element) = document.get_element_by_id(&dialog_id_for_effect) else {
            return;
        };

        // Check current state for idempotency (effect may run multiple times)
        let is_dialog_open = element.has_attribute("open");

        if is_open {
            if is_dialog_open {
                return;
            }
            if let Ok(show_modal) = js_sys_x::Reflect::get(&element, &"showModal".into()) {
                if let Some(func) = show_modal.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&element);
                }
            }
        } else {
            if !is_dialog_open {
                return;
            }
            if let Ok(close) = js_sys_x::Reflect::get(&element, &"close".into()) {
                if let Some(func) = close.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&element);
                }
            }
        }
    });

    let dialog_class = class.unwrap_or_default();

    // Native <dialog> handles its own display (none when closed, block when open).
    // IMPORTANT: Do NOT add display-related classes (flex, block, grid, etc.) to the
    // dialog element - they will override the native display:none and make the dialog
    // visible even when closed. Use the inner container for layout instead.
    // The ::backdrop pseudo-element provides the overlay styling.
    rsx! {
        dialog {
            id: "{dialog_id_for_rsx}",
            class: "p-0 bg-transparent backdrop:bg-black/80 {dialog_class}",
            // Escape key fires 'cancel' event
            oncancel: move |evt| {
                evt.prevent_default();
                on_close.call(());
            },
            // Only render children when open - no need for DOM when dialog is closed
            if is_open() {
                // Fixed container for backdrop click handling and content centering
                div {
                    class: "fixed inset-0 flex items-center justify-center",
                    onclick: move |_| on_close.call(()),
                    // Inner wrapper prevents click propagation so content clicks don't close
                    div { onclick: move |evt| evt.stop_propagation(), {children} }
                }
            }
        }
    }
}
