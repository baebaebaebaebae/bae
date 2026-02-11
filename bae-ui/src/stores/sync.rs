//! Sync status state store

use dioxus::prelude::*;

/// Activity of a single remote device (display-only).
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct DeviceActivityInfo {
    pub device_id: String,
    pub last_seq: u64,
    /// RFC 3339 timestamp of when the device last synced.
    pub last_sync: Option<String>,
}

/// Sync status state for the UI.
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct SyncState {
    /// When this device last synced (RFC 3339). None if never synced.
    pub last_sync_time: Option<String>,
    /// Other devices' activity.
    pub other_devices: Vec<DeviceActivityInfo>,
    /// Whether a sync cycle is currently in progress.
    pub syncing: bool,
    /// Last sync error message, if any.
    pub error: Option<String>,
    /// User's Ed25519 public key (hex-encoded). None if no keypair exists.
    pub user_pubkey: Option<String>,
}
