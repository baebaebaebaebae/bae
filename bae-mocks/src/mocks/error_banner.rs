//! ErrorBanner mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::ErrorBanner;
use dioxus::prelude::*;

#[component]
pub fn ErrorBannerMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .string_control("heading", "Heading", "Import failed")
        .string_control("detail", "Detail", "Connection timed out after 30s")
        .string_control("button_label", "Button Label", "Retry Import")
        .with_presets(vec![
            Preset::new("Import Failed"),
            Preset::new("Lookup Failed")
                .set_string("heading", "Lookup failed")
                .set_string("detail", "MusicBrainz API returned 503 Service Unavailable")
                .set_string("button_label", "Retry Lookup"),
            Preset::new("Long Error")
                .set_string("heading", "Import failed")
                .set_string(
                    "detail",
                    "Failed to write file: Permission denied (os error 13) while writing to /Volumes/Music/Library/Artist/Album/01 - Track.flac",
                )
                .set_string("button_label", "Retry Import"),
        ])
        .build(initial_state);

    registry.use_url_sync_button();

    let heading = registry.get_string("heading");
    let detail = registry.get_string("detail");
    let button_label = registry.get_string("button_label");

    rsx! {
        MockPanel { current_mock: MockPage::ErrorBanner, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "ErrorBanner Component" }

                // Interactive demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "max-w-lg",
                        ErrorBanner {
                            heading,
                            detail,
                            button_label,
                            on_retry: |_| {},
                        }
                    }
                }

                // Use cases
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Use Cases" }
                    div { class: "max-w-lg space-y-4",
                        ErrorBanner {
                            heading: "Import failed".to_string(),
                            detail: "Connection timed out after 30s".to_string(),
                            button_label: "Retry Import".to_string(),
                            on_retry: |_| {},
                        }
                        ErrorBanner {
                            heading: "Lookup failed".to_string(),
                            detail: "MusicBrainz API returned 503 Service Unavailable".to_string(),
                            button_label: "Retry Lookup".to_string(),
                            on_retry: |_| {},
                        }
                    }
                }
            }
        }
    }
}
