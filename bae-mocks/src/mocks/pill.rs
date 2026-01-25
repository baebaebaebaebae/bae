//! Pill mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{Pill, PillVariant};
use dioxus::prelude::*;

#[component]
pub fn PillMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "variant",
            "Variant",
            "muted",
            vec![("muted", "Muted"), ("link", "Link")],
        )
        .bool_control("monospace", "Monospace", false)
        .bool_control("has_link", "Has Link", false)
        .with_presets(vec![
            Preset::new("Token (Muted)"),
            Preset::new("Link Pill")
                .set_string("variant", "link")
                .set_bool("has_link", true),
            Preset::new("Disc ID")
                .set_string("variant", "link")
                .set_bool("monospace", true)
                .set_bool("has_link", true),
        ])
        .build(initial_state);

    registry.use_url_sync_button();

    let variant_str = registry.get_string("variant");
    let monospace = registry.get_bool("monospace");
    let has_link = registry.get_bool("has_link");

    let variant = match variant_str.as_str() {
        "link" => PillVariant::Link,
        _ => PillVariant::Muted,
    };

    let href = if has_link {
        Some("https://musicbrainz.org/cdtoc/XzPS7vW.HPHsYemQh0HBUGr8vuU-".to_string())
    } else {
        None
    };

    let label = if monospace {
        "XzPS7vW.HPHsYemQh0HBUGr8vuU-"
    } else {
        "Example Token"
    };

    rsx! {
        MockPanel { current_mock: MockPage::Pill, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "Pill Component" }

                // Interactive demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "flex items-center gap-4",
                        Pill { variant, href, monospace, "{label}" }
                    }
                }

                // All variants showcase
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "All Variants" }
                    div { class: "flex flex-wrap items-center gap-3",
                        Pill { variant: PillVariant::Muted, "Muted" }
                        Pill { variant: PillVariant::Link, "Link" }
                        Pill {
                            variant: PillVariant::Link,
                            href: "https://example.com",
                            "With URL"
                        }
                        Pill { variant: PillVariant::Link, monospace: true, "monospace" }
                    }
                }

                // Use case: Search tokens
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Use Case: Search Tokens" }
                    div { class: "flex flex-wrap gap-1.5",
                        Pill { variant: PillVariant::Muted, "The Midnight Signal" }
                        Pill { variant: PillVariant::Muted, "Neon Frequencies" }
                        Pill { variant: PillVariant::Muted, "2023" }
                    }
                }

                // Use case: Disc ID
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Use Case: Disc ID Badge" }
                    p { class: "text-sm text-gray-400",
                        "Disc ID "
                        Pill {
                            variant: PillVariant::Link,
                            href: "https://musicbrainz.org/cdtoc/XzPS7vW.HPHsYemQh0HBUGr8vuU-",
                            monospace: true,
                            "XzPS7vW.HPHsYemQh0HBUGr8vuU-"
                        }
                        " matches multiple releases."
                    }
                }
            }
        }
    }
}
