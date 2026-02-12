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
    /// Subsonic server bind address (default: 127.0.0.1)
    pub subsonic_bind_address: String,

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

    /// Base URL for share links (e.g. "https://listen.example.com")
    pub share_base_url: Option<String>,
}
