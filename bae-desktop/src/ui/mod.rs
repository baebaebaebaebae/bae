pub mod app;
pub mod app_context;
pub mod app_service;
pub mod components;
pub mod display_types;
pub mod import_helpers;
pub mod local_file_url;
mod protocol_handler;
pub mod shortcuts;
#[cfg(target_os = "macos")]
pub mod window_activation;
pub use app::*;
// Legacy re-exports for backwards compatibility
pub use app_context::AppContext;
pub use local_file_url::cover_url;
