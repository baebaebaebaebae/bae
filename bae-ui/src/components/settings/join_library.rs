//! Join shared library view -- single invite code paste form.

use crate::components::{
    Button, ButtonSize, ButtonVariant, LoadingSpinner, SettingsSection, TextInput, TextInputSize,
    TextInputType,
};
use dioxus::prelude::*;

/// Status of the join operation.
#[derive(Clone, Debug, PartialEq)]
pub enum JoinStatus {
    Joining(String),
    Success,
    Error(String),
}

/// Pure view component for joining a shared library.
///
/// The user pastes a single invite code (received from the library owner)
/// and clicks Join. The desktop wrapper decodes the code and handles the
/// bootstrap flow.
#[component]
pub fn JoinLibraryView(
    invite_code: String,
    status: Option<JoinStatus>,
    decoded_library_name: Option<String>,
    decoded_owner_pubkey: Option<String>,
    decoded_cloud_home: Option<String>,
    decode_error: Option<String>,

    on_code_change: EventHandler<String>,
    on_join: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let is_joining = matches!(status, Some(JoinStatus::Joining(_)));
    let is_success = matches!(status, Some(JoinStatus::Success));
    let has_valid_code = decoded_library_name.is_some() && decode_error.is_none();
    let can_join = !is_joining && !is_success && has_valid_code;

    rsx! {
        SettingsSection {
            div {
                h2 { class: "text-xl font-semibold text-white", "Join Shared Library" }
                p { class: "text-sm text-gray-400 mt-1",
                    "Paste the invite code you received from the library owner."
                }
            }

            if is_success {
                div { class: "p-4 rounded-lg bg-green-900/30 border border-green-700",
                    p { class: "text-sm text-green-300",
                        "Successfully joined the library. Restarting..."
                    }
                }
            } else {
                div { class: "space-y-4 mt-4",
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Invite code"
                        }
                        TextInput {
                            value: invite_code.clone(),
                            on_input: move |v| on_code_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Text,
                            placeholder: "Paste invite code here",
                            disabled: is_joining,
                        }
                    }

                    // Decode error
                    if let Some(ref err) = decode_error {
                        if !invite_code.is_empty() {
                            div { class: "text-sm text-red-400", "{err}" }
                        }
                    }

                    // Decoded info preview
                    if let Some(ref name) = decoded_library_name {
                        div { class: "p-3 bg-gray-700/50 rounded-lg space-y-2 text-sm",
                            div { class: "flex justify-between",
                                span { class: "text-gray-400", "Library" }
                                span { class: "text-gray-200", "\"{name}\"" }
                            }
                            if let Some(ref pubkey) = decoded_owner_pubkey {
                                div { class: "flex justify-between",
                                    span { class: "text-gray-400", "Owner" }
                                    span { class: "text-gray-200 font-mono", {truncate_pubkey(pubkey)} }
                                }
                            }
                            if let Some(ref cloud) = decoded_cloud_home {
                                div { class: "flex justify-between",
                                    span { class: "text-gray-400", "Cloud home" }
                                    span { class: "text-gray-200", "{cloud}" }
                                }
                            }
                        }
                    }
                }

                // Status display
                if let Some(ref status) = status {
                    match status {
                        JoinStatus::Joining(msg) => rsx! {
                            div { class: "flex items-center gap-2 mt-4 p-3 rounded-lg bg-gray-800",
                                LoadingSpinner {}
                                p { class: "text-sm text-gray-300", "{msg}" }
                            }
                        },
                        JoinStatus::Error(err) => rsx! {
                            div { class: "mt-4 p-3 rounded-lg bg-red-900/30 border border-red-700",
                                p { class: "text-sm text-red-300", "{err}" }
                            }
                        },
                        JoinStatus::Success => rsx! {},
                    }
                }

                // Buttons
                div { class: "flex justify-end gap-3 mt-6",
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Medium,
                        disabled: is_joining,
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Medium,
                        disabled: !can_join,
                        onclick: move |_| on_join.call(()),
                        if is_joining {
                            "Joining..."
                        } else {
                            "Join Library"
                        }
                    }
                }
            }
        }
    }
}

fn truncate_pubkey(key: &str) -> String {
    if key.len() > 20 {
        format!("{}...{}", &key[..8], &key[key.len() - 8..])
    } else {
        key.to_string()
    }
}
