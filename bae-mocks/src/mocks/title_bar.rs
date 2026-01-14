//! TitleBarView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{NavItem, SearchResult, TitleBarView};
use dioxus::prelude::*;

#[component]
pub fn TitleBarMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "active_nav",
            "Active Nav",
            "library",
            vec![
                ("library", "Library"),
                ("import", "Import"),
                ("settings", "Settings"),
            ],
        )
        .inline()
        .enum_control(
            "update_state",
            "Update State",
            "idle",
            vec![
                ("idle", "Idle"),
                ("downloading", "Downloading"),
                ("ready", "Ready"),
            ],
        )
        .inline()
        .bool_control("show_search_results", "Show Search Results", false)
        .int_control("search_results_count", "Search Results", 3, 0, Some(10))
        .visible_when("show_search_results", "true")
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Update Ready").set_string("update_state", "ready"),
            Preset::new("Downloading").set_string("update_state", "downloading"),
            Preset::new("With Search")
                .set_bool("show_search_results", true)
                .set_int("search_results_count", 5),
        ])
        .build(initial_state);

    registry.use_url_sync_title_bar();

    let active_nav = registry.get_string("active_nav");
    let update_state = registry.get_string("update_state");
    let show_search_results = registry.get_bool("show_search_results");
    let search_results_count = registry.get_int("search_results_count") as usize;

    let nav_items = vec![
        NavItem {
            id: "library".to_string(),
            label: "Library".to_string(),
            is_active: active_nav == "library",
        },
        NavItem {
            id: "import".to_string(),
            label: "Import".to_string(),
            is_active: active_nav == "import",
        },
        NavItem {
            id: "settings".to_string(),
            label: "Settings".to_string(),
            is_active: active_nav == "settings",
        },
    ];

    let search_results: Vec<SearchResult> = if show_search_results {
        mock_search_results()
            .into_iter()
            .take(search_results_count)
            .collect()
    } else {
        vec![]
    };

    let update_indicator = match update_state.as_str() {
        "ready" => rsx! {
            UpdateIndicatorReady {}
        },
        "downloading" => rsx! {
            UpdateIndicatorDownloading {}
        },
        _ => rsx! {},
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::TitleBar,
            registry,
            max_width: "6xl",
            TitleBarView {
                nav_items,
                on_nav_click: |_| {},
                search_value: if show_search_results { "glass".to_string() } else { String::new() },
                on_search_change: |_| {},
                search_results,
                on_search_result_click: |_| {},
                show_search_results,
                on_search_dismiss: |_| {},
                on_search_focus: |_| {},
                imports_indicator: rsx! {
                    {update_indicator}
                },
                left_padding: 16,
                relative: true,
            }
        }
    }
}

/// Mock update indicator - ready state (matches desktop implementation)
#[component]
fn UpdateIndicatorReady() -> Element {
    rsx! {
        button {
            class: "flex items-center gap-1.5 px-2 py-1 text-xs text-emerald-400 hover:text-emerald-300 hover:bg-emerald-900/30 rounded transition-colors",
            title: "Update ready - click to restart",
            span { class: "relative flex h-2 w-2",
                span { class: "animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" }
                span { class: "relative inline-flex rounded-full h-2 w-2 bg-emerald-500" }
            }
            "Update"
        }
    }
}

/// Mock update indicator - downloading state
#[component]
fn UpdateIndicatorDownloading() -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1.5 px-2 py-1 text-xs text-gray-400",
            title: "Downloading update...",
            svg {
                class: "animate-spin h-3 w-3",
                xmlns: "http://www.w3.org/2000/svg",
                fill: "none",
                view_box: "0 0 24 24",
                circle {
                    class: "opacity-25",
                    cx: "12",
                    cy: "12",
                    r: "10",
                    stroke: "currentColor",
                    stroke_width: "4",
                }
                path {
                    class: "opacity-75",
                    fill: "currentColor",
                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                }
            }
            "Updating..."
        }
    }
}

fn mock_search_results() -> Vec<SearchResult> {
    vec![
        SearchResult {
            id: "1".to_string(),
            title: "Pacific Standard".to_string(),
            subtitle: "Glass Harbor • 2022".to_string(),
            cover_url: Some("/covers/glass-harbor_pacific-standard.png".to_string()),
        },
        SearchResult {
            id: "2".to_string(),
            title: "Landlocked".to_string(),
            subtitle: "Glass Harbor • 2021".to_string(),
            cover_url: Some("/covers/glass-harbor_landlocked.png".to_string()),
        },
        SearchResult {
            id: "3".to_string(),
            title: "Grow Light".to_string(),
            subtitle: "Apartment Garden • 2021".to_string(),
            cover_url: Some("/covers/apartment-garden_grow-light.png".to_string()),
        },
        SearchResult {
            id: "4".to_string(),
            title: "Set Theory".to_string(),
            subtitle: "Velvet Mathematics • 2023".to_string(),
            cover_url: Some("/covers/velvet-mathematics_set-theory.png".to_string()),
        },
        SearchResult {
            id: "5".to_string(),
            title: "Neon Frequencies".to_string(),
            subtitle: "The Midnight Signal • 2023".to_string(),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        },
    ]
}
