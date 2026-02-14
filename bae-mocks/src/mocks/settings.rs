//! Settings mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel};
use bae_ui::stores::config::{CloudProvider, FollowedLibraryInfo, LibrarySource};
use bae_ui::stores::{DeviceActivityInfo, Member, MemberRole, SharedReleaseDisplay};
use bae_ui::{
    AboutSectionView, BitTorrentSectionView, BitTorrentSettings, CloudProviderOption,
    DiscogsSectionView, LibraryInfo, LibrarySectionView, SettingsTab, SettingsView,
    StorageLocation, StorageProfile, StorageProfilesSectionView, SubsonicSectionView,
    SyncSectionView,
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
    let mut subsonic_edit_auth_enabled = use_signal(|| false);
    let mut subsonic_edit_username = use_signal(String::new);
    let mut subsonic_edit_password = use_signal(String::new);
    let mut subsonic_edit_password_confirm = use_signal(String::new);

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
                            followed_libraries: mock_followed_libraries(),
                            active_source: LibrarySource::Local,
                            on_switch: |_| {},
                            on_create: |_| {},
                            on_join: |_| {},
                            on_follow: |_| {},
                            on_unfollow: |_| {},
                            on_switch_source: |_| {},
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
                            user_pubkey: Some("a1b2c3d4e5f67890abcdef1234567890a1b2c3d4e5f67890abcdef1234567890".to_string()),
                            on_copy_pubkey: |_| {},
                            members: mock_members(),
                            is_owner: true,
                            on_remove_member: |_| {},
                            is_removing_member: false,
                            removing_member_error: None,
                            on_sync_now: |_| {},
                            cloud_home_configured: true,
                            // Cloud provider picker
                            cloud_provider: Some(CloudProvider::GoogleDrive),
                            cloud_options: mock_cloud_options(),
                            signing_in: false,
                            sign_in_error: None,
                            on_select_provider: |_| {},
                            on_sign_in: |_| {},
                            on_disconnect_provider: |_| {},
                            on_use_icloud: |_| {},
                            // S3 edit state
                            is_editing: false,
                            edit_bucket: String::new(),
                            edit_region: String::new(),
                            edit_endpoint: String::new(),
                            edit_access_key: String::new(),
                            edit_secret_key: String::new(),
                            on_edit_start: |_| {},
                            on_cancel_edit: |_| {},
                            on_save_config: |_| {},
                            on_bucket_change: |_| {},
                            on_region_change: |_| {},
                            on_endpoint_change: |_| {},
                            on_access_key_change: |_| {},
                            on_secret_key_change: |_| {},
                            // Invite
                            show_invite_form: false,
                            invite_pubkey: String::new(),
                            invite_role: MemberRole::Member,
                            invite_status: None,
                            share_info: None,
                            on_toggle_invite_form: |_| {},
                            on_invite_pubkey_change: |_| {},
                            on_invite_role_change: |_| {},
                            on_invite_member: |_| {},
                            on_copy_share_info: |_| {},
                            on_dismiss_share_info: |_| {},
                            shared_releases: mock_shared_releases(),
                            accept_grant_text: String::new(),
                            is_accepting_grant: false,
                            accept_grant_error: None,
                            on_accept_grant_text_change: |_| {},
                            on_accept_grant: |_| {},
                            on_revoke_shared_release: |_| {},
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
                            auth_enabled: false,
                            auth_username: None,
                            auth_password_set: false,
                            is_editing: *subsonic_editing.read(),
                            edit_enabled: *subsonic_edit_enabled.read(),
                            edit_port: subsonic_edit_port(),
                            edit_auth_enabled: *subsonic_edit_auth_enabled.read(),
                            edit_username: subsonic_edit_username(),
                            edit_password: subsonic_edit_password(),
                            edit_password_confirm: subsonic_edit_password_confirm(),
                            is_saving: false,
                            has_changes: false,
                            save_error: None,
                            on_edit_start: move |_| subsonic_editing.set(true),
                            on_cancel: move |_| subsonic_editing.set(false),
                            on_save: move |_| subsonic_editing.set(false),
                            on_enabled_change: move |v| subsonic_edit_enabled.set(v),
                            on_port_change: move |v| subsonic_edit_port.set(v),
                            share_base_url: "https://listen.example.com".to_string(),
                            share_default_expiry: "never".to_string(),
                            share_signing_key_version: 1,
                            is_editing_share: false,
                            edit_share_base_url: String::new(),
                            edit_share_expiry: "never".to_string(),
                            is_saving_share: false,
                            has_share_changes: false,
                            share_save_error: None,
                            on_share_edit_start: |_| {},
                            on_share_cancel: |_| {},
                            on_share_save: |_| {},
                            on_share_base_url_change: |_| {},
                            on_share_expiry_change: |_| {},
                            on_invalidate_links: |_| {},
                            on_auth_enabled_change: move |v| subsonic_edit_auth_enabled.set(v),
                            on_username_change: move |v| subsonic_edit_username.set(v),
                            on_password_change: move |v| subsonic_edit_password.set(v),
                            on_password_confirm_change: move |v| subsonic_edit_password_confirm.set(v),
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

fn mock_followed_libraries() -> Vec<FollowedLibraryInfo> {
    vec![FollowedLibraryInfo {
        id: "follow-1".to_string(),
        name: "Friend's Library".to_string(),
        server_url: "http://192.168.1.50:4533".to_string(),
        username: "listener".to_string(),
    }]
}

fn mock_members() -> Vec<Member> {
    vec![
        Member {
            pubkey: "a1b2c3d4e5f67890abcdef1234567890a1b2c3d4e5f67890abcdef1234567890".to_string(),
            display_name: "a1b2...7890".to_string(),
            role: MemberRole::Owner,
            is_self: true,
        },
        Member {
            pubkey: "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddee".to_string(),
            display_name: "ff00...ddee".to_string(),
            role: MemberRole::Member,
            is_self: false,
        },
    ]
}

fn mock_shared_releases() -> Vec<SharedReleaseDisplay> {
    vec![
        SharedReleaseDisplay {
            grant_id: "grant-001".to_string(),
            release_id: "rel-abc123".to_string(),
            from_library_id: "lib-xyz789".to_string(),
            from_user_pubkey: "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddee"
                .to_string(),
            bucket: "shared-music-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            expires: Some("2026-06-01T00:00:00Z".to_string()),
        },
        SharedReleaseDisplay {
            grant_id: "grant-002".to_string(),
            release_id: "rel-def456".to_string(),
            from_library_id: "lib-uvw321".to_string(),
            from_user_pubkey: "aabbccddeeff00112233445566778899aabbccddeeff0011223344556677889900"
                .to_string(),
            bucket: "another-bucket".to_string(),
            region: "eu-west-1".to_string(),
            endpoint: Some("https://s3.example.com".to_string()),
            expires: None,
        },
    ]
}

fn mock_cloud_options() -> Vec<CloudProviderOption> {
    vec![
        CloudProviderOption {
            provider: CloudProvider::ICloud,
            label: "iCloud Drive",
            description: "Automatic sync, no setup needed",
            available: cfg!(target_os = "macos"),
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::GoogleDrive,
            label: "Google Drive",
            description: "Sign in to sync via Google Drive",
            available: true,
            connected_account: Some("user@gmail.com".to_string()),
        },
        CloudProviderOption {
            provider: CloudProvider::Dropbox,
            label: "Dropbox",
            description: "Sign in to sync via Dropbox",
            available: true,
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::OneDrive,
            label: "OneDrive",
            description: "Sign in to sync via OneDrive",
            available: true,
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::PCloud,
            label: "pCloud",
            description: "Sign in to sync via pCloud",
            available: true,
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::S3,
            label: "S3-compatible",
            description: "For Backblaze B2, Wasabi, MinIO, AWS, etc.",
            available: true,
            connected_account: None,
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
