//! Follow library view -- form for adding a remote server to follow.

use crate::components::{
    Button, ButtonSize, ButtonVariant, LoadingSpinner, SettingsSection, TextInput, TextInputSize,
    TextInputType,
};
use dioxus::prelude::*;

/// Status of the follow test/save operation.
#[derive(Clone, Debug, PartialEq)]
pub enum FollowTestStatus {
    /// Connection test in progress.
    Testing,
    /// Connection test succeeded.
    Success,
    /// Connection test failed.
    Error(String),
}

/// Pure view component for following a remote server.
#[component]
pub fn FollowLibraryView(
    follow_code: String,
    code_error: Option<String>,
    name: String,
    server_url: String,
    username: String,
    password: String,
    test_status: Option<FollowTestStatus>,
    is_saving: bool,

    on_code_change: EventHandler<String>,
    on_name_change: EventHandler<String>,
    on_server_url_change: EventHandler<String>,
    on_username_change: EventHandler<String>,
    on_password_change: EventHandler<String>,
    on_test: EventHandler<()>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let is_testing = matches!(test_status, Some(FollowTestStatus::Testing));
    let test_succeeded = matches!(test_status, Some(FollowTestStatus::Success));
    let can_test = !is_testing
        && !is_saving
        && !server_url.is_empty()
        && !username.is_empty()
        && !password.is_empty();
    let can_save = !is_testing
        && !is_saving
        && test_succeeded
        && !name.is_empty()
        && !server_url.is_empty()
        && !username.is_empty()
        && !password.is_empty();

    rsx! {
        SettingsSection {
            div {
                h2 { class: "text-xl font-semibold text-white", "Follow Server" }
                p { class: "text-sm text-gray-400 mt-1",
                    "Paste a follow code, or enter connection details manually."
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
                    placeholder: "Paste a follow code to auto-fill...",
                    disabled: is_saving,
                }
                if let Some(ref err) = code_error {
                    p { class: "text-sm text-red-400 mt-1", "{err}" }
                }
            }

            div { class: "relative my-4",
                div { class: "absolute inset-0 flex items-center",
                    div { class: "w-full border-t border-gray-700" }
                }
                div { class: "relative flex justify-center text-xs",
                    span { class: "bg-gray-900 px-2 text-gray-500", "or enter manually" }
                }
            }

            div { class: "space-y-4",
                // Display name
                div {
                    label { class: "block text-sm font-medium text-gray-300 mb-1",
                        "Name "
                        span { class: "text-red-400", "*" }
                    }
                    TextInput {
                        value: name,
                        on_input: move |v| on_name_change.call(v),
                        size: TextInputSize::Medium,
                        input_type: TextInputType::Text,
                        placeholder: "Friend's Library",
                        disabled: is_saving,
                    }
                }
                // Server URL
                div {
                    label { class: "block text-sm font-medium text-gray-300 mb-1",
                        "Server URL "
                        span { class: "text-red-400", "*" }
                    }
                    TextInput {
                        value: server_url,
                        on_input: move |v| on_server_url_change.call(v),
                        size: TextInputSize::Medium,
                        input_type: TextInputType::Text,
                        placeholder: "http://192.168.1.100:4533",
                        disabled: is_saving,
                    }
                }
                // Username
                div {
                    label { class: "block text-sm font-medium text-gray-300 mb-1",
                        "Username "
                        span { class: "text-red-400", "*" }
                    }
                    TextInput {
                        value: username,
                        on_input: move |v| on_username_change.call(v),
                        size: TextInputSize::Medium,
                        input_type: TextInputType::Text,
                        placeholder: "listener",
                        disabled: is_saving,
                    }
                }
                // Password
                div {
                    label { class: "block text-sm font-medium text-gray-300 mb-1",
                        "Password "
                        span { class: "text-red-400", "*" }
                    }
                    TextInput {
                        value: password,
                        on_input: move |v| on_password_change.call(v),
                        size: TextInputSize::Medium,
                        input_type: TextInputType::Password,
                        placeholder: "",
                        disabled: is_saving,
                    }
                }
            }

            // Test status
            if let Some(ref status) = test_status {
                match status {
                    FollowTestStatus::Testing => rsx! {
                        div { class: "flex items-center gap-2 mt-4 p-3 rounded-lg bg-gray-800",
                            LoadingSpinner {}
                            p { class: "text-sm text-gray-300", "Testing connection..." }
                        }
                    },
                    FollowTestStatus::Success => rsx! {
                        div { class: "mt-4 p-3 rounded-lg bg-green-900/30 border border-green-700",
                            p { class: "text-sm text-green-300", "Connection successful." }
                        }
                    },
                    FollowTestStatus::Error(err) => rsx! {
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
                    disabled: is_testing || is_saving,
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Medium,
                    disabled: !can_test,
                    onclick: move |_| on_test.call(()),
                    if is_testing {
                        "Testing..."
                    } else {
                        "Test Connection"
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Medium,
                    disabled: !can_save,
                    onclick: move |_| on_save.call(()),
                    if is_saving {
                        "Saving..."
                    } else {
                        "Save"
                    }
                }
            }
        }
    }
}
