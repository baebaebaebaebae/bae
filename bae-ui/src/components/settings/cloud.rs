//! Cloud sync section view

use crate::components::{
    Button, ButtonSize, ButtonVariant, SettingsCard, SettingsSection, TextInput, TextInputSize,
    TextInputType,
};
use crate::stores::CloudSyncStatus;
use dioxus::prelude::*;

/// Cloud sync section view
#[component]
pub fn CloudSectionView(
    /// Whether encryption is configured (required for cloud sync)
    encryption_configured: bool,
    /// Whether cloud sync is enabled
    enabled: bool,
    /// Last successful upload timestamp (ISO 8601)
    last_upload: Option<String>,
    /// Current sync status
    sync_status: CloudSyncStatus,
    /// Whether currently in edit mode
    is_editing: bool,
    /// Temporary values while editing
    edit_enabled: bool,
    edit_bucket: String,
    edit_region: String,
    edit_endpoint: String,
    edit_access_key: String,
    edit_secret_key: String,
    /// State flags
    is_saving: bool,
    has_changes: bool,
    save_error: Option<String>,
    /// Callbacks
    on_edit_start: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_save: EventHandler<()>,
    on_sync_now: EventHandler<()>,
    on_enabled_change: EventHandler<bool>,
    on_bucket_change: EventHandler<String>,
    on_region_change: EventHandler<String>,
    on_endpoint_change: EventHandler<String>,
    on_access_key_change: EventHandler<String>,
    on_secret_key_change: EventHandler<String>,
) -> Element {
    if !encryption_configured {
        return rsx! {
            SettingsSection {
                h2 { class: "text-xl font-semibold text-white mb-6", "Cloud Sync" }
                SettingsCard {
                    div { class: "text-center py-8",
                        p { class: "text-gray-400 mb-2",
                            "Cloud sync requires an encryption key to protect your data."
                        }
                        p { class: "text-sm text-gray-500",
                            "Set up an encryption key in the Discogs section first."
                        }
                    }
                }
            }
        };
    }

    rsx! {
        SettingsSection {
            h2 { class: "text-xl font-semibold text-white mb-6", "Cloud Sync" }

            // Status card
            SettingsCard {
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Sync Status" }
                    div { class: "flex items-center gap-3",
                        if enabled {
                            if !matches!(sync_status, CloudSyncStatus::Syncing) {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Small,
                                    onclick: move |_| on_sync_now.call(()),
                                    "Sync Now"
                                }
                            }
                        }
                        if !is_editing {
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Small,
                                onclick: move |_| on_edit_start.call(()),
                                "Edit"
                            }
                        }
                    }
                }

                div { class: "space-y-2 text-sm",
                    div { class: "flex items-center gap-2",
                        span { class: "text-gray-400", "Status:" }
                        match &sync_status {
                            CloudSyncStatus::Idle => rsx! {
                                span { class: if enabled { "text-green-400" } else { "text-gray-500" },
                                    if enabled {
                                        "Enabled"
                                    } else {
                                        "Disabled"
                                    }
                                }
                            },
                            CloudSyncStatus::Syncing => rsx! {
                                span { class: "text-indigo-400", "Syncing..." }
                            },
                            CloudSyncStatus::Error(msg) => rsx! {
                                span { class: "text-red-400", "Error: {msg}" }
                            },
                        }
                    }
                    if let Some(ref timestamp) = last_upload {
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Last upload:" }
                            span { class: "text-white font-mono text-xs", "{timestamp}" }
                        }
                    }
                }
            }

            // Edit form
            if is_editing {
                SettingsCard {
                    h3 { class: "text-lg font-medium text-white mb-4", "Configuration" }

                    div { class: "space-y-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: edit_enabled,
                                onchange: move |e| on_enabled_change.call(e.checked()),
                            }
                            label { class: "text-sm text-gray-300", "Enable cloud sync" }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "S3 Bucket"
                            }
                            TextInput {
                                value: edit_bucket,
                                on_input: move |v| on_bucket_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "my-bae-backup",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Region"
                            }
                            TextInput {
                                value: edit_region,
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
                                value: edit_endpoint,
                                on_input: move |v| on_endpoint_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "https://s3.example.com",
                            }
                            p { class: "text-xs text-gray-500 mt-1",
                                "Custom endpoint for MinIO, Backblaze B2, etc."
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Access Key"
                            }
                            TextInput {
                                value: edit_access_key,
                                on_input: move |v| on_access_key_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Password,
                                placeholder: "Access key ID",
                            }
                        }

                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2",
                                "Secret Key"
                            }
                            TextInput {
                                value: edit_secret_key,
                                on_input: move |v| on_secret_key_change.call(v),
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Password,
                                placeholder: "Secret access key",
                            }
                        }
                    }
                }

                if let Some(error) = save_error {
                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                        "{error}"
                    }
                }

                div { class: "flex gap-3",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Medium,
                        disabled: !has_changes || is_saving,
                        loading: is_saving,
                        onclick: move |_| on_save.call(()),
                        if is_saving {
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Medium,
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                }
            }

            // Info card
            SettingsCard {
                h3 { class: "text-lg font-medium text-white mb-4", "About Cloud Sync" }
                div { class: "space-y-3 text-sm text-gray-400",
                    p {
                        "Cloud sync uploads an encrypted copy of your library database and cover art to S3-compatible storage. "
                        "Use it to restore your library on a new device."
                    }
                    p {
                        "Uploads happen automatically when your library changes and can also be triggered manually."
                    }
                }
            }
        }
    }
}
