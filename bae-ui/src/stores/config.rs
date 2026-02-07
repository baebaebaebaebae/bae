//! Application configuration state store

use dioxus::prelude::*;

/// Application configuration state
///
/// This mirrors the config values from bae_core::config::Config that are
/// relevant to the UI. The Store is updated when config changes.
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ConfigState {
    /// Whether a Discogs API key is stored (hint flag, avoids keyring read)
    pub discogs_key_stored: bool,
    /// Whether an encryption key is stored (hint flag, avoids keyring read)
    pub encryption_key_stored: bool,
    /// SHA-256 fingerprint of the encryption key (for display and validation)
    pub encryption_key_fingerprint: Option<String>,

    // Subsonic settings
    /// Whether the Subsonic API server is enabled
    pub subsonic_enabled: bool,
    /// Subsonic server port
    pub subsonic_port: u16,

    // BitTorrent settings
    /// Interface to bind torrent client to
    pub torrent_bind_interface: Option<String>,
    /// Port for incoming torrent connections (None = random)
    pub torrent_listen_port: Option<u16>,
    /// Enable UPnP port forwarding
    pub torrent_enable_upnp: bool,
    /// Global max connections (None = unlimited)
    pub torrent_max_connections: Option<i32>,
    /// Max connections per torrent (None = unlimited)
    pub torrent_max_connections_per_torrent: Option<i32>,
    /// Global max upload slots (None = unlimited)
    pub torrent_max_uploads: Option<i32>,
    /// Max upload slots per torrent (None = unlimited)
    pub torrent_max_uploads_per_torrent: Option<i32>,

    // Cloud sync settings
    /// Whether cloud sync is enabled
    pub cloud_sync_enabled: bool,
    /// S3 bucket for cloud sync
    pub cloud_sync_bucket: Option<String>,
    /// S3 region for cloud sync
    pub cloud_sync_region: Option<String>,
    /// S3 endpoint for cloud sync (custom endpoint for MinIO etc.)
    pub cloud_sync_endpoint: Option<String>,
    /// Last successful cloud sync upload (ISO 8601)
    pub cloud_sync_last_upload: Option<String>,
    /// Current cloud sync status
    pub cloud_sync_status: CloudSyncStatus,
}

/// Status of cloud sync operations
#[derive(Clone, Debug, PartialEq)]
pub enum CloudSyncStatus {
    Idle,
    Syncing,
    Error(String),
}

#[allow(clippy::derivable_impls)]
impl Default for CloudSyncStatus {
    fn default() -> Self {
        Self::Idle
    }
}
