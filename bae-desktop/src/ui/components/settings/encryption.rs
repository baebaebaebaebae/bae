//! Encryption section wrapper - reads config, delegates UI to EncryptionSectionView

use crate::ui::app_service::use_app;
use bae_ui::EncryptionSectionView;
use dioxus::prelude::*;

/// Encryption section - read-only key status
#[component]
pub fn EncryptionSection() -> Element {
    let app = use_app();
    let config = app.config.clone();

    // Handle optional encryption key
    let (key_preview, key_length, is_configured) = if let Some(ref key) = config.encryption_key {
        let preview = if key.len() > 16 {
            format!("{}...{}", &key[..8], &key[key.len() - 8..])
        } else {
            "***".to_string()
        };
        let length = key.len() / 2;
        (preview, length, true)
    } else {
        ("Not configured".to_string(), 0, false)
    };

    rsx! {
        EncryptionSectionView { is_configured, key_preview, key_length }
    }
}
