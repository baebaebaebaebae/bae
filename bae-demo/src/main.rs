//! bae demo - Web demo for screenshot generation
//!
//! A minimal web app that renders UI components with fixture data.
//! Used for Playwright-based screenshot generation.

mod demo_data;

use bae_ui::{
    AboutSectionView, AlbumDetailView, ApiKeysSectionView, AppLayoutView, BackButton,
    BitTorrentSectionView, BitTorrentSettings, CdDriveStatus, CdSelectorView,
    EncryptionSectionView, ErrorDisplay, FolderSelectorView, ImportSource, ImportView, LibraryView,
    NavItem, NowPlayingBarView, PageContainer, PlaybackDisplay, QueueItem, QueueSidebarView,
    SearchResult, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfilesSectionView, SubsonicSectionView, TitleBarView, TorrentInputMode,
    TorrentInputView, Track, TrackImportState,
};
use dioxus::prelude::*;

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(DemoLayout)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id")]
    AlbumDetail { album_id: String },
    #[route("/import")]
    Import {},
    #[route("/settings")]
    Settings {},
}

/// Get a mock track for playback display
fn mock_playing_track() -> Track {
    Track {
        id: "mock-track-1".to_string(),
        title: "Neon Frequencies".to_string(),
        track_number: Some(1),
        disc_number: Some(1),
        duration_ms: Some(245_000),
        is_available: true,
        import_state: TrackImportState::Complete,
    }
}

/// Get mock queue items
fn mock_queue() -> Vec<QueueItem> {
    vec![
        QueueItem {
            track: Track {
                id: "queue-track-1".to_string(),
                title: "Signal Lost".to_string(),
                track_number: Some(2),
                disc_number: Some(1),
                duration_ms: Some(198_000),
                is_available: true,
                import_state: TrackImportState::Complete,
            },
            album_title: "Neon Frequencies".to_string(),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        },
        QueueItem {
            track: Track {
                id: "queue-track-2".to_string(),
                title: "Proof by Induction".to_string(),
                track_number: Some(1),
                disc_number: Some(1),
                duration_ms: Some(312_000),
                is_available: true,
                import_state: TrackImportState::Complete,
            },
            album_title: "Proof by Induction".to_string(),
            cover_url: Some("/covers/velvet-mathematics_proof-by-induction.png".to_string()),
        },
    ]
}

/// Layout component for demo app with full app chrome
#[component]
fn DemoLayout() -> Element {
    let current_route = use_route::<Route>();
    let mut search_query = use_signal(String::new);
    let mut show_search_results = use_signal(|| false);
    let mut is_playing = use_signal(|| true);
    let mut position_ms = use_signal(|| 45_000u64);
    let mut queue_open = use_signal(|| false);

    // Mock search - filter albums by query
    let search_results: Vec<SearchResult> = {
        let query = search_query().to_lowercase();
        if query.is_empty() {
            vec![]
        } else {
            let albums = demo_data::get_albums();
            let artists_by_album = demo_data::get_artists_by_album();
            albums
                .into_iter()
                .filter(|album| {
                    album.title.to_lowercase().contains(&query)
                        || artists_by_album
                            .get(&album.id)
                            .map(|artists| {
                                artists
                                    .iter()
                                    .any(|a| a.name.to_lowercase().contains(&query))
                            })
                            .unwrap_or(false)
                })
                .take(5)
                .map(|album| {
                    let artists = artists_by_album.get(&album.id).cloned().unwrap_or_default();
                    let artist_name = artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Unknown Artist".to_string());
                    SearchResult {
                        id: album.id,
                        title: album.title,
                        subtitle: artist_name,
                        cover_url: album.cover_url,
                    }
                })
                .collect()
        }
    };

    // Build nav items
    let nav_items = vec![
        NavItem {
            id: "library".to_string(),
            label: "Library".to_string(),
            is_active: matches!(current_route, Route::Library {} | Route::AlbumDetail { .. }),
        },
        NavItem {
            id: "import".to_string(),
            label: "Import".to_string(),
            is_active: matches!(current_route, Route::Import {}),
        },
        NavItem {
            id: "settings".to_string(),
            label: "Settings".to_string(),
            is_active: matches!(current_route, Route::Settings {}),
        },
    ];

    // Mock track for playback bar
    let mock_track = mock_playing_track();
    let playback = if is_playing() {
        PlaybackDisplay::Playing {
            track_id: mock_track.id.clone(),
            position_ms: position_ms(),
            duration_ms: 245_000,
        }
    } else {
        PlaybackDisplay::Paused {
            track_id: mock_track.id.clone(),
            position_ms: position_ms(),
            duration_ms: 245_000,
        }
    };

    // Current track for queue sidebar
    let current_queue_item = QueueItem {
        track: mock_track.clone(),
        album_title: "Neon Frequencies".to_string(),
        cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
    };

    rsx! {
        AppLayoutView {
            title_bar: rsx! {
                TitleBarView {
                    nav_items,
                    on_nav_click: move |id: String| {
                        let _ = match id.as_str() {
                            "library" => navigator().push(Route::Library {}),
                            "import" => navigator().push(Route::Import {}),
                            "settings" => navigator().push(Route::Settings {}),
                            _ => None,
                        };
                    },
                    search_value: search_query(),
                    on_search_change: move |value: String| {
                        search_query.set(value.clone());
                        show_search_results.set(!value.is_empty());
                    },
                    search_results,
                    on_search_result_click: move |album_id: String| {
                        show_search_results.set(false);
                        search_query.set(String::new());
                        navigator().push(Route::AlbumDetail { album_id });
                    },
                    show_search_results: show_search_results(),
                    on_search_dismiss: move |_| show_search_results.set(false),
                    on_search_focus: move |_| {
                        if !search_query().is_empty() {
                            show_search_results.set(true);
                        }
                    },
                    // No window drag/zoom on web
                    left_padding: 16,
                }
            },
            playback_bar: rsx! {
                NowPlayingBarView {
                    track: Some(mock_track),
                    artist_name: "The Midnight Signal".to_string(),
                    cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
                    playback,
                    position_ms: position_ms(),
                    duration_ms: 245_000,
                    on_previous: move |_| {},
                    on_pause: move |_| is_playing.set(false),
                    on_resume: move |_| is_playing.set(true),
                    on_next: move |_| {},
                    on_seek: move |pos| position_ms.set(pos),
                    on_toggle_queue: move |_| queue_open.toggle(),
                    on_track_click: move |_track_id: String| {},
                }
            },
            queue_sidebar: rsx! {
                QueueSidebarView {
                    is_open: queue_open(),
                    current_track: Some(current_queue_item),
                    queue: mock_queue(),
                    current_track_id: Some("mock-track-1".to_string()),
                    on_close: move |_| queue_open.set(false),
                    on_clear: move |_| {},
                    on_remove: move |_idx| {},
                    on_track_click: move |_track_id: String| {},
                }
            },
            Outlet::<Route> {}
        }
    }
}

/// Demo library page - uses static fixture data
#[component]
fn Library() -> Element {
    let albums = demo_data::get_albums();
    let artists_by_album = demo_data::get_artists_by_album();

    rsx! {
        LibraryView {
            albums,
            artists_by_album,
            loading: false,
            error: None,
            on_album_click: move |album_id: String| {
                navigator().push(Route::AlbumDetail { album_id });
            },
            on_play_album: |_| {},
            on_add_album_to_queue: |_| {},
        }
    }
}

/// Demo album detail page - uses bae-ui's AlbumDetailView with fixture data
#[component]
fn AlbumDetail(album_id: String) -> Element {
    let album = demo_data::get_album(&album_id);
    let artists = demo_data::get_artists_for_album(&album_id);
    let releases = demo_data::get_releases_for_album(&album_id);
    let tracks = demo_data::get_tracks_for_album(&album_id);

    // Create per-track signals for reactivity
    let track_signals: Vec<Signal<Track>> = tracks.into_iter().map(Signal::new).collect();

    // Signals for import state (not used in demo, but required by component)
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    let selected_release_id = releases.first().map(|r| r.id.clone());

    rsx! {
        PageContainer {
            BackButton {
                on_click: move |_| {
                    navigator().push(Route::Library {});
                },
            }

            if let Some(album) = album {
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    tracks: track_signals,
                    selected_release_id,
                    import_progress,
                    import_error,
                    playback: PlaybackDisplay::Stopped,
                    // Navigation callback
                    on_release_select: |_release_id: String| {},
                    // Album-level callbacks (no-ops for demo)
                    on_album_deleted: |_| {},
                    on_export_release: |_| {},
                    on_delete_album: |_| {},
                    on_delete_release: |_| {},
                    // Track playback callbacks (no-ops for demo)
                    on_track_play: |_| {},
                    on_track_pause: |_| {},
                    on_track_resume: |_| {},
                    on_track_add_next: |_| {},
                    on_track_add_to_queue: |_| {},
                    on_track_export: |_| {},
                    // Album playback callbacks (no-ops for demo)
                    on_play_album: |_| {},
                    on_add_album_to_queue: |_| {},
                }
            } else {
                ErrorDisplay { message: "Album not found in demo data".to_string() }
            }
        }
    }
}

/// Demo import page
#[component]
fn Import() -> Element {
    let mut selected_source = use_signal(|| ImportSource::Folder);

    rsx! {
        ImportView {
            selected_source: *selected_source.read(),
            on_source_select: move |source| selected_source.set(source),

            // Demo content for the selected import source
            match *selected_source.read() {
                ImportSource::Folder => rsx! {
                    FolderImportDemo {}
                },
                ImportSource::Torrent => rsx! {
                    TorrentImportDemo {}
                },
                ImportSource::Cd => rsx! {
                    CdImportDemo {}
                },
            }
        }
    }
}

/// Demo folder import UI
#[component]
fn FolderImportDemo() -> Element {
    rsx! {
        FolderSelectorView { on_select_click: |_| {} }
    }
}

/// Demo torrent import UI
#[component]
fn TorrentImportDemo() -> Element {
    let mut input_mode = use_signal(|| TorrentInputMode::File);

    rsx! {
        TorrentInputView {
            input_mode: *input_mode.read(),
            on_mode_change: move |mode| input_mode.set(mode),
            on_select_click: |_| {},
            on_magnet_submit: |_| {},
        }
    }
}

/// Demo CD import UI
#[component]
fn CdImportDemo() -> Element {
    rsx! {
        CdSelectorView { status: CdDriveStatus::NoDisc, on_rip_click: |_| {} }
    }
}

/// Demo settings page
#[component]
fn Settings() -> Element {
    let mut active_tab = use_signal(|| SettingsTab::StorageProfiles);

    rsx! {
        SettingsView {
            active_tab: *active_tab.read(),
            on_tab_change: move |tab| active_tab.set(tab),

            match *active_tab.read() {
                SettingsTab::StorageProfiles => rsx! {
                    StorageProfilesSectionView {
                        profiles: mock_storage_profiles(),
                        is_loading: false,
                        editing_profile: None,
                        is_creating: false,
                        on_create: |_| {},
                        on_edit: |_| {},
                        on_delete: |_| {},
                        on_set_default: |_| {},
                        on_save: |_| {},
                        on_cancel_edit: |_| {},
                    }
                },
                SettingsTab::ApiKeys => rsx! {
                    ApiKeysSectionView {
                        discogs_configured: true,
                        discogs_key_value: String::new(),
                        is_editing: false,
                        is_saving: false,
                        has_changes: false,
                        save_error: None,
                        on_edit_start: |_| {},
                        on_key_change: |_| {},
                        on_save: |_| {},
                        on_cancel: |_| {},
                    }
                },
                SettingsTab::Encryption => rsx! {
                    EncryptionSectionView {
                        is_configured: true,
                        key_preview: "a1b2c3d4...x7y8z9".to_string(),
                        key_length: 32,
                    }
                },
                SettingsTab::BitTorrent => rsx! {
                    BitTorrentSectionView {
                        settings: BitTorrentSettings {
                            listen_port: Some(51413),
                            enable_upnp: true,
                            enable_natpmp: true,
                            max_connections: Some(200),
                            max_connections_per_torrent: Some(50),
                            max_uploads: Some(10),
                            max_uploads_per_torrent: Some(5),
                            bind_interface: None,
                        },
                        editing_section: None,
                        edit_listen_port: String::new(),
                        edit_enable_upnp: true,
                        edit_max_connections: String::new(),
                        edit_max_connections_per_torrent: String::new(),
                        edit_max_uploads: String::new(),
                        edit_max_uploads_per_torrent: String::new(),
                        edit_bind_interface: String::new(),
                        is_saving: false,
                        has_changes: false,
                        save_error: None,
                        on_edit_section: |_| {},
                        on_cancel_edit: |_| {},
                        on_save: |_| {},
                        on_listen_port_change: |_| {},
                        on_enable_upnp_change: |_| {},
                        on_max_connections_change: |_| {},
                        on_max_connections_per_torrent_change: |_| {},
                        on_max_uploads_change: |_| {},
                        on_max_uploads_per_torrent_change: |_| {},
                        on_bind_interface_change: |_| {},
                    }
                },
                SettingsTab::Subsonic => rsx! {
                    SubsonicSectionView {
                        enabled: true,
                        port: 4533,
                        is_editing: false,
                        edit_enabled: true,
                        edit_port: "4533".to_string(),
                        is_saving: false,
                        has_changes: false,
                        save_error: None,
                        on_edit_start: |_| {},
                        on_cancel: |_| {},
                        on_save: |_| {},
                        on_enabled_change: |_| {},
                        on_port_change: |_| {},
                    }
                },
                SettingsTab::About => rsx! {
                    AboutSectionView {
                        version: "0.1.0-demo".to_string(),
                        album_count: 20,
                        on_check_updates: |_| {},
                    }
                },
            }
        }
    }
}

/// Mock storage profiles for demo
fn mock_storage_profiles() -> Vec<StorageProfile> {
    vec![
        StorageProfile {
            id: "profile-1".to_string(),
            name: "Cloud Storage".to_string(),
            location: StorageLocation::Cloud,
            location_path: String::new(),
            encrypted: true,
            is_default: true,
            cloud_bucket: Some("my-music-bucket".to_string()),
            cloud_region: Some("us-east-1".to_string()),
            cloud_endpoint: None,
            cloud_access_key: Some("AKIA***".to_string()),
            cloud_secret_key: Some("***".to_string()),
        },
        StorageProfile {
            id: "profile-2".to_string(),
            name: "Local Backup".to_string(),
            location: StorageLocation::Local,
            location_path: "/Users/demo/Music/bae".to_string(),
            encrypted: false,
            is_default: false,
            cloud_bucket: None,
            cloud_region: None,
            cloud_endpoint: None,
            cloud_access_key: None,
            cloud_secret_key: None,
        },
    ]
}

/// Main demo app component
#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "min-h-screen", Router::<Route> {} }
    }
}

fn main() {
    dioxus::launch(App);
}
