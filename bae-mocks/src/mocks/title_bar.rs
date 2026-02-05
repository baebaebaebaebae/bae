//! TitleBarView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{
    AlbumResult, ArtistResult, GroupedSearchResults, NavItem, SearchAction, TitleBarView,
    TrackResult,
};
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
                ("none", "None"),
            ],
        )
        .inline()
        .bool_control("show_search_results", "Show Search Results", false)
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("With Search").set_bool("show_search_results", true),
        ])
        .build(initial_state);

    registry.use_url_sync_title_bar();

    let active_nav = registry.get_string("active_nav");
    let show_search_results_bool = registry.get_bool("show_search_results");
    let show_search_results: ReadSignal<bool> = use_memo(move || show_search_results_bool).into();

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
    ];

    let settings_active = active_nav == "settings";

    let search_results = if show_search_results_bool {
        mock_search_results()
    } else {
        GroupedSearchResults::default()
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::TitleBar,
            registry,
            max_width: "6xl",
            TitleBarView {
                nav_items,
                on_nav_click: |_| {},
                search_value: if show_search_results_bool { "glass".to_string() } else { String::new() },
                on_search_change: |_| {},
                search_results,
                on_search_result_click: |_: SearchAction| {},
                show_search_results,
                on_search_dismiss: |_| {},
                on_search_focus: |_| {},
                on_search_blur: |_| {},
                on_settings_click: |_| {},
                settings_active,
                left_padding: 16,
            }
        }
    }
}

fn mock_search_results() -> GroupedSearchResults {
    GroupedSearchResults {
        artists: vec![
            ArtistResult {
                id: "a1".to_string(),
                name: "Glass Harbor".to_string(),
                album_count: 2,
            },
            ArtistResult {
                id: "a2".to_string(),
                name: "Apartment Garden".to_string(),
                album_count: 2,
            },
        ],
        albums: vec![
            AlbumResult {
                id: "1".to_string(),
                title: "Pacific Standard".to_string(),
                artist_name: "Glass Harbor".to_string(),
                year: Some(2022),
                cover_url: Some("/covers/glass-harbor_pacific-standard.png".to_string()),
            },
            AlbumResult {
                id: "2".to_string(),
                title: "Landlocked".to_string(),
                artist_name: "Glass Harbor".to_string(),
                year: Some(2021),
                cover_url: Some("/covers/glass-harbor_landlocked.png".to_string()),
            },
            AlbumResult {
                id: "3".to_string(),
                title: "Grow Light".to_string(),
                artist_name: "Apartment Garden".to_string(),
                year: Some(2021),
                cover_url: Some("/covers/apartment-garden_grow-light.png".to_string()),
            },
        ],
        tracks: vec![
            TrackResult {
                id: "t1".to_string(),
                album_id: "1".to_string(),
                title: "Glass Ceiling".to_string(),
                artist_name: "Glass Harbor".to_string(),
                album_title: "Pacific Standard".to_string(),
                duration_ms: Some(245000),
            },
            TrackResult {
                id: "t2".to_string(),
                album_id: "3".to_string(),
                title: "Stained Glass".to_string(),
                artist_name: "Apartment Garden".to_string(),
                album_title: "Grow Light".to_string(),
                duration_ms: Some(198000),
            },
        ],
    }
}
