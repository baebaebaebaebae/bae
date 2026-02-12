//! TitleBarView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{
    ActiveImport, AlbumResult, ArtistResult, GroupedSearchResults, ImportStatus,
    ImportsDropdownView, NavItem, SearchAction, TitleBarView, TrackResult,
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

    // When the mock toggle is on, always provide results (the panel shows on focus)
    let search_results = if show_search_results_bool {
        mock_search_results()
    } else {
        GroupedSearchResults::default()
    };

    // Mock imports for split button + dropdown
    let mut mock_imports = use_signal(mock_active_imports);
    let mut imports_dropdown_open = use_signal(|| false);
    let imports_dropdown_open_read: ReadSignal<bool> = imports_dropdown_open.into();
    let import_count = mock_imports.read().len();

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
                on_search_focus: |_| {},
                on_search_blur: |_| {},
                on_settings_click: |_| {},
                settings_active,
                import_count,
                show_imports_dropdown: Some(imports_dropdown_open_read),
                on_imports_dropdown_toggle: Some(EventHandler::new(move |_| imports_dropdown_open.toggle())),
                on_imports_dropdown_close: Some(EventHandler::new(move |_| imports_dropdown_open.set(false))),
                imports_dropdown_content: rsx! {
                    ImportsDropdownView {
                        imports: mock_imports(),
                        on_import_click: |_id: String| {},
                        on_import_dismiss: move |id: String| {
                            mock_imports.with_mut(|list| list.retain(|i| i.import_id != id));
                            if mock_imports.read().is_empty() {
                                imports_dropdown_open.set(false);
                            }
                        },
                        on_clear_all: move |_| {
                            mock_imports.set(vec![]);
                            imports_dropdown_open.set(false);
                        },
                    }
                },
                left_padding: 16,
            }
        }
    }
}

fn mock_active_imports() -> Vec<ActiveImport> {
    vec![
        ActiveImport {
            import_id: "imp-1".to_string(),
            album_title: "Pacific Standard".to_string(),
            artist_name: "Glass Harbor".to_string(),
            status: ImportStatus::Importing,
            current_step_text: None,
            progress_percent: Some(65),
            release_id: None,
        },
        ActiveImport {
            import_id: "imp-2".to_string(),
            album_title: "Grow Light".to_string(),
            artist_name: "Apartment Garden".to_string(),
            status: ImportStatus::Complete,
            current_step_text: None,
            progress_percent: None,
            release_id: Some("release-3".to_string()),
        },
        ActiveImport {
            import_id: "imp-3".to_string(),
            album_title: "Neon Frequencies".to_string(),
            artist_name: "The Midnight Signal".to_string(),
            status: ImportStatus::Preparing,
            current_step_text: Some("Parsing metadata...".to_string()),
            progress_percent: None,
            release_id: None,
        },
    ]
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
