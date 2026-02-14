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

/// Status of an invite operation.
#[derive(Clone, Debug, PartialEq)]
pub enum InviteStatus {
    Sending,
    Success,
    Error(String),
}

/// Cloud home coordinates to share with an invitee after a successful invite.
#[derive(Clone, Debug, PartialEq)]
pub struct ShareInfo {
    pub cloud_home_bucket: String,
    pub cloud_home_region: String,
    pub cloud_home_endpoint: Option<String>,
    pub invitee_pubkey: String,
}

/// A release shared with us via a share grant (display-only).
#[derive(Clone, Debug, PartialEq)]
pub struct SharedReleaseDisplay {
    pub grant_id: String,
    pub release_id: String,
    pub from_library_id: String,
    pub from_user_pubkey: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub expires: Option<String>,
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

    // Cloud home configuration (mirrors Config, for UI display)
    /// S3 bucket name for cloud home.
    pub cloud_home_bucket: Option<String>,
    /// S3 region for cloud home.
    pub cloud_home_region: Option<String>,
    /// S3 endpoint for cloud home.
    pub cloud_home_endpoint: Option<String>,
    /// Whether cloud home is fully configured (bucket + region + credentials present).
    pub cloud_home_configured: bool,

    // Invite flow state
    /// Current invite operation status.
    pub invite_status: Option<InviteStatus>,
    /// Share info shown after a successful invite.
    pub share_info: Option<ShareInfo>,

    // Remove member flow state
    /// Whether a member removal is in progress.
    pub removing_member: bool,
    /// Error from a member removal attempt.
    pub remove_member_error: Option<String>,

    // Shared releases
    /// Releases shared with us via accepted share grants.
    pub shared_releases: Vec<SharedReleaseDisplay>,
}
