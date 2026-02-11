//! Join shared library view -- form for accepting an invitation to a shared library.

use crate::components::{
    Button, ButtonSize, ButtonVariant, LoadingSpinner, SettingsSection, TextInput, TextInputSize,
    TextInputType,
};
use dioxus::prelude::*;

/// Status of the join operation.
#[derive(Clone, Debug, PartialEq)]
pub enum JoinStatus {
    /// Currently joining.
    Joining(String),
    /// Join succeeded.
    Success,
    /// Join failed with an error.
    Error(String),
}

/// Pure view component for joining a shared library.
///
/// The user pastes sync bucket coordinates (received out-of-band from the owner)
/// and clicks Join. The desktop wrapper handles the actual bootstrap flow.
#[component]
pub fn JoinLibraryView(
    /// Bucket name input value.
    bucket: String,
    /// Region input value.
    region: String,
    /// Endpoint input value (optional).
    endpoint: String,
    /// Access key input value.
    access_key: String,
    /// Secret key input value.
    secret_key: String,
    /// Current status of the join operation.
    status: Option<JoinStatus>,

    // --- Callbacks ---
    on_bucket_change: EventHandler<String>,
    on_region_change: EventHandler<String>,
    on_endpoint_change: EventHandler<String>,
    on_access_key_change: EventHandler<String>,
    on_secret_key_change: EventHandler<String>,
    /// Called when the user clicks "Join". The desktop wrapper performs the actual work.
    on_join: EventHandler<()>,
    /// Called when the user clicks "Cancel" to go back.
    on_cancel: EventHandler<()>,
) -> Element {
    let is_joining = matches!(status, Some(JoinStatus::Joining(_)));
    let is_success = matches!(status, Some(JoinStatus::Success));
    let can_join = !is_joining
        && !is_success
        && !bucket.is_empty()
        && !region.is_empty()
        && !access_key.is_empty()
        && !secret_key.is_empty();

    rsx! {
        SettingsSection {
            div {
                h2 { class: "text-xl font-semibold text-white", "Join Shared Library" }
                p { class: "text-sm text-gray-400 mt-1",
                    "Enter the sync bucket details shared with you by the library owner."
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
                    // Bucket
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Bucket "
                            span { class: "text-red-400", "*" }
                        }
                        TextInput {
                            value: bucket,
                            on_input: move |v| on_bucket_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Text,
                            placeholder: "my-sync-bucket",
                            disabled: is_joining,
                        }
                    }

                    // Region
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Region "
                            span { class: "text-red-400", "*" }
                        }
                        TextInput {
                            value: region,
                            on_input: move |v| on_region_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Text,
                            placeholder: "us-east-1",
                            disabled: is_joining,
                        }
                    }

                    // Endpoint (optional)
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Endpoint "
                            span { class: "text-xs text-gray-500",
                                "(optional, for S3-compatible services)"
                            }
                        }
                        TextInput {
                            value: endpoint,
                            on_input: move |v| on_endpoint_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Text,
                            placeholder: "https://s3.example.com",
                            disabled: is_joining,
                        }
                    }

                    // Access Key
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Access Key "
                            span { class: "text-red-400", "*" }
                        }
                        TextInput {
                            value: access_key,
                            on_input: move |v| on_access_key_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Text,
                            placeholder: "AKIAIOSFODNN7EXAMPLE",
                            disabled: is_joining,
                        }
                    }

                    // Secret Key
                    div {
                        label { class: "block text-sm font-medium text-gray-300 mb-1",
                            "Secret Key "
                            span { class: "text-red-400", "*" }
                        }
                        TextInput {
                            value: secret_key,
                            on_input: move |v| on_secret_key_change.call(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Password,
                            placeholder: "",
                            disabled: is_joining,
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
