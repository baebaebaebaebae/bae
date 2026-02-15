//! Sync status and configuration section view

use crate::components::helpers::Tooltip;
use crate::components::icons::{CheckIcon, CopyIcon};
use crate::components::settings::cloud_provider::{CloudProviderOption, CloudProviderPicker};
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChromelessButton, SettingsCard, SettingsSection, TextInput,
    TextInputSize, TextInputType,
};
use crate::floating_ui::Placement;
use crate::stores::config::CloudProvider;
use crate::stores::{
    DeviceActivityInfo, InviteStatus, Member, MemberRole, ShareInfo, SharedReleaseDisplay,
};
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
    /// Whether sync is fully configured (cloud provider + credentials).
    cloud_home_configured: bool,

    // --- Cloud provider props ---
    /// Currently selected cloud provider.
    cloud_provider: Option<CloudProvider>,
    /// Available cloud provider options.
    cloud_options: Vec<CloudProviderOption>,
    /// Whether a cloud sign-in is in progress.
    signing_in: bool,
    /// Error from a cloud sign-in attempt.
    sign_in_error: Option<String>,
    /// Callback when user selects a provider.
    on_select_provider: EventHandler<CloudProvider>,
    /// Callback when user clicks sign in for an OAuth provider.
    on_sign_in: EventHandler<CloudProvider>,
    /// Callback when user disconnects the current provider.
    on_disconnect_provider: EventHandler<()>,
    /// Callback when user selects iCloud Drive.
    on_use_icloud: EventHandler<()>,

    // --- S3 edit state props (passed through to CloudProviderPicker) ---
    /// Whether currently editing the S3 config.
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

    // --- Members props ---
    /// Current library members from membership chain. Empty if solo/not syncing.
    members: Vec<Member>,
    /// Whether the current user is an owner (controls visibility of invite/remove).
    is_owner: bool,
    /// Called when the user confirms removal of a member. Carries the member's pubkey.
    on_remove_member: EventHandler<String>,
    /// Whether a member removal operation is in progress.
    is_removing_member: bool,
    /// Error from a member removal attempt.
    removing_member_error: Option<String>,

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

    // --- Shared releases props ---
    /// Releases shared with us via accepted share grants.
    shared_releases: Vec<SharedReleaseDisplay>,
    /// Accept form: JSON text input.
    accept_grant_text: String,
    /// Whether an accept operation is in progress.
    is_accepting_grant: bool,
    /// Error from a grant accept attempt.
    accept_grant_error: Option<String>,
    /// Called when the accept textarea content changes.
    on_accept_grant_text_change: EventHandler<String>,
    /// Called when the user clicks Accept.
    on_accept_grant: EventHandler<String>,
    /// Called when the user clicks Remove on a shared release.
    on_revoke_shared_release: EventHandler<String>,
) -> Element {
    let mut copied = use_signal(|| false);
    let mut share_copied = use_signal(|| false);
    let mut confirming_remove_pubkey = use_signal(|| Option::<String>::None);

    let handle_copy = move |_| {
        on_copy_pubkey.call(());
        copied.set(true);
        spawn(async move {
            sleep_ms(2000).await;
            copied.set(false);
        });
    };

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
                        Tooltip {
                            text: "Copy public key to clipboard",
                            placement: Placement::Top,
                            nowrap: true,
                            ChromelessButton {
                                class: Some("text-gray-400 hover:text-white transition-colors".to_string()),
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
                        if cloud_home_configured {
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
                        disabled: syncing || !cloud_home_configured,
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
            if cloud_home_configured {
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
                            let confirming = confirming_remove_pubkey.read().clone();
                            rsx! {
                                div { class: "space-y-2",
                                    for member in members.iter() {
                                        {
                                            let can_remove = is_owner && !member.is_self
                                                && !(member.role == MemberRole::Owner && owner_count <= 1);
                                            let pubkey = member.pubkey.clone();
                                            let is_confirming = confirming.as_deref() == Some(&member.pubkey);
                                            let is_this_removing = is_confirming && is_removing_member;
                                            rsx! {
                                                div { key: "{member.pubkey}", class: "py-1.5",
                                                    div { class: "flex justify-between items-center",
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
                                                        if can_remove && !is_confirming {
                                                            Button {
                                                                variant: ButtonVariant::Secondary,
                                                                size: ButtonSize::Small,
                                                                onclick: move |_| confirming_remove_pubkey.set(Some(pubkey.clone())),
                                                                "Remove"
                                                            }
                                                        }
                                                    }
                                                    if is_confirming {
                                                        div { class: "mt-2 p-3 bg-red-900/20 border border-red-800 rounded-lg",
                                                            p { class: "text-sm text-gray-300 mb-3",
                                                                "Remove {member.display_name}? This will rotate the encryption key."
                                                            }
                                                            if let Some(ref err) = removing_member_error {
                                                                div { class: "text-sm text-red-400 mb-3", "{err}" }
                                                            }
                                                            div { class: "flex gap-2",
                                                                {
                                                                    let confirm_pubkey = member.pubkey.clone();
                                                                    rsx! {
                                                                        Button {
                                                                            variant: ButtonVariant::Danger,
                                                                            size: ButtonSize::Small,
                                                                            disabled: is_this_removing,
                                                                            loading: is_this_removing,
                                                                            onclick: move |_| on_remove_member.call(confirm_pubkey.clone()),
                                                                            if is_this_removing {
                                                                                "Removing..."
                                                                            } else {
                                                                                "Confirm"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                Button {
                                                                    variant: ButtonVariant::Secondary,
                                                                    size: ButtonSize::Small,
                                                                    disabled: is_this_removing,
                                                                    onclick: move |_| confirming_remove_pubkey.set(None),
                                                                    "Cancel"
                                                                }
                                                            }
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
                            let code = info.invite_code.clone();
                            rsx! {
                                div { class: "mt-4 pt-4 border-t border-gray-700",
                                    h4 { class: "text-sm font-medium text-gray-300 mb-3",
                                        "Send this invite code to {info.invitee_display}:"
                                    }
                                    textarea {
                                        class: "w-full h-24 bg-gray-700 text-white text-sm font-mono rounded-lg p-3 border border-gray-600 focus:outline-none resize-none",
                                        readonly: true,
                                        value: "{info.invite_code}",
                                    }
                                    p { class: "text-xs text-gray-500 mt-2",
                                        "The code contains cloud home connection info. The encryption key is delivered separately via the membership chain."
                                    }
                                    div { class: "flex gap-3 mt-3",
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            size: ButtonSize::Small,
                                            onclick: {
                                                let text = code.clone();
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
                                            "Done"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Cloud provider picker (replaces old S3-only sync bucket card)
            CloudProviderPicker {
                selected: cloud_provider,
                options: cloud_options,
                signing_in,
                sign_in_error,
                s3_is_editing: is_editing,
                s3_bucket: edit_bucket,
                s3_region: edit_region,
                s3_endpoint: edit_endpoint,
                s3_access_key: edit_access_key,
                s3_secret_key: edit_secret_key,
                on_select: move |p| on_select_provider.call(p),
                on_sign_in: move |p| on_sign_in.call(p),
                on_disconnect: move |_| on_disconnect_provider.call(()),
                on_use_icloud: move |_| on_use_icloud.call(()),
                on_s3_edit_start: move |_| on_edit_start.call(()),
                on_s3_cancel: move |_| on_cancel_edit.call(()),
                on_s3_save: move |config| on_save_config.call(config),
                on_s3_bucket_change: move |v| on_bucket_change.call(v),
                on_s3_region_change: move |v| on_region_change.call(v),
                on_s3_endpoint_change: move |v| on_endpoint_change.call(v),
                on_s3_access_key_change: move |v| on_access_key_change.call(v),
                on_s3_secret_key_change: move |v| on_secret_key_change.call(v),
            }

            // Shared with Me
            SettingsCard {
                h3 { class: "text-lg font-medium text-white mb-4", "Shared with Me" }

                // Accept grant form
                div { class: "space-y-3 mb-4",
                    label { class: "block text-sm text-gray-400",
                        "Paste a share grant JSON token to gain access to a shared release."
                    }
                    textarea {
                        class: "w-full h-24 bg-gray-700 text-white text-sm font-mono rounded-lg p-3 border border-gray-600 focus:border-blue-500 focus:outline-none resize-none placeholder-gray-500",
                        placeholder: "Paste share grant JSON here...",
                        value: "{accept_grant_text}",
                        oninput: move |e| on_accept_grant_text_change.call(e.value()),
                    }

                    if let Some(ref err) = accept_grant_error {
                        div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                            "{err}"
                        }
                    }

                    {
                        let text = accept_grant_text.clone();
                        rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                disabled: text.trim().is_empty() || is_accepting_grant,
                                loading: is_accepting_grant,
                                onclick: move |_| on_accept_grant.call(text.clone()),
                                if is_accepting_grant {
                                    "Accepting..."
                                } else {
                                    "Accept"
                                }
                            }
                        }
                    }
                }

                // Shared releases list
                if shared_releases.is_empty() {
                    p { class: "text-sm text-gray-500", "No shared releases yet." }
                } else {
                    div { class: "space-y-2",
                        for release in shared_releases.iter() {
                            {
                                let grant_id = release.grant_id.clone();
                                rsx! {
                                    div {
                                        key: "{release.grant_id}",
                                        class: "flex items-center justify-between p-3 bg-gray-700/50 rounded-lg",
                                        div { class: "min-w-0 flex-1",
                                            div { class: "flex items-center gap-2 text-sm",
                                                span { class: "text-gray-200 font-mono truncate", {truncate_id(&release.release_id)} }
                                                span { class: "text-gray-500", "from" }
                                                span { class: "text-gray-400 font-mono", {truncate_pubkey(&release.from_user_pubkey)} }
                                            }
                                            div { class: "flex items-center gap-3 mt-1 text-xs text-gray-500",
                                                span { "Bucket: {release.bucket}" }
                                                if let Some(ref exp) = release.expires {
                                                    span { "Expires: {exp}" }
                                                }
                                            }
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            size: ButtonSize::Small,
                                            onclick: move |_| on_revoke_shared_release.call(grant_id.clone()),
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
    }
}

/// Truncate an ID for display: first 8 characters.
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..8])
    } else {
        id.to_string()
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
