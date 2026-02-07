//! Unlock screen for key recovery
//!
//! Shown when `config.encryption_key_stored` is true but the key is missing
//! from the keyring (keyring wiped, new device without iCloud Keychain sync).
//! User pastes their recovery key, we validate the fingerprint, save to keyring,
//! and re-exec the binary.

use bae_core::encryption::compute_key_fingerprint;
use bae_core::keys::KeyService;
use bae_ui::components::button::{Button, ButtonSize, ButtonVariant};
use bae_ui::components::text_input::{TextInput, TextInputSize, TextInputType};
use dioxus::prelude::*;
use tracing::{error, info};

use crate::ui::app::MAIN_CSS;
use crate::ui::app::TAILWIND_CSS;

#[derive(Clone)]
struct UnlockContext {
    key_service: KeyService,
    expected_fingerprint: String,
}

/// Launch a minimal Dioxus app with the unlock screen.
pub fn launch_unlock(key_service: KeyService, expected_fingerprint: String) {
    let config = dioxus::desktop::Config::default()
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title("bae")
                .with_inner_size(dioxus::desktop::LogicalSize::new(500, 400))
                .with_resizable(false)
                .with_decorations(true)
                .with_transparent(true)
                .with_background_color((0x0f, 0x11, 0x16, 0xff)),
        )
        .with_background_color((0x0f, 0x11, 0x16, 0xff));

    let ctx = UnlockContext {
        key_service,
        expected_fingerprint,
    };

    LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context_provider(move || Box::new(ctx.clone()))
        .launch(UnlockApp);
}

#[component]
fn UnlockApp() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        UnlockScreen {}
    }
}

#[derive(Clone, PartialEq)]
enum UnlockStatus {
    Idle,
    Error(String),
}

#[component]
fn UnlockScreen() -> Element {
    let mut key_input = use_signal(String::new);
    let mut status = use_signal(|| UnlockStatus::Idle);

    let on_submit = move |_| {
        let key_hex = key_input.read().trim().to_string();
        let ctx = use_context::<UnlockContext>();

        let fingerprint = match compute_key_fingerprint(&key_hex) {
            Some(fp) => fp,
            None => {
                status.set(UnlockStatus::Error(
                    "Invalid key. Must be 64 hex characters (32 bytes).".into(),
                ));
                return;
            }
        };

        if fingerprint != ctx.expected_fingerprint {
            status.set(UnlockStatus::Error(format!(
                "Wrong key. Fingerprint {fingerprint} does not match expected {}.",
                ctx.expected_fingerprint,
            )));
            return;
        }

        match ctx.key_service.set_encryption_key(&key_hex) {
            Ok(()) => {
                info!("Encryption key restored to keyring, re-launching");
                super::welcome::relaunch();
            }
            Err(e) => {
                error!("Failed to save key to keyring: {e}");
                status.set(UnlockStatus::Error(format!("Keyring error: {e}")));
            }
        }
    };

    rsx! {
        div { class: "flex flex-col items-center justify-center min-h-screen bg-gray-900 p-8",
            div { class: "max-w-md w-full",
                h1 { class: "text-3xl font-bold text-white text-center mb-2", "bae" }
                p { class: "text-gray-400 text-center mb-8",
                    "Your encryption key is missing from the keyring. Paste your recovery key to continue."
                }
                div { class: "space-y-4",
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-1",
                            "Recovery Key"
                        }
                        TextInput {
                            value: key_input.read().clone(),
                            on_input: move |v| key_input.set(v),
                            size: TextInputSize::Medium,
                            input_type: TextInputType::Password,
                            placeholder: "64-character hex key",
                            monospace: true,
                            autofocus: true,
                        }
                    }
                    match status.read().clone() {
                        UnlockStatus::Idle => rsx! {},
                        UnlockStatus::Error(msg) => rsx! {
                            div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                "{msg}"
                            }
                        },
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Medium,
                        onclick: on_submit,
                        "Unlock"
                    }
                }
            }
        }
    }
}
