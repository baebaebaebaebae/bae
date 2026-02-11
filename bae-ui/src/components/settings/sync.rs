//! Sync status and configuration section view

use crate::components::icons::{CheckIcon, CopyIcon};
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChromelessButton, SettingsCard, SettingsSection, TextInput,
    TextInputSize, TextInputType,
};
use crate::stores::{DeviceActivityInfo, InviteStatus, Member, MemberRole, ShareInfo};
use dioxus::prelude::*;

/// Data bundle for sync bucket configuration fields (avoids 5 separate EventHandler props for save).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncBucketConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
}

/// Sync status and configuration section view (pure, props-based).
#[component]
pub fn SyncSectionView(
    // --- Status props ---
    /// When this device last synced (RFC 3339). None if never synced.
    last_sync_time: Option<String>,
    /// Other devices' sync activity.
    other_devices: Vec<DeviceActivityInfo>,
    /// Whether a sync is currently in progress.
    syncing: bool,
    /// Last sync error, if any.
    error: Option<String>,
    /// User's Ed25519 public key (hex). None if no keypair exists.
    user_pubkey: Option<String>,
    /// Called when the user clicks the copy button on their public key.
    on_copy_pubkey: EventHandler<()>,

    // --- Config display props ---
    /// Current configured bucket name (from store). None if not configured.
    sync_bucket: Option<String>,
    /// Current configured region (from store).
    sync_region: Option<String>,
    /// Current configured endpoint (from store).
    sync_endpoint: Option<String>,
    /// Whether sync is fully configured (bucket + region + credentials).
    sync_configured: bool,

    // --- Edit state props ---
    /// Whether currently editing the sync config.
    is_editing: bool,
    /// Edit field: bucket name.
    edit_bucket: String,
    /// Edit field: region.
    edit_region: String,
    /// Edit field: endpoint.
    edit_endpoint: String,
    /// Edit field: access key.
    edit_access_key: String,
    /// Edit field: secret key.
    edit_secret_key: String,
    /// Whether a save is in progress.
    is_saving: bool,
    /// Error from a save attempt.
    save_error: Option<String>,

    // --- Test connection state ---
    /// Whether a connection test is in progress.
    is_testing: bool,
    /// Success message from a connection test.
    test_success: Option<String>,
    /// Error message from a connection test.
    test_error: Option<String>,

    // --- Members props ---
    /// Current library members from membership chain. Empty if solo/not syncing.
    members: Vec<Member>,
    /// Whether the current user is an owner (controls visibility of invite/remove).
    is_owner: bool,
    /// Called when the user clicks "Remove" on a member. Carries the member's pubkey.
    on_remove_member: EventHandler<String>,

    // --- Invite props ---
    /// Whether the invite form is open.
    show_invite_form: bool,
    /// Invite form: invitee's public key input.
    invite_pubkey: String,
    /// Invite form: selected role for the invitee.
    invite_role: MemberRole,
    /// Invite operation status.
    invite_status: Option<InviteStatus>,
    /// Share info to display after successful invite.
    share_info: Option<ShareInfo>,

    // --- Callbacks ---
    on_sync_now: EventHandler<()>,
    on_edit_start: EventHandler<()>,
    on_cancel_edit: EventHandler<()>,
    on_save_config: EventHandler<SyncBucketConfig>,
    on_test_connection: EventHandler<()>,
    on_bucket_change: EventHandler<String>,
    on_region_change: EventHandler<String>,
    on_endpoint_change: EventHandler<String>,
    on_access_key_change: EventHandler<String>,
    on_secret_key_change: EventHandler<String>,

    // --- Invite callbacks ---
    /// Toggle the invite form open/closed.
    on_toggle_invite_form: EventHandler<()>,
    /// Invite pubkey input changed.
    on_invite_pubkey_change: EventHandler<String>,
    /// Invite role selection changed.
    on_invite_role_change: EventHandler<MemberRole>,
    /// Submit the invite. Carries (pubkey, role).
    on_invite_member: EventHandler<(String, MemberRole)>,
    /// Copy share info text to clipboard. Carries the formatted text.
    on_copy_share_info: EventHandler<String>,
    /// Dismiss the share info panel.
    on_dismiss_share_info: EventHandler<()>,
) -> Element {
    let mut copied = use_signal(|| false);
    let mut share_copied = use_signal(|| false);

    let handle_copy = move |_| {
        on_copy_pubkey.call(());
        copied.set(true);
        spawn(async move {
            sleep_ms(2000).await;
            copied.set(false);
        });
    };

    let has_required_fields = !edit_bucket.is_empty()
        && !edit_region.is_empty()
        && !edit_access_key.is_empty()
        && !edit_secret_key.is_empty();

    let is_valid_invite_pubkey =
        invite_pubkey.len() == 64 && invite_pubkey.chars().all(|c| c.is_ascii_hexdigit());
    let is_inviting = matches!(invite_status, Some(InviteStatus::Sending));

    rsx! {
        SettingsSection {
            h2 { class: "text-xl font-semibold text-white", "Sync" }

            // Your identity
            if let Some(ref pubkey) = user_pubkey {
                SettingsCard {
                    h3 { class: "text-lg font-medium text-white mb-4", "Your identity" }
                    div { class: "flex items-center gap-3",
                        span { class: "text-gray-400 font-mono text-sm truncate",
                            {truncate_pubkey(pubkey)}
                        }
                        ChromelessButton {
                            class: Some("text-gray-400 hover:text-white transition-colors".to_string()),
                            title: Some("Copy public key to clipboard".to_string()),
                            aria_label: Some("Copy public key to clipboard".to_string()),
                            onclick: handle_copy,
                            if *copied.read() {
                                CheckIcon { class: "w-4 h-4 text-green-400" }
                            } else {
                                CopyIcon { class: "w-4 h-4" }
                            }
                        }
                    }
                }
            }

            // Sync status card
            SettingsCard {
                h3 { class: "text-lg font-medium text-white mb-4", "Status" }
                div { class: "space-y-3",

                    // Current device sync status
                    div { class: "flex justify-between items-center",
                        span { class: "text-gray-400", "Last synced" }
                        span { class: "text-white",
                            if syncing {
                                "Syncing..."
                            } else if let Some(ref ts) = last_sync_time {
                                {format_relative_time(ts).as_str()}
                            } else {
                                "Never"
                            }
                        }
                    }

                    div { class: "flex justify-between items-center",
                        span { class: "text-gray-400", "Sync bucket" }
                        if sync_configured {
                            span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm",
                                "Configured"
                            }
                        } else {
                            span { class: "px-3 py-1 bg-gray-700 text-gray-400 rounded-full text-sm",
                                "Not configured"
                            }
                        }
                    }

                    // Error display
                    if let Some(ref err) = error {
                        div { class: "text-red-400 text-sm", "{err}" }
                    }
                }

                div { class: "mt-4",
                    Button {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Small,
                        disabled: syncing || !sync_configured,
                        loading: syncing,
                        onclick: move |_| on_sync_now.call(()),
                        if syncing {
                            "Syncing..."
                        } else {
                            "Sync Now"
                        }
                    }
                }
            }

            // Other devices
            if !other_devices.is_empty() {
                SettingsCard {
                    h3 { class: "text-lg font-medium text-white mb-4", "Other devices" }
                    div { class: "space-y-2",
                        for device in other_devices.iter() {
                            div {
                                key: "{device.device_id}",
                                class: "flex justify-between items-center py-1",
                                span { class: "text-gray-400 font-mono text-sm truncate mr-4",
                                    {short_device_id(&device.device_id)}
                                }
                                span { class: "text-gray-300 text-sm flex-shrink-0",
                                    if let Some(ref ts) = device.last_sync {
                                        {format_relative_time(ts).as_str()}
                                    } else {
                                        "Unknown"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Members card (shown when sync is configured)
            if sync_configured {
                SettingsCard {
                    div { class: "flex items-center justify-between mb-4",
                        h3 { class: "text-lg font-medium text-white", "Members" }
                        if is_owner && !show_invite_form {
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Small,
                                onclick: move |_| on_toggle_invite_form.call(()),
                                "Invite Member"
                            }
                        }
                    }

                    if !members.is_empty() {
                        {
                            let owner_count = members.iter().filter(|m| m.role == MemberRole::Owner).count();
                            rsx! {
                                div { class: "space-y-2",
                                    for member in members.iter() {
                                        {
                                            let can_remove = is_owner && !member.is_self
                                                && !(member.role == MemberRole::Owner && owner_count <= 1);
                                            let pubkey = member.pubkey.clone();
                                            rsx! {
                                                div { key: "{member.pubkey}", class: "flex justify-between items-center py-1.5",
                                                    div { class: "flex items-center gap-3 min-w-0",
                                                        span { class: "text-gray-200 text-sm truncate",
                                                            "{member.display_name}"
                                                            if member.is_self {
                                                                span { class: "text-gray-500 ml-1", "(you)" }
                                                            }
                                                        }
                                                        match member.role {
                                                            MemberRole::Owner => rsx! {
                                                                span { class: "px-2 py-0.5 bg-amber-900/60 text-amber-300 rounded text-xs font-medium flex-shrink-0",
                                                                    "Owner"
                                                                }
                                                            },
                                                            MemberRole::Member => rsx! {
                                                                span { class: "px-2 py-0.5 bg-gray-700 text-gray-400 rounded text-xs font-medium flex-shrink-0",
                                                                    "Member"
                                                                }
                                                            },
                                                        }
                                                    }
                                                    if can_remove {
                                                        Button {
                                                            variant: ButtonVariant::Secondary,
                                                            size: ButtonSize::Small,
                                                            onclick: move |_| on_remove_member.call(pubkey.clone()),
                                                            "Remove"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        p { class: "text-sm text-gray-500 mb-2",
                            "No members yet. Invite someone to share this library."
                        }
                    }

                    // Invite form (inline, below member list)
                    if show_invite_form {
                        div { class: "mt-4 pt-4 border-t border-gray-700",
                            h4 { class: "text-sm font-medium text-gray-300 mb-3",
                                "Invite a new member"
                            }
                            div { class: "space-y-3",
                                div {
                                    label { class: "block text-sm text-gray-400 mb-1",
                                        "Public key (64-character hex)"
                                    }
                                    TextInput {
                                        value: invite_pubkey.clone(),
                                        on_input: move |v| on_invite_pubkey_change.call(v),
                                        size: TextInputSize::Medium,
                                        input_type: TextInputType::Text,
                                        placeholder: "Paste invitee's Ed25519 public key",
                                    }
                                }

                                div {
                                    label { class: "block text-sm text-gray-400 mb-1",
                                        "Role"
                                    }
                                    div { class: "flex gap-2",
                                        Button {
                                            variant: if invite_role == MemberRole::Member { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                                            size: ButtonSize::Small,
                                            onclick: move |_| on_invite_role_change.call(MemberRole::Member),
                                            "Member"
                                        }
                                        Button {
                                            variant: if invite_role == MemberRole::Owner { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                                            size: ButtonSize::Small,
                                            onclick: move |_| on_invite_role_change.call(MemberRole::Owner),
                                            "Owner"
                                        }
                                    }
                                }

                                // Invite status messages
                                if matches!(invite_status, Some(InviteStatus::Success)) {
                                    div { class: "p-3 bg-green-900/30 border border-green-700 rounded-lg text-sm text-green-300",
                                        "Invitation sent successfully."
                                    }
                                }

                                if let Some(InviteStatus::Error(ref err)) = invite_status {
                                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                        "{err}"
                                    }
                                }

                                div { class: "flex gap-3",
                                    {
                                        let pk = invite_pubkey.clone();
                                        let role = invite_role.clone();
                                        rsx! {
                                            Button {
                                                variant: ButtonVariant::Primary,
                                                size: ButtonSize::Medium,
                                                disabled: !is_valid_invite_pubkey || is_inviting,
                                                loading: is_inviting,
                                                onclick: move |_| on_invite_member.call((pk.clone(), role.clone())),
                                                if is_inviting {
                                                    "Inviting..."
                                                } else {
                                                    "Invite"
                                                }
                                            }
                                        }
                                    }
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Medium,
                                        disabled: is_inviting,
                                        onclick: move |_| on_toggle_invite_form.call(()),
                                        "Cancel"
                                    }
                                }
                            }
                        }
                    }

                    // Share info panel (shown after successful invite)
                    if let Some(ref info) = share_info {
                        {
                            let share_text = format_share_text(info);
                            rsx! {
                                div { class: "mt-4 pt-4 border-t border-gray-700",

                                    h4 { class: "text-sm font-medium text-gray-300 mb-3", "Share these details with the invitee" }
                                    div { class: "p-3 bg-gray-700/50 rounded-lg space-y-2 text-sm",
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-400", "Bucket" }
                                            span { class: "text-gray-200 font-mono", "{info.bucket}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-400", "Region" }
                                            span { class: "text-gray-200 font-mono", "{info.region}" }
                                        }
                                        if let Some(ref ep) = info.endpoint {
                                            div { class: "flex justify-between",
                                                span { class: "text-gray-400", "Endpoint" }
                                                span { class: "text-gray-200 font-mono", "{ep}" }
                                            }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-400", "Invitee key" }
                                            span { class: "text-gray-200 font-mono", {truncate_pubkey(&info.invitee_pubkey)} }
                                        }
                                    }
        
                                    div { class: "flex gap-3 mt-3",
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            size: ButtonSize::Small,
                                            onclick: {
                                                let text = share_text.clone();
                                                move |_| {
                                                    on_copy_share_info.call(text.clone());
                                                    share_copied.set(true);
                                                    spawn(async move {
                                                        sleep_ms(2000).await;
                                                        share_copied.set(false);
                                                    });
                                                }
                                            },
                                            if *share_copied.read() {
                                                "Copied"
                                            } else {
                                                "Copy to clipboard"
                                            }
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            size: ButtonSize::Small,
                                            onclick: move |_| on_dismiss_share_info.call(()),
                                            "Dismiss"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Sync bucket configuration card
            SettingsCard {
                div { class: "flex items-center justify-between mb-4",
                    div {
                        h3 { class: "text-lg font-medium text-white", "Sync Bucket" }
                        p { class: "text-sm text-gray-400 mt-1",
                            "S3-compatible bucket for syncing your library across devices"
                        }
                    }
                    if !is_editing {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_edit_start.call(()),
                            if sync_configured {
                                "Edit"
                            } else {
                                "Configure"
                            }
                        }
                    }
                }

                if is_editing {
                    div { class: "space-y-4",
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Bucket"
                            }
                            TextInput {
                                value: edit_bucket.to_string(),
                                on_input: move |v| on_bucket_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "my-sync-bucket",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Region"
                            }
                            TextInput {
                                value: edit_region.to_string(),
                                on_input: move |v| on_region_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "us-east-1",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Endpoint (optional)"
                            }
                            TextInput {
                                value: edit_endpoint.to_string(),
                                on_input: move |v| on_endpoint_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "https://s3.example.com",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Access Key"
                            }
                            TextInput {
                                value: edit_access_key.to_string(),
                                on_input: move |v| on_access_key_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "AKIA...",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Secret Key"
                            }
                            TextInput {
                                value: edit_secret_key.to_string(),
                                on_input: move |v| on_secret_key_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Password,
                                placeholder: "Secret key",
                            }
                        }

                        if let Some(ref err) = save_error {
                            div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                "{err}"
                            }
                        }

                        // Test connection result
                        if let Some(ref msg) = test_success {
                            div { class: "p-3 bg-green-900/30 border border-green-700 rounded-lg text-sm text-green-300",
                                "{msg}"
                            }
                        }

                        if let Some(ref err) = test_error {
                            div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                "{err}"
                            }
                        }

                        div { class: "flex gap-3",
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Medium,
                                disabled: !has_required_fields || is_saving,
                                loading: is_saving,
                                onclick: {
                                    let config = SyncBucketConfig {
                                        bucket: edit_bucket.to_string(),
                                        region: edit_region.to_string(),
                                        endpoint: edit_endpoint.to_string(),
                                        access_key: edit_access_key.to_string(),
                                        secret_key: edit_secret_key.to_string(),
                                    };
                                    move |_| on_save_config.call(config.clone())
                                },
                                if is_saving {
                                    "Saving..."
                                } else {
                                    "Save"
                                }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Medium,
                                disabled: !has_required_fields || is_testing,
                                loading: is_testing,
                                onclick: move |_| on_test_connection.call(()),
                                if is_testing {
                                    "Testing..."
                                } else {
                                    "Test Connection"
                                }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Medium,
                                onclick: move |_| on_cancel_edit.call(()),
                                "Cancel"
                            }
                        }
                    }
                } else if sync_configured {
                    // Show current config summary
                    div { class: "space-y-2 text-sm",
                        if let Some(ref bucket) = sync_bucket {
                            div { class: "flex justify-between",
                                span { class: "text-gray-400", "Bucket" }
                                span { class: "text-gray-200 font-mono", "{bucket}" }
                            }
                        }
                        if let Some(ref region) = sync_region {
                            div { class: "flex justify-between",
                                span { class: "text-gray-400", "Region" }
                                span { class: "text-gray-200 font-mono", "{region}" }
                            }
                        }
                        if let Some(ref endpoint) = sync_endpoint {
                            div { class: "flex justify-between",
                                span { class: "text-gray-400", "Endpoint" }
                                span { class: "text-gray-200 font-mono", "{endpoint}" }
                            }
                        }
                    }
                } else {
                    p { class: "text-sm text-gray-500",
                        "No sync bucket configured. Click Configure to set up syncing."
                    }
                }

                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "The sync bucket must be created externally (e.g. in your S3 provider's console). "
                        "bae uses this bucket to sync library metadata across your devices."
                    }
                }
            }
        }
    }
}

/// Truncate a hex-encoded public key for display: first 8 and last 8 characters.
fn truncate_pubkey(key: &str) -> String {
    if key.len() > 20 {
        format!("{}...{}", &key[..8], &key[key.len() - 8..])
    } else {
        key.to_string()
    }
}

/// Format a device ID for display: show first 8 characters.
fn short_device_id(id: &str) -> String {
    let clean = id.replace('-', "");
    if clean.len() > 8 {
        format!("{}...", &clean[..8])
    } else {
        clean
    }
}

/// Format share info as a text block for clipboard copy.
fn format_share_text(info: &ShareInfo) -> String {
    let mut lines = vec![
        format!("Bucket: {}", info.bucket),
        format!("Region: {}", info.region),
    ];
    if let Some(ref ep) = info.endpoint {
        lines.push(format!("Endpoint: {ep}"));
    }
    lines.push(format!("Invitee key: {}", info.invitee_pubkey));
    lines.join("\n")
}

/// Format an RFC 3339 timestamp as a relative time string.
///
/// Falls back to the raw timestamp if parsing fails.
fn format_relative_time(rfc3339: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rfc3339) else {
        return rfc3339.to_string();
    };

    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_seconds() < 60 {
        return "Just now".to_string();
    }

    if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        return if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{mins} minutes ago")
        };
    }

    if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hours} hours ago")
        };
    }

    let days = duration.num_days();
    if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{days} days ago")
    }
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
