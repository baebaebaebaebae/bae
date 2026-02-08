//! Settings mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{
    SettingsTab, SettingsView, StorageLocation, StorageProfile, StorageProfilesSectionView,
};
use dioxus::prelude::*;

#[component]
pub fn SettingsMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .bool_control("encryption_configured", "Encryption Configured", true)
        .bool_control("has_profiles", "Has Profiles", true)
        .bool_control("loading", "Loading", false)
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("No Encryption").set_bool("encryption_configured", false),
            Preset::new("Empty")
                .set_bool("has_profiles", false)
                .set_bool("encryption_configured", false),
            Preset::new("Loading").set_bool("loading", true),
        ])
        .build(initial_state);

    registry.use_url_sync_settings();

    let encryption_configured = registry.get_bool("encryption_configured");
    let has_profiles = registry.get_bool("has_profiles");
    let loading = registry.get_bool("loading");

    let profiles = if has_profiles {
        mock_storage_profiles()
    } else {
        vec![]
    };

    let encryption_key_fingerprint = if encryption_configured {
        "a1b2c3d4e5f6g7h8".to_string()
    } else {
        String::new()
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::Settings,
            registry,
            max_width: "full",
            SettingsView { active_tab: SettingsTab::Storage, on_tab_change: |_| {},
                StorageProfilesSectionView {
                    profiles,
                    is_loading: loading,
                    editing_profile: None,
                    is_creating: false,
                    encryption_configured,
                    encryption_key_fingerprint,
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
            }
        }
    }
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
