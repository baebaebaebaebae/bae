//! Settings mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel};
use bae_ui::stores::DeviceActivityInfo;
use bae_ui::{
    AboutSectionView, BitTorrentSectionView, BitTorrentSettings, DiscogsSectionView, LibraryInfo,
    LibrarySectionView, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfilesSectionView, SubsonicSectionView, SyncSectionView,
};
use dioxus::prelude::*;

#[component]
pub fn SettingsMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new().build(initial_state);
    let mut active_tab = use_signal(|| SettingsTab::Library);

    // Storage section state
    let mut editing_profile = use_signal(|| Option::<StorageProfile>::None);
    let mut is_creating = use_signal(|| false);
    let browsed_directory = use_signal(|| Option::<String>::None);
    let display_editing = editing_profile.read().clone();

    // Discogs state
    let mut discogs_editing = use_signal(|| false);
    let mut discogs_key = use_signal(String::new);

    // Subsonic state
    let mut subsonic_editing = use_signal(|| false);
    let mut subsonic_edit_enabled = use_signal(|| true);
    let mut subsonic_edit_port = use_signal(|| "4533".to_string());

    rsx! {
        MockPanel {
            current_mock: MockPage::Settings,
            registry,
            max_width: "full",
            SettingsView {
                active_tab: *active_tab.read(),
                on_tab_change: move |tab| active_tab.set(tab),

                match *active_tab.read() {
                    SettingsTab::Library => rsx! {
                        LibrarySectionView {
                            libraries: mock_libraries(),
                            on_switch: |_| {},
                            on_create: |_| {},
                            on_add_existing: |_| {},
                            on_rename: |_| {},
                            on_remove: |_| {},
                        }
                    },
                    SettingsTab::Storage => rsx! {
                        StorageProfilesSectionView {
                            profiles: mock_storage_profiles(),
                            is_loading: false,
                            editing_profile: display_editing,
                            is_creating: *is_creating.read(),
                            delete_error: None,
                            encryption_configured: true,
                            encryption_key_fingerprint: "a1b2c3d4e5f6g7h8".to_string(),
                            on_copy_key: |_| {},
                            on_import_key: |_| {},
                            on_create: move |_| {
                                is_creating.set(true);
                                editing_profile.set(None);
                            },
                            on_edit: move |profile: StorageProfile| {
                                editing_profile.set(Some(profile));
                                is_creating.set(false);
                            },
                            on_delete: |_| {},
                            on_set_default: |_| {},
                            on_save: move |_: StorageProfile| {
                                is_creating.set(false);
                                editing_profile.set(None);
                            },
                            on_cancel_edit: move |_| {
                                is_creating.set(false);
                                editing_profile.set(None);
                            },
                            on_browse_directory: |_| {},
                            browsed_directory,
                        }
                    },
                    SettingsTab::Sync => rsx! {
                        SyncSectionView {
                            last_sync_time: Some("2026-02-10T12:00:00Z".to_string()),
                            other_devices: vec![
                                DeviceActivityInfo {
                                    device_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                                    last_seq: 42,
                                    last_sync: Some("2026-02-10T11:55:00Z".to_string()),
                                },
                            ],
                            syncing: false,
                            error: None,
                        }
                    },
                    SettingsTab::Discogs => rsx! {
                        DiscogsSectionView {
                            discogs_configured: true,
                            discogs_key_value: discogs_key(),
                            is_editing: *discogs_editing.read(),
                            is_saving: false,
                            has_changes: !discogs_key.read().is_empty(),
                            save_error: None,
                            on_edit_start: move |_| discogs_editing.set(true),
                            on_key_change: move |v| discogs_key.set(v),
                            on_save: move |_| discogs_editing.set(false),
                            on_cancel: move |_| {
                                discogs_editing.set(false);
                                discogs_key.set(String::new());
                            },
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
                            is_editing: *subsonic_editing.read(),
                            edit_enabled: *subsonic_edit_enabled.read(),
                            edit_port: subsonic_edit_port(),
                            is_saving: false,
                            has_changes: false,
                            save_error: None,
                            on_edit_start: move |_| subsonic_editing.set(true),
                            on_cancel: move |_| subsonic_editing.set(false),
                            on_save: move |_| subsonic_editing.set(false),
                            on_enabled_change: move |v| subsonic_edit_enabled.set(v),
                            on_port_change: move |v| subsonic_edit_port.set(v),
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
}

fn mock_libraries() -> Vec<LibraryInfo> {
    vec![
        LibraryInfo {
            id: "abc-123".to_string(),
            name: Some("My Music".to_string()),
            path: "/Users/demo/.bae/libraries/abc-123".to_string(),
            is_active: true,
        },
        LibraryInfo {
            id: "def-456".to_string(),
            name: Some("Jazz Collection".to_string()),
            path: "/Users/demo/.bae/libraries/def-456".to_string(),
            is_active: false,
        },
    ]
}

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
