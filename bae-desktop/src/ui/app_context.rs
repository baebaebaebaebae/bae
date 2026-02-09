//! Application context types for passing services through the Dioxus context boundary.
//!
//! Note: The main application service is `AppService` in `app_service.rs`.
//! This file contains the `AppServices` struct for passing backend service handles
//! from main.rs through the launch boundary.

use bae_core::cache;
use bae_core::config;
use bae_core::import;
use bae_core::keys::KeyService;
use bae_core::library::SharedLibraryManager;
use bae_core::playback;
#[cfg(feature = "torrent")]
use bae_core::torrent;

/// Service handles provided at app launch (Send + Sync safe).
///
/// These are the backend service handles that can be passed through
/// the context provider boundary. The reactive state is created separately
/// inside the Dioxus component tree by `AppService`.
#[derive(Clone)]
pub struct AppServices {
    /// Library manager for database operations
    pub library_manager: SharedLibraryManager,
    /// Application configuration
    pub config: config::Config,
    /// Import service handle for submitting imports
    pub import_handle: import::ImportServiceHandle,
    /// Playback service handle for audio control
    pub playback_handle: playback::PlaybackHandle,
    /// Cache manager for images/files
    pub cache: cache::CacheManager,
    /// Torrent manager (feature-gated)
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
    /// Key service for secret management
    pub key_service: KeyService,
}

// =============================================================================
// Legacy: AppContext for launch_app backwards compatibility
// =============================================================================

#[derive(Clone)]
pub struct AppContext {
    pub library_manager: SharedLibraryManager,
    pub config: config::Config,
    pub import_handle: import::ImportServiceHandle,
    pub playback_handle: playback::PlaybackHandle,
    pub cache: cache::CacheManager,
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
    pub key_service: KeyService,
    pub runtime_handle: tokio::runtime::Handle,
}
