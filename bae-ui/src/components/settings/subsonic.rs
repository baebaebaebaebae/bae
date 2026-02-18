//! Subsonic section view

use crate::components::{Button, ButtonSize, ButtonVariant, SettingsCard, SettingsSection};
use dioxus::prelude::*;

/// Subsonic section view
#[component]
pub fn SubsonicSectionView(
    /// Whether Subsonic server is enabled
    enabled: bool,
    /// Port number
    port: u16,
    /// Whether auth is enabled
    auth_enabled: bool,
    /// Configured username (display mode)
    auth_username: Option<String>,
    /// Whether a password is configured (display mode)
    auth_password_set: bool,
    /// Whether currently in edit mode
    is_editing: bool,
    /// Temporary values while editing
    edit_enabled: bool,
    edit_port: String,
    edit_auth_enabled: bool,
    edit_username: String,
    edit_password: String,
    edit_password_confirm: String,
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
    // Share link settings
    share_base_url: String,
    is_editing_share: bool,
    edit_share_base_url: String,
    is_saving_share: bool,
    has_share_changes: bool,
    share_save_error: Option<String>,
    on_share_edit_start: EventHandler<()>,
    on_share_cancel: EventHandler<()>,
    on_share_save: EventHandler<()>,
    on_share_base_url_change: EventHandler<String>,
    on_auth_enabled_change: EventHandler<bool>,
    on_username_change: EventHandler<String>,
    on_password_change: EventHandler<String>,
    on_password_confirm_change: EventHandler<String>,
) -> Element {
    let passwords_mismatch = !edit_password.is_empty() && edit_password != edit_password_confirm;
    let needs_password = edit_auth_enabled && !auth_password_set && edit_password.is_empty();
    let needs_username = edit_auth_enabled && edit_username.is_empty();

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

            SettingsCard {
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Authentication" }
                }

                if is_editing {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: edit_auth_enabled,
                                onchange: move |e| on_auth_enabled_change.call(e.checked()),
                            }
                            label { class: "text-sm text-gray-300", "Require authentication" }
                        }

                        if edit_auth_enabled {
                            div { class: "flex items-center gap-4",
                                label { class: "text-sm text-gray-400 w-32", "Username:" }
                                input {
                                    r#type: "text",
                                    class: "flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                    placeholder: "admin",
                                    value: "{edit_username}",
                                    oninput: move |e| on_username_change.call(e.value()),
                                }
                            }
                            div { class: "flex items-center gap-4",
                                label { class: "text-sm text-gray-400 w-32", "Password:" }
                                input {
                                    r#type: "password",
                                    class: "flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                    placeholder: if auth_password_set { "Leave blank to keep current" } else { "Enter password" },
                                    value: "{edit_password}",
                                    oninput: move |e| on_password_change.call(e.value()),
                                }
                            }
                            div { class: "flex items-center gap-4",
                                label { class: "text-sm text-gray-400 w-32", "Confirm:" }
                                input {
                                    r#type: "password",
                                    class: "flex-1 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                    placeholder: "Confirm password",
                                    value: "{edit_password_confirm}",
                                    oninput: move |e| on_password_confirm_change.call(e.value()),
                                }
                            }

                            if passwords_mismatch {
                                div { class: "text-sm text-red-400", "Passwords do not match" }
                            }
                            if needs_username {
                                div { class: "text-sm text-red-400", "Username is required" }
                            }
                            if needs_password {
                                div { class: "text-sm text-red-400", "Password is required" }
                            }
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Authentication:" }
                            span { class: if auth_enabled { "text-green-400" } else { "text-gray-500" },
                                if auth_enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            }
                        }
                        if auth_enabled {
                            if let Some(username) = &auth_username {
                                div { class: "flex items-center gap-2",
                                    span { class: "text-gray-400", "Username:" }
                                    span { class: "text-white", "{username}" }
                                }
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
                        disabled: !has_changes || is_saving || passwords_mismatch || needs_password || needs_username,
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
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Share Links" }
                    if !is_editing_share {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_share_edit_start.call(()),
                            "Edit"
                        }
                    }
                }

                if is_editing_share {
                    div { class: "space-y-4",
                        div {
                            label { class: "block text-sm text-gray-400 mb-1", "Base URL" }
                            input {
                                r#type: "text",
                                class: "w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "https://listen.example.com",
                                value: "{edit_share_base_url}",
                                oninput: move |e| on_share_base_url_change.call(e.value()),
                            }
                            p { class: "text-xs text-gray-500 mt-1",
                                "The public URL where your bae instance is accessible."
                            }
                        }
                    }

                    if let Some(error) = share_save_error {
                        div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300 mt-4",
                            "{error}"
                        }
                    }

                    div { class: "flex gap-3 mt-4",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Medium,
                            disabled: !has_share_changes || is_saving_share,
                            loading: is_saving_share,
                            onclick: move |_| on_share_save.call(()),
                            if is_saving_share {
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
                                span { class: "text-white font-mono", "{share_base_url}" }
                            }
                        }
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
        }
    }
}
