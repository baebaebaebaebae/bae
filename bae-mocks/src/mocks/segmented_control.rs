//! SegmentedControl mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{ButtonVariant, Segment, SegmentedControl};
use dioxus::prelude::*;

#[component]
pub fn SegmentedControlMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "variant",
            "Selected Variant",
            "primary",
            vec![("primary", "Primary"), ("secondary", "Secondary")],
        )
        .enum_control(
            "count",
            "Segments",
            "2",
            vec![("2", "2"), ("3", "3"), ("4", "4")],
        )
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("3 Segments").set_string("count", "3"),
            Preset::new("Secondary Variant")
                .set_string("variant", "secondary")
                .set_string("count", "3"),
        ])
        .build(initial_state);

    registry.use_url_sync_segmented_control();

    let variant_str = registry.get_string("variant");
    let count_str = registry.get_string("count");

    let selected_variant = match variant_str.as_str() {
        "secondary" => ButtonVariant::Secondary,
        _ => ButtonVariant::Primary,
    };

    let count: usize = count_str.parse().unwrap_or(2);

    let all_labels = ["Alpha", "Beta", "Gamma", "Delta"];
    let all_values = ["alpha", "beta", "gamma", "delta"];

    let segments: Vec<Segment> = (0..count)
        .map(|i| Segment::new(all_labels[i], all_values[i]))
        .collect();

    let mut selected = use_signal(|| "alpha".to_string());

    rsx! {
        MockPanel { current_mock: MockPage::SegmentedControl, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "SegmentedControl Component" }

                // Interactive demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "flex items-center gap-4",
                        SegmentedControl {
                            segments: segments.clone(),
                            selected: selected.read().clone(),
                            selected_variant,
                            on_select: move |value: &str| {
                                selected.set(value.to_string());
                            },
                        }
                        span { class: "text-sm text-gray-500", "Selected: {selected}" }
                    }
                }

                // Both variants side by side
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Variants" }
                    div { class: "flex flex-wrap items-center gap-6",
                        div {
                            p { class: "text-xs text-gray-500 mb-2", "Primary" }
                            SegmentedControl {
                                segments: vec![
                                    Segment::new("Title", "title"),
                                    Segment::new("Catalog #", "catalog"),
                                    Segment::new("Barcode", "barcode"),
                                ],
                                selected: "title".to_string(),
                                selected_variant: ButtonVariant::Primary,
                                on_select: |_| {},
                            }
                        }
                        div {
                            p { class: "text-xs text-gray-500 mb-2", "Secondary" }
                            SegmentedControl {
                                segments: vec![
                                    Segment::new("Folder", "folder"),
                                    Segment::new("Torrent", "torrent"),
                                    Segment::new("CD", "cd"),
                                ],
                                selected: "folder".to_string(),
                                selected_variant: ButtonVariant::Secondary,
                                on_select: |_| {},
                            }
                        }
                    }
                }

                // With disabled segments
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Disabled Segments" }
                    SegmentedControl {
                        segments: vec![
                            Segment::new("Folder", "folder"),
                            Segment::new("Torrent", "torrent").disabled(),
                            Segment::new("CD", "cd").disabled(),
                        ],
                        selected: "folder".to_string(),
                        selected_variant: ButtonVariant::Secondary,
                        on_select: |_| {},
                    }
                }

                // Real-world examples
                div {
                    h3 { class: "text-sm text-gray-400 mb-3", "Usage Examples" }
                    div { class: "space-y-4",
                        div {
                            p { class: "text-xs text-gray-500 mb-2", "Search source toggle" }
                            SegmentedControl {
                                segments: vec![Segment::new("MusicBrainz", "mb"), Segment::new("Discogs", "dc")],
                                selected: "mb".to_string(),
                                selected_variant: ButtonVariant::Primary,
                                on_select: |_| {},
                            }
                        }
                        div {
                            p { class: "text-xs text-gray-500 mb-2", "Search tabs" }
                            SegmentedControl {
                                segments: vec![
                                    Segment::new("Title", "title"),
                                    Segment::new("Catalog #", "catalog"),
                                    Segment::new("Barcode", "barcode"),
                                ],
                                selected: "catalog".to_string(),
                                selected_variant: ButtonVariant::Primary,
                                on_select: |_| {},
                            }
                        }
                    }
                }
            }
        }
    }
}
