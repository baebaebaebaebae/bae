//! Sync status section view

use crate::components::{SettingsCard, SettingsSection};
use crate::stores::DeviceActivityInfo;
use dioxus::prelude::*;

/// Sync status section view (pure, props-based).
///
/// Displays the current sync state: when we last synced, whether a sync
/// is in progress, and what other devices have been doing.
#[component]
pub fn SyncSectionView(
    /// When this device last synced (RFC 3339). None if never synced.
    last_sync_time: Option<String>,
    /// Other devices' sync activity.
    other_devices: Vec<DeviceActivityInfo>,
    /// Whether a sync is currently in progress.
    syncing: bool,
    /// Last sync error, if any.
    error: Option<String>,
) -> Element {
    rsx! {
        SettingsSection {
            h2 { class: "text-xl font-semibold text-white", "Sync" }

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

                    // Error display
                    if let Some(ref err) = error {
                        div { class: "text-red-400 text-sm", "{err}" }
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
        }
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
