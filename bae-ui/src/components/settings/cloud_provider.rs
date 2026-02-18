//! Cloud provider picker component for the sync settings.

use crate::components::{
    Button, ButtonSize, ButtonVariant, LoadingSpinner, SettingsCard, TextInput, TextInputSize,
    TextInputType,
};
use crate::stores::config::CloudProvider;
use dioxus::prelude::*;

use super::sync::SyncBucketConfig;

/// Display info for a single cloud provider option.
#[derive(Clone, Debug, PartialEq)]
pub struct CloudProviderOption {
    pub provider: CloudProvider,
    pub label: &'static str,
    pub description: &'static str,
    pub connected_account: Option<String>,
}

/// Auth mode for bae cloud signup/login form.
#[derive(Clone, Debug, PartialEq)]
pub enum BaeCloudAuthMode {
    SignUp,
    LogIn,
}

/// Cloud provider picker -- lets the user select and configure a cloud home backend.
#[component]
pub fn CloudProviderPicker(
    /// Currently selected provider.
    selected: Option<CloudProvider>,
    /// Available provider options.
    options: Vec<CloudProviderOption>,
    /// Whether a sign-in is in progress.
    signing_in: bool,
    /// Sign-in error message.
    sign_in_error: Option<String>,

    // --- S3 edit state (shown when S3 is selected) ---
    s3_is_editing: bool,
    s3_bucket: String,
    s3_region: String,
    s3_endpoint: String,
    s3_access_key: String,
    s3_secret_key: String,

    // --- bae cloud form state ---
    bae_cloud_is_editing: bool,
    bae_cloud_mode: BaeCloudAuthMode,
    bae_cloud_email: String,
    bae_cloud_username: String,
    bae_cloud_password: String,

    // --- Callbacks ---
    on_select: EventHandler<CloudProvider>,
    on_sign_in: EventHandler<CloudProvider>,
    on_disconnect: EventHandler<()>,
    on_use_icloud: EventHandler<()>,
    // S3 callbacks
    on_s3_edit_start: EventHandler<()>,
    on_s3_cancel: EventHandler<()>,
    on_s3_save: EventHandler<SyncBucketConfig>,
    on_s3_bucket_change: EventHandler<String>,
    on_s3_region_change: EventHandler<String>,
    on_s3_endpoint_change: EventHandler<String>,
    on_s3_access_key_change: EventHandler<String>,
    on_s3_secret_key_change: EventHandler<String>,
    // bae cloud callbacks
    on_bae_cloud_mode_change: EventHandler<BaeCloudAuthMode>,
    on_bae_cloud_email_change: EventHandler<String>,
    on_bae_cloud_username_change: EventHandler<String>,
    on_bae_cloud_password_change: EventHandler<String>,
    on_bae_cloud_submit: EventHandler<()>,
) -> Element {
    let s3_has_required = !s3_bucket.is_empty()
        && !s3_region.is_empty()
        && !s3_access_key.is_empty()
        && !s3_secret_key.is_empty();

    let bae_cloud_has_required = !bae_cloud_email.is_empty()
        && !bae_cloud_password.is_empty()
        && (bae_cloud_mode == BaeCloudAuthMode::LogIn || !bae_cloud_username.is_empty());

    rsx! {
        SettingsCard {
            div { class: "mb-4",
                h3 { class: "text-lg font-medium text-white", "Cloud Home" }
                p { class: "text-sm text-gray-400 mt-1",
                    "Where should bae store your library data for sync?"
                }
            }
            if let Some(ref err) = sign_in_error {
                div { class: "mb-4 p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                    "{err}"
                }
            }
            div { class: "space-y-1",
                for option in options.iter() {
                    {
                        let is_selected = selected.as_ref() == Some(&option.provider);
                        let provider_for_select = option.provider.clone();
                        let provider_for_sign_in = option.provider.clone();
                        let connected = option.connected_account.clone();
                        let is_s3 = option.provider == CloudProvider::S3;
                        let is_icloud = option.provider == CloudProvider::ICloud;
                        let is_bae_cloud = option.provider == CloudProvider::BaeCloud;
                        let needs_oauth = matches!(
                            option.provider,
                            CloudProvider::GoogleDrive | CloudProvider::Dropbox | CloudProvider::OneDrive
                        );
                        let label = option.label;
                        let description = option.description;
                        rsx! {
                            div {
                                key: "{label}",
                                class: "p-3 rounded-lg cursor-pointer transition-colors",
                                class: if is_selected { "bg-gray-700/50 border border-gray-600" } else { "hover:bg-gray-700/30 border border-transparent" },
                                onclick: move |_| {
                                    on_select.call(provider_for_select.clone());
                                },
                                div { class: "flex items-start gap-3",
                                    div { class: "mt-0.5 flex-shrink-0",
                                        div {
                                            class: "w-4 h-4 rounded-full border-2 flex items-center justify-center",
                                            class: if is_selected { "border-blue-500" } else { "border-gray-500" },
                                            if is_selected {
                                                div { class: "w-2 h-2 rounded-full bg-blue-500" }
                                            }
                                        }
                                    }
                                    div { class: "flex-1 min-w-0",
                                        span { class: "text-sm font-medium text-gray-200", "{label}" }
                                        if let Some(ref account) = connected {
                                            div { class: "flex items-center gap-2 mt-1",
                                                span { class: "text-xs text-green-400", "Connected as {account}" }
                                                if is_selected {
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        size: ButtonSize::Small,
                                                        onclick: move |evt: Event<MouseData>| {
                                                            evt.stop_propagation();
                                                            on_disconnect.call(());
                                                        },
                                                        "Disconnect"
                                                    }
                                                }
                                            }
                                        } else {
                                            p { class: "text-xs text-gray-500 mt-0.5", "{description}" }
                                        }
                                        if is_selected && connected.is_none() {
                                            div { class: "mt-2",
                                                if signing_in {
                                                    div { class: "flex items-center gap-2 text-sm text-gray-400",
                                                        LoadingSpinner {}
                                                        "Signing in..."
                                                    }
                                                } else if is_icloud {
                                                    Button {
                                                        variant: ButtonVariant::Primary,
                                                        size: ButtonSize::Small,
                                                        onclick: move |evt: Event<MouseData>| {
                                                            evt.stop_propagation();
                                                            on_use_icloud.call(());
                                                        },
                                                        "Use iCloud Drive"
                                                    }
                                                } else if needs_oauth {
                                                    Button {
                                                        variant: ButtonVariant::Primary,
                                                        size: ButtonSize::Small,
                                                        onclick: {
                                                            let p = provider_for_sign_in.clone();
                                                            move |evt: Event<MouseData>| {
                                                                evt.stop_propagation();
                                                                on_sign_in.call(p.clone());
                                                            }
                                                        },
                                                        "Sign in with {label}"
                                                    }
                                                } else if is_s3 && !s3_is_editing {
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        size: ButtonSize::Small,
                                                        onclick: move |evt: Event<MouseData>| {
                                                            evt.stop_propagation();
                                                            on_s3_edit_start.call(());
                                                        },
                                                        "Configure"
                                                    }
                                                } else if is_bae_cloud && !bae_cloud_is_editing {
                            // noop: the form renders below
                            // bae cloud inline form
                            // Mode toggle
        
                                                    // S3 inline edit form
                                                }
                                            }
                                        }
                                        if is_selected && is_bae_cloud && bae_cloud_is_editing {
                                            div {
                                                class: "mt-3 space-y-3",
                                                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                                                div { class: "flex gap-3 text-sm",
                                                    span {
                                                        class: if bae_cloud_mode == BaeCloudAuthMode::SignUp { "text-blue-400 cursor-default" } else { "text-gray-400 hover:text-gray-200 cursor-pointer" },
                                                        onclick: move |_| {
                                                            on_bae_cloud_mode_change.call(BaeCloudAuthMode::SignUp);
                                                        },
                                                        "Sign up"
                                                    }
                                                    span {
                                                        class: if bae_cloud_mode == BaeCloudAuthMode::LogIn { "text-blue-400 cursor-default" } else { "text-gray-400 hover:text-gray-200 cursor-pointer" },
                                                        onclick: move |_| {
                                                            on_bae_cloud_mode_change.call(BaeCloudAuthMode::LogIn);
                                                        },
                                                        "Log in"
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Email"
                                                    }
                                                    TextInput {
                                                        value: bae_cloud_email.to_string(),
                                                        on_input: move |v| on_bae_cloud_email_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Text,
                                                        placeholder: "you@example.com",
                                                    }
                                                }
                                                if bae_cloud_mode == BaeCloudAuthMode::SignUp {
                                                    div {
                                                        label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                            "Username"
                                                        }
                                                        TextInput {
                                                            value: bae_cloud_username.to_string(),
                                                            on_input: move |v| on_bae_cloud_username_change.call(v),
                                                            size: TextInputSize::Medium,
                                                            input_type: TextInputType::Text,
                                                            placeholder: "alice",
                                                        }
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Password"
                                                    }
                                                    TextInput {
                                                        value: bae_cloud_password.to_string(),
                                                        on_input: move |v| on_bae_cloud_password_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Password,
                                                        placeholder: "Password",
                                                    }
                                                }
                                                div { class: "flex gap-2",
                                                    Button {
                                                        variant: ButtonVariant::Primary,
                                                        size: ButtonSize::Small,
                                                        disabled: !bae_cloud_has_required,
                                                        onclick: move |evt: Event<MouseData>| {
                                                            evt.stop_propagation();
                                                            on_bae_cloud_submit.call(());
                                                        },
                                                        if bae_cloud_mode == BaeCloudAuthMode::SignUp {
                                                            "Sign up"
                                                        } else {
                                                            "Log in"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if is_selected && is_s3 && s3_is_editing {
                                            div {
                                                class: "mt-3 space-y-3",
                                                onclick: move |evt: Event<MouseData>| evt.stop_propagation(),
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Bucket"
                                                    }
                                                    TextInput {
                                                        value: s3_bucket.to_string(),
                                                        on_input: move |v| on_s3_bucket_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Text,
                                                        placeholder: "my-sync-bucket",
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Region"
                                                    }
                                                    TextInput {
                                                        value: s3_region.to_string(),
                                                        on_input: move |v| on_s3_region_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Text,
                                                        placeholder: "us-east-1",
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Endpoint (optional)"
                                                    }
                                                    TextInput {
                                                        value: s3_endpoint.to_string(),
                                                        on_input: move |v| on_s3_endpoint_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Text,
                                                        placeholder: "https://s3.example.com",
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Access Key"
                                                    }
                                                    TextInput {
                                                        value: s3_access_key.to_string(),
                                                        on_input: move |v| on_s3_access_key_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Text,
                                                        placeholder: "AKIA...",
                                                    }
                                                }
                                                div {
                                                    label { class: "block text-xs font-medium text-gray-400 mb-1",
                                                        "Secret Key"
                                                    }
                                                    TextInput {
                                                        value: s3_secret_key.to_string(),
                                                        on_input: move |v| on_s3_secret_key_change.call(v),
                                                        size: TextInputSize::Medium,
                                                        input_type: TextInputType::Password,
                                                        placeholder: "Secret key",
                                                    }
                                                }
                                                div { class: "flex gap-2",
                                                    Button {
                                                        variant: ButtonVariant::Primary,
                                                        size: ButtonSize::Small,
                                                        disabled: !s3_has_required,
                                                        onclick: {
                                                            let config = SyncBucketConfig {
                                                                bucket: s3_bucket.to_string(),
                                                                region: s3_region.to_string(),
                                                                endpoint: s3_endpoint.to_string(),
                                                                access_key: s3_access_key.to_string(),
                                                                secret_key: s3_secret_key.to_string(),
                                                            };
                                                            move |evt: Event<MouseData>| {
                                                                evt.stop_propagation();
                                                                on_s3_save.call(config.clone());
                                                            }
                                                        },
                                                        "Save"
                                                    }
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        size: ButtonSize::Small,
                                                        onclick: move |evt: Event<MouseData>| {
                                                            evt.stop_propagation();
                                                            on_s3_cancel.call(());
                                                        },
                                                        "Cancel"
                                                    }
                                                }
                                            }
                                        }
                                        if is_selected && is_s3 && !s3_is_editing && connected.is_some() {
                                            div { class: "mt-2 text-xs text-gray-400 space-y-0.5",
                                                if !s3_bucket.is_empty() {
                                                    div { "Bucket: {s3_bucket}" }
                                                }
                                                if !s3_region.is_empty() {
                                                    div { "Region: {s3_region}" }
                                                }
                                                if !s3_endpoint.is_empty() {
                                                    div { "Endpoint: {s3_endpoint}" }
                                                }
                                                Button {
                                                    variant: ButtonVariant::Secondary,
                                                    size: ButtonSize::Small,
                                                    onclick: move |evt: Event<MouseData>| {
                                                        evt.stop_propagation();
                                                        on_s3_edit_start.call(());
                                                    },
                                                    "Edit"
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
    }
}
