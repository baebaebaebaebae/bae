//! Subsonic section view

use crate::components::{
    Button, ButtonSize, ButtonVariant, Select, SelectOption, SettingsCard, SettingsSection,
};
use dioxus::prelude::*;

/// Subsonic section view
#[component]
pub fn SubsonicSectionView(
    /// Whether Subsonic server is enabled
    enabled: bool,
    /// Port number
    port: u16,
    /// Whether currently in edit mode
    is_editing: bool,
    /// Temporary values while editing
    edit_enabled: bool,
    edit_port: String,
    /// State flags
    is_saving: bool,
    has_changes: bool,
    save_error: Option<String>,
    /// Callbacks
    on_edit_start: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_save: EventHandler<()>,
    on_enabled_change: EventHandler<bool>,
    on_port_change: EventHandler<String>,
    /// Base URL for share links
    share_base_url: String,
    /// Whether share link settings are being edited
    share_is_editing: bool,
    /// Temporary value while editing share base URL
    share_edit_base_url: String,
    /// Default expiry days (None = never)
    share_default_expiry_days: Option<u32>,
    /// Editing value for expiry days
    share_edit_expiry_days: Option<u32>,
    /// Signing key version
    share_signing_key_version: u32,
    /// State flags for share settings
    share_is_saving: bool,
    share_has_changes: bool,
    share_save_error: Option<String>,
    /// Callbacks for share settings
    on_share_edit_start: EventHandler<()>,
    on_share_cancel: EventHandler<()>,
    on_share_save: EventHandler<()>,
    on_share_base_url_change: EventHandler<String>,
    on_share_expiry_change: EventHandler<Option<u32>>,
    on_share_rotate_key: EventHandler<()>,
) -> Element {
    rsx! {
        SettingsSection {
            h2 { class: "text-xl font-semibold text-white mb-6", "Subsonic Server" }

            SettingsCard {
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Server Settings" }
                    if !is_editing {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_edit_start.call(()),
                            "Edit"
                        }
                    }
                }

                if is_editing {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: edit_enabled,
                                onchange: move |e| on_enabled_change.call(e.checked()),
                            }
                            label { class: "text-sm text-gray-300", "Enable Subsonic API server" }
                        }
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-32", "Port:" }
                            input {
                                r#type: "number",
                                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                min: "1024",
                                max: "65535",
                                value: "{edit_port}",
                                oninput: move |e| on_port_change.call(e.value()),
                            }
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Status:" }
                            span { class: if enabled { "text-green-400" } else { "text-gray-500" },
                                if enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Port:" }
                            span { class: "text-white font-mono", "{port}" }
                        }
                        if enabled {
                            div { class: "flex items-center gap-2",
                                span { class: "text-gray-400", "URL:" }
                                span { class: "text-indigo-400 font-mono", "http://127.0.0.1:{port}" }
                            }
                        }
                    }
                }
            }

            if is_editing {
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

                div { class: "p-4 bg-yellow-900/20 border border-yellow-700/50 rounded-lg",
                    p { class: "text-sm text-yellow-200/80",
                        "Changes require an app restart to take effect."
                    }
                }
            }

            SettingsCard {
                h3 { class: "text-lg font-medium text-white mb-4", "About Subsonic" }
                div { class: "space-y-3 text-sm text-gray-400",
                    p {
                        "The Subsonic API allows you to stream your music library to compatible apps like "
                        "Symfonium, Ultrasonic, or any Subsonic/Navidrome-compatible client."
                    }
                    p {
                        "Connect your mobile app to "
                        span { class: "font-mono text-indigo-400", "http://YOUR_IP:{port}" }
                        " (or use localhost for the same device)."
                    }
                }
            }

            SettingsCard {
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Share Links" }
                    if !share_is_editing {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_share_edit_start.call(()),
                            "Edit"
                        }
                    }
                }

                if share_is_editing {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-32", "Base URL:" }
                            input {
                                r#type: "text",
                                class: "flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "https://listen.example.com",
                                value: "{share_edit_base_url}",
                                oninput: move |e| on_share_base_url_change.call(e.value()),
                            }
                        }
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-32", "Default expiry:" }
                            Select {
                                value: match share_edit_expiry_days {
                                    Some(7) => "7".to_string(),
                                    Some(30) => "30".to_string(),
                                    Some(90) => "90".to_string(),
                                    Some(other) => other.to_string(),
                                    None => "never".to_string(),
                                },
                                onchange: move |val: String| {
                                    let days = match val.as_str() {
                                        "never" => None,
                                        v => v.parse().ok(),
                                    };
                                    on_share_expiry_change.call(days);
                                },
                                SelectOption {
                                    value: "7".to_string(),
                                    label: "7 days".to_string(),
                                }
                                SelectOption {
                                    value: "30".to_string(),
                                    label: "30 days".to_string(),
                                }
                                SelectOption {
                                    value: "90".to_string(),
                                    label: "90 days".to_string(),
                                }
                                SelectOption {
                                    value: "never".to_string(),
                                    label: "Never".to_string(),
                                }
                            }
                        }
                    }

                    if let Some(ref error) = share_save_error {
                        div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                            "{error}"
                        }
                    }

                    div { class: "flex gap-3",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Medium,
                            disabled: !share_has_changes || share_is_saving,
                            loading: share_is_saving,
                            onclick: move |_| on_share_save.call(()),
                            if share_is_saving {
                                "Saving..."
                            } else {
                                "Save Changes"
                            }
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Medium,
                            onclick: move |_| on_share_cancel.call(()),
                            "Cancel"
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Base URL:" }
                            if share_base_url.is_empty() {
                                span { class: "text-gray-500 italic", "Not configured" }
                            } else {
                                span { class: "text-indigo-400 font-mono", "{share_base_url}" }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Default expiry:" }
                            span { class: "text-white",
                                match share_default_expiry_days {
                                    Some(7) => "7 days",
                                    Some(30) => "30 days",
                                    Some(90) => "90 days",
                                    Some(_) => "Custom",
                                    None => "Never",
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Signing key version:" }
                            span { class: "text-white font-mono", "{share_signing_key_version}" }
                        }
                    }
                }

                div { class: "mt-4 pt-4 border-t border-gray-700",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "text-sm text-gray-400",
                                "Invalidate all outstanding share links by rotating the signing key."
                            }
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_share_rotate_key.call(()),
                            "Rotate Key"
                        }
                    }
                }
            }
        }
    }
}
