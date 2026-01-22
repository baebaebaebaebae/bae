//! WASM utilities for browser interop
//!
//! # Event Listener Cleanup Pattern
//!
//! In Rust/WASM, when you attach a JavaScript event listener using a `Closure`,
//! you need to ensure the closure lives as long as the listener is attached.
//! The naive approach is `closure.forget()`, but this leaks memory and leaves
//! the listener attached forever.
//!
//! The standard pattern is to store the closure in a struct that implements `Drop`,
//! removing the listener when the struct is dropped. This ties the listener lifetime
//! to Rust's ownership system:
//!
//! ```ignore
//! // Listener is attached when DocumentEventListener is created
//! let listener = DocumentEventListener::new(document, "click", callback);
//!
//! // Listener is automatically removed when `listener` goes out of scope or is dropped
//! drop(listener);
//! ```
//!
//! This is particularly useful with Dioxus signals—store the listener in a
//! `Signal<Option<DocumentEventListener>>` and set it to `None` to remove the listener.

use wasm_bindgen_x::prelude::*;

/// A document event listener that automatically removes itself when dropped.
///
/// This provides RAII-style cleanup for JavaScript event listeners, preventing
/// memory leaks and dangling listeners that can occur with `Closure::forget()`.
pub struct DocumentEventListener {
    document: web_sys_x::Document,
    event_name: &'static str,
    callback: Closure<dyn FnMut(wasm_bindgen_x::JsValue)>,
}

impl DocumentEventListener {
    /// Attaches an event listener to the document.
    ///
    /// The listener is automatically removed when this struct is dropped.
    pub fn new(
        document: web_sys_x::Document,
        event_name: &'static str,
        callback: impl FnMut(wasm_bindgen_x::JsValue) + 'static,
    ) -> Self {
        let callback: Closure<dyn FnMut(wasm_bindgen_x::JsValue)> =
            Closure::wrap(Box::new(callback));

        document
            .add_event_listener_with_callback(event_name, callback.as_ref().unchecked_ref())
            .ok();

        Self {
            document,
            event_name,
            callback,
        }
    }
}

impl Drop for DocumentEventListener {
    fn drop(&mut self) {
        let _ = self.document.remove_event_listener_with_callback(
            self.event_name,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}
