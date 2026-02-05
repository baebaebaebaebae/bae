//! Discogs section view

use crate::components::{
    Button, ButtonSize, ButtonVariant, TextInput, TextInputSize, TextInputType,
};
use dioxus::prelude::*;

/// Discogs API key configuration
#[component]
pub fn DiscogsSectionView(
    /// Whether a Discogs key is configured (don't pass the actual key for security)
    discogs_configured: bool,
    /// Current key value when editing (masked or empty)
    discogs_key_value: String,
    /// Whether currently in edit mode
    is_editing: bool,
    /// Whether saving is in progress
    is_saving: bool,
    /// Whether there are unsaved changes
    has_changes: bool,
    /// Error message if save failed
    save_error: Option<String>,
    on_edit_start: EventHandler<()>,
    on_key_change: EventHandler<String>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Discogs" }
            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        div {
                            h3 { class: "text-lg font-medium text-white", "API Key" }
                            p { class: "text-sm text-gray-400 mt-1",
                                "Used for release metadata and cover art"
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

                    if is_editing {
                        div { class: "space-y-4",
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-2",
                                    "API Key"
                                }
                                TextInput {
                                    value: discogs_key_value.to_string(),
                                    on_input: move |v| on_key_change.call(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Password,
                                    placeholder: "Enter your Discogs API key",
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
                                        "Save"
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
                    } else {
                        div { class: "flex items-center gap-3",
                            div { class: "flex-1 px-4 py-2 bg-gray-700 rounded-lg text-gray-400 font-mono",
                                "••••••••••••••••"
                            }
                            if discogs_configured {
                                span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm",
                                    "Configured"
                                }
                            } else {
                                span { class: "px-3 py-1 bg-gray-700 text-gray-400 rounded-full text-sm",
                                    "Not Set"
                                }
                            }
                        }
                    }
                }

                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "Get your Discogs API key from "
                        a {
                            class: "text-indigo-400 hover:text-indigo-300",
                            href: "https://www.discogs.com/settings/developers",
                            target: "_blank",
                            "discogs.com/settings/developers"
                        }
                    }
                }
            }
        }
    }
}
