//! Share grant dialog — create a share grant token for a release

use crate::components::icons::{AlertTriangleIcon, XIcon};
use crate::components::Modal;
use dioxus::prelude::*;

#[component]
pub fn ShareGrantDialog(
    is_open: ReadSignal<bool>,
    on_close: EventHandler<()>,
    /// The grant JSON to display (Some after successful creation)
    grant_json: Option<String>,
    /// Error message from grant creation
    grant_error: Option<String>,
    /// Whether the release is on a cloud storage profile
    has_cloud_profile: bool,
    /// Called with the recipient's public key hex to create a grant
    on_create_grant: EventHandler<String>,
) -> Element {
    let mut recipient_pubkey = use_signal(String::new);

    // Reset input when dialog opens/closes
    use_effect(move || {
        let open = is_open();
        if !open {
            recipient_pubkey.set(String::new());
        }
    });

    let can_create = !recipient_pubkey().is_empty() && has_cloud_profile;

    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-lg w-full mx-4 max-h-[80vh] flex flex-col",
                // Header
                div { class: "flex items-center justify-between px-6 pt-6 pb-4 border-b border-gray-700",
                    h2 { class: "text-xl font-bold text-white", "Share Release" }
                    button {
                        class: "text-gray-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }

                div { class: "p-6 space-y-4 overflow-y-auto flex-1",
                    if !has_cloud_profile {
                        // Not on cloud storage — can't share
                        div { class: "flex items-start gap-3 p-4 bg-yellow-900/30 border border-yellow-700/50 rounded-lg",
                            AlertTriangleIcon { class: "w-5 h-5 text-yellow-400 shrink-0 mt-0.5" }
                            div {
                                div { class: "text-sm font-medium text-yellow-300",
                                    "Cloud storage required"
                                }
                                div { class: "text-xs text-yellow-400 mt-1",
                                    "Transfer this release to a cloud storage profile before sharing."
                                }
                            }
                        }
                    } else if grant_json.is_some() {
                        // Success — show the grant JSON
                        div { class: "space-y-3",
                            div { class: "text-sm text-gray-300",
                                "Share grant created. Copy this token and send it to the recipient."
                            }
                            textarea {
                                class: "w-full h-48 bg-gray-900 text-gray-200 text-xs font-mono p-3 rounded-lg border border-gray-600 resize-none focus:outline-none focus:border-blue-500",
                                readonly: true,
                                value: grant_json.clone().unwrap_or_default(),
                            }
                        }
                    } else {
                        // Input form
                        div { class: "space-y-4",
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-2",
                                    "Recipient's public key"
                                }
                                input {
                                    class: "w-full bg-gray-900 text-gray-200 text-sm font-mono px-3 py-2 rounded-lg border border-gray-600 focus:outline-none focus:border-blue-500",
                                    r#type: "text",
                                    placeholder: "Paste hex-encoded Ed25519 public key...",
                                    value: recipient_pubkey(),
                                    oninput: move |evt| {
                                        recipient_pubkey.set(evt.value());
                                    },
                                }
                                div { class: "text-xs text-gray-500 mt-1",
                                    "The recipient can find their public key in Settings."
                                }
                            }

                            if let Some(ref error) = grant_error {
                                div { class: "flex items-start gap-3 p-4 bg-red-900/30 border border-red-700/50 rounded-lg",
                                    AlertTriangleIcon { class: "w-5 h-5 text-red-400 shrink-0 mt-0.5" }
                                    div {
                                        div { class: "text-sm font-medium text-red-300",
                                            "Failed to create share grant"
                                        }
                                        div { class: "text-xs text-red-400 mt-1", {error.clone()} }
                                    }
                                }
                            }

                            button {
                                class: "w-full py-2 px-4 rounded-lg text-sm font-medium transition-colors",
                                class: if can_create { "bg-blue-600 hover:bg-blue-500 text-white" } else { "bg-gray-700 text-gray-500 cursor-not-allowed" },
                                disabled: !can_create,
                                onclick: {
                                    move |_| {
                                        let pubkey = recipient_pubkey();
                                        if !pubkey.is_empty() {
                                            on_create_grant.call(pubkey);
                                        }
                                    }
                                },
                                "Create Share Grant"
                            }
                        }
                    }
                }
            }
        }
    }
}
