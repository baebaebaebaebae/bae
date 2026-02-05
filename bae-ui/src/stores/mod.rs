//! Store types for UI state management
//!
//! These stores hold UI state that can be shared between bae-desktop (real app)
//! and bae-mocks (design tool). Each store derives `Store` for fine-grained
//! reactivity via lensing.

pub mod active_imports;
pub mod album_detail;
pub mod app;
pub mod artist_detail;
pub mod config;
pub mod import;
pub mod library;
pub mod playback;
pub mod storage_profiles;
pub mod ui;

pub use active_imports::*;
pub use album_detail::*;
pub use app::*;
pub use artist_detail::*;
pub use config::*;
pub use import::*;
pub use library::*;
pub use playback::*;
pub use storage_profiles::*;
pub use ui::*;
