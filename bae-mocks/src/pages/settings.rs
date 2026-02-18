//! Settings page

use bae_ui::stores::config::{CloudProvider, FollowedLibraryInfo, LibrarySource};
use bae_ui::stores::{DeviceActivityInfo, Member, MemberRole, SharedReleaseDisplay};
use bae_ui::{
    AboutSectionView, BaeCloudAuthMode, BitTorrentSectionView, BitTorrentSettings,
    CloudProviderOption, DiscogsSectionView, LibraryInfo, LibrarySectionView, SettingsTab,
    SettingsView, SubsonicSectionView, SyncSectionView,
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
                        followed_libraries: mock_followed_libraries(),
                        active_source: LibrarySource::Local,
                        on_switch: |_| {},
                        on_create: |_| {},
                        on_join: |_| {},
                        on_follow: |_| {},
                        on_unfollow: |_| {},
                        on_copy_follow_code: |_| {},
                        on_switch_source: |_| {},
                        on_rename: |_| {},
                        on_remove: |_| {},
                        show_link_device_button: false,
                        on_link_device: |_| {},
                        device_link_qr_svg: None,
                        on_close_device_link: |_| {},
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
                        // bae cloud form state
                        bae_cloud_is_editing: false,
                        bae_cloud_mode: BaeCloudAuthMode::SignUp,
                        bae_cloud_email: String::new(),
                        bae_cloud_username: String::new(),
                        bae_cloud_password: String::new(),
                        on_bae_cloud_mode_change: |_| {},
                        on_bae_cloud_email_change: |_| {},
                        on_bae_cloud_username_change: |_| {},
                        on_bae_cloud_password_change: |_| {},
                        on_bae_cloud_submit: |_| {},
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
                        // Recovery key
                        recovery_key: None,
                        on_reveal_recovery_key: |_| {},
                        on_copy_recovery_key: |_| {},
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
                        auth_enabled: false,
                        auth_username: None,
                        auth_password_set: false,
                        is_editing: false,
                        edit_enabled: true,
                        edit_port: "4533".to_string(),
                        edit_auth_enabled: false,
                        edit_username: String::new(),
                        edit_password: String::new(),
                        edit_password_confirm: String::new(),
                        is_saving: false,
                        has_changes: false,
                        save_error: None,
                        on_edit_start: |_| {},
                        on_cancel: |_| {},
                        on_save: |_| {},
                        on_enabled_change: |_| {},
                        on_port_change: |_| {},
                        share_base_url: "https://listen.example.com".to_string(),
                        is_editing_share: false,
                        edit_share_base_url: String::new(),
                        is_saving_share: false,
                        has_share_changes: false,
                        share_save_error: None,
                        on_share_edit_start: |_| {},
                        on_share_cancel: |_| {},
                        on_share_save: |_| {},
                        on_share_base_url_change: |_| {},
                        on_auth_enabled_change: |_| {},
                        on_username_change: |_| {},
                        on_password_change: |_| {},
                        on_password_confirm_change: |_| {},
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
            path: "/Users/demo/.bae/libraries/ghi-789".to_string(),
            is_active: false,
        },
    ]
}

fn mock_followed_libraries() -> Vec<FollowedLibraryInfo> {
    vec![FollowedLibraryInfo {
        id: "follow-1".to_string(),
        name: "Friend's Library".to_string(),
        proxy_url: "https://alice.bae.fm".to_string(),
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
    vec![SharedReleaseDisplay {
        grant_id: "grant-001".to_string(),
        release_id: "rel-abc123".to_string(),
        from_library_id: "lib-xyz789".to_string(),
        from_user_pubkey: "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddee"
            .to_string(),
        bucket: "shared-music-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint: None,
        expires: Some("2026-06-01T00:00:00Z".to_string()),
    }]
}

fn mock_cloud_options() -> Vec<CloudProviderOption> {
    vec![
        CloudProviderOption {
            provider: CloudProvider::BaeCloud,
            label: "bae cloud",
            description: "Encrypted cloud sync",
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::ICloud,
            label: "iCloud Drive",
            description: "Automatic sync, no setup needed",
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::GoogleDrive,
            label: "Google Drive",
            description: "Sign in to sync via Google Drive",
            connected_account: Some("user@gmail.com".to_string()),
        },
        CloudProviderOption {
            provider: CloudProvider::Dropbox,
            label: "Dropbox",
            description: "Sign in to sync via Dropbox",
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::OneDrive,
            label: "OneDrive",
            description: "Sign in to sync via OneDrive",
            connected_account: None,
        },
        CloudProviderOption {
            provider: CloudProvider::S3,
            label: "S3-compatible",
            description: "For Backblaze B2, Wasabi, MinIO, AWS, etc.",
            connected_account: None,
        },
    ]
}
