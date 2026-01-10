pub mod app;
pub mod app_context;
pub mod components;
#[cfg(feature = "demo")]
pub mod demo_app;
#[cfg(feature = "demo")]
pub mod demo_data;
pub mod display_types;
pub mod import_context;
pub mod local_file_url;
#[cfg(target_os = "macos")]
pub mod window_activation;
pub use app::*;
pub use app_context::*;
pub use local_file_url::{image_url, local_file_url};

/// Delay before showing loading spinners to avoid flicker on fast operations
pub const LOADING_SPINNER_DELAY_MS: u64 = 250;
