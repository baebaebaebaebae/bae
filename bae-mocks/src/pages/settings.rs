//! Settings page

use bae_ui::stores::DeviceActivityInfo;
use bae_ui::{
    AboutSectionView, BitTorrentSectionView, BitTorrentSettings, DiscogsSectionView, LibraryInfo,
    LibrarySectionView, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfilesSectionView, SubsonicSectionView, SyncSectionView,
};
use dioxus::prelude::*;

#[component]
pub fn Settings() -> Element {
    let mut active_tab = use_signal(|| SettingsTab::Library);

    rsx! {
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
                        editing_profile: None,
                        is_creating: false,
                        delete_error: None,
                        encryption_configured: true,
                        encryption_key_fingerprint: "a1b2c3d4e5f6g7h8".to_string(),
                        on_copy_key: |_| {},
                        on_import_key: |_| {},
                        on_create: |_| {},
                        on_edit: |_| {},
                        on_delete: |_| {},
                        on_set_default: |_| {},
                        on_save: |_| {},
                        on_cancel_edit: |_| {},
                        on_browse_directory: |_| {},
                        browsed_directory: None,
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
                        user_pubkey: Some("a1b2c3d4e5f67890abcdef1234567890a1b2c3d4e5f67890abcdef1234567890".to_string()),
                        on_copy_pubkey: |_| {},
                        sync_bucket: Some("my-sync-bucket".to_string()),
                        sync_region: Some("us-east-1".to_string()),
                        sync_endpoint: None,
                        sync_configured: true,
                        is_editing: false,
                        edit_bucket: String::new(),
                        edit_region: String::new(),
                        edit_endpoint: String::new(),
                        edit_access_key: String::new(),
                        edit_secret_key: String::new(),
                        is_saving: false,
                        save_error: None,
                        is_testing: false,
                        test_success: None,
                        test_error: None,
                        on_edit_start: |_| {},
                        on_cancel_edit: |_| {},
                        on_save_config: |_| {},
                        on_test_connection: |_| {},
                        on_bucket_change: |_| {},
                        on_region_change: |_| {},
                        on_endpoint_change: |_| {},
                        on_access_key_change: |_| {},
                        on_secret_key_change: |_| {},
                    }
                },
                SettingsTab::Discogs => rsx! {
                    DiscogsSectionView {
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
        LibraryInfo {
            id: "ghi-789".to_string(),
            name: None,
            path: "/Volumes/External/bae-library".to_string(),
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
