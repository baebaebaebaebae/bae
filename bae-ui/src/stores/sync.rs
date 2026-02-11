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

/// Role of a library member (display-only, shadows bae-core's MemberRole).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Member,
}

/// A library member for display in the sync settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    /// Ed25519 public key (hex-encoded).
    pub pubkey: String,
    /// Display name (or truncated pubkey if no name is set).
    pub display_name: String,
    /// Role in the library.
    pub role: MemberRole,
    /// Whether this member is the current user.
    pub is_self: bool,
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
    /// Current library members (from membership chain). Empty if solo or not syncing.
    pub members: Vec<Member>,

    // Sync bucket configuration (mirrors Config, for UI display)
    /// S3 bucket name for sync.
    pub sync_bucket: Option<String>,
    /// S3 region for sync bucket.
    pub sync_region: Option<String>,
    /// S3 endpoint for sync bucket.
    pub sync_endpoint: Option<String>,
    /// Whether sync is fully configured (bucket + region + credentials present).
    pub sync_configured: bool,
}
