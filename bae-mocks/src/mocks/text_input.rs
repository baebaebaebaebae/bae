//! TextInput mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{TextInput, TextInputSize, TextInputType};
use dioxus::prelude::*;

#[component]
pub fn TextInputMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "size",
            "Size",
            "medium",
            vec![("small", "Small"), ("medium", "Medium")],
        )
        .bool_control("disabled", "Disabled", false)
        .bool_control("has_placeholder", "Has Placeholder", true)
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Small").set_string("size", "small"),
            Preset::new("Disabled").set_bool("disabled", true),
            Preset::new("No Placeholder").set_bool("has_placeholder", false),
        ])
        .build(initial_state);

    registry.use_url_sync_button();

    let size_str = registry.get_string("size");
    let disabled = registry.get_bool("disabled");
    let has_placeholder = registry.get_bool("has_placeholder");

    let size = match size_str.as_str() {
        "small" => TextInputSize::Small,
        _ => TextInputSize::Medium,
    };

    let placeholder = if has_placeholder {
        Some("Enter text...")
    } else {
        None
    };

    let mut value = use_signal(|| "The Midnight Signal".to_string());

    rsx! {
        MockPanel { current_mock: MockPage::TextInput, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "TextInput Component" }

                // Interactive demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "max-w-md",
                        TextInput {
                            value: value(),
                            on_input: move |v| value.set(v),
                            size,
                            input_type: TextInputType::Text,
                            placeholder,
                            disabled,
                        }
                    }
                }

                // All sizes
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Sizes" }
                    div { class: "space-y-3 max-w-md",
                        div {
                            label { class: "block text-xs text-gray-400 mb-1.5", "Small" }
                            TextInput {
                                value: "Small input".to_string(),
                                on_input: move |_| {},
                                size: TextInputSize::Small,
                                input_type: TextInputType::Text,
                            }
                        }
                        div {
                            label { class: "block text-xs text-gray-400 mb-1.5", "Medium" }
                            TextInput {
                                value: "Medium input".to_string(),
                                on_input: move |_| {},
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                            }
                        }
                    }
                }

                // States
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "States" }
                    div { class: "space-y-3 max-w-md",
                        div {
                            label { class: "block text-xs text-gray-400 mb-1.5", "With placeholder" }
                            TextInput {
                                value: "".to_string(),
                                on_input: move |_| {},
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                placeholder: "Search artist or album...",
                            }
                        }
                        div {
                            label { class: "block text-xs text-gray-400 mb-1.5", "Disabled" }
                            TextInput {
                                value: "Cannot edit".to_string(),
                                on_input: move |_| {},
                                size: TextInputSize::Medium,
                                input_type: TextInputType::Text,
                                disabled: true,
                            }
                        }
                    }
                }

                // Use case: Search form
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Use Case: Search Form" }
                    div { class: "bg-gray-800/20 rounded-lg p-4 max-w-2xl",
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Artist" }
                                TextInput {
                                    value: "The Midnight Signal".to_string(),
                                    on_input: move |_| {},
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                }
                            }
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Album" }
                                TextInput {
                                    value: "Neon Frequencies".to_string(),
                                    on_input: move |_| {},
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
