//! Follow library view -- form for adding a remote library to follow via follow code.

use crate::components::{
    Button, ButtonSize, ButtonVariant, LoadingSpinner, SettingsSection, TextInput, TextInputSize,
    TextInputType,
};
use dioxus::prelude::*;

/// Status of the follow save operation.
#[derive(Clone, Debug, PartialEq)]
pub enum FollowSyncStatus {
    /// Initial sync in progress.
    Syncing(String),
    /// Follow succeeded.
    Success,
    /// Follow failed.
    Error(String),
}

/// Pure view component for following a remote library via follow code.
#[component]
pub fn FollowLibraryView(
    follow_code: String,
    code_error: Option<String>,
    decoded_name: Option<String>,
    decoded_url: Option<String>,
    is_saving: bool,
    save_status: Option<FollowSyncStatus>,

    on_code_change: EventHandler<String>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let can_save = !is_saving
        && decoded_url.is_some()
        && !matches!(save_status, Some(FollowSyncStatus::Syncing(_)));

    rsx! {
        SettingsSection {
            div {
                h2 { class: "text-xl font-semibold text-white", "Follow Library" }
                p { class: "text-sm text-gray-400 mt-1",
                    "Paste a follow code to sync a read-only copy of a remote library."
                }
            }

            // Follow code input
            div { class: "mt-4",
                label { class: "block text-sm font-medium text-gray-300 mb-1", "Follow Code" }
                TextInput {
                    value: follow_code,
                    on_input: move |v| on_code_change.call(v),
                    size: TextInputSize::Medium,
                    input_type: TextInputType::Text,
                    placeholder: "Paste the follow code here...",
                    disabled: is_saving,
                }
                if let Some(ref err) = code_error {
                    p { class: "text-sm text-red-400 mt-1", "{err}" }
                }
            }

            // Decoded info
            if decoded_name.is_some() || decoded_url.is_some() {
                div { class: "mt-4 p-3 rounded-lg bg-gray-800 space-y-1",
                    if let Some(ref name) = decoded_name {
                        p { class: "text-sm text-gray-300",
                            span { class: "text-gray-500", "Name: " }
                            "{name}"
                        }
                    }
                    if let Some(ref url) = decoded_url {
                        p { class: "text-sm text-gray-300",
                            span { class: "text-gray-500", "Proxy: " }
                            "{url}"
                        }
                    }
                }
            }

            // Save status
            if let Some(ref status) = save_status {
                match status {
                    FollowSyncStatus::Syncing(msg) => rsx! {
                        div { class: "flex items-center gap-2 mt-4 p-3 rounded-lg bg-gray-800",
                            LoadingSpinner {}
                            p { class: "text-sm text-gray-300", "{msg}" }
                        }
                    },
                    FollowSyncStatus::Success => rsx! {
                        div { class: "mt-4 p-3 rounded-lg bg-green-900/30 border border-green-700",
                            p { class: "text-sm text-green-300", "Library followed." }
                        }
                    },
                    FollowSyncStatus::Error(err) => rsx! {
                        div { class: "mt-4 p-3 rounded-lg bg-red-900/30 border border-red-700",
                            p { class: "text-sm text-red-300", "{err}" }
                        }
                    },
                }
            }

            // Buttons
            div { class: "flex justify-end gap-3 mt-6",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Medium,
                    disabled: is_saving,
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Medium,
                    disabled: !can_save,
                    onclick: move |_| on_save.call(()),
                    if is_saving {
                        "Saving..."
                    } else {
                        "Follow"
                    }
                }
            }
        }
    }
}
