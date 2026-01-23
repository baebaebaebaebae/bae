//! Storage Profiles section wrapper - handles persistence, delegates UI to StorageProfilesSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, StorageProfilesStateStoreExt};
use bae_ui::{StorageProfile, StorageProfilesSectionView};
use dioxus::prelude::*;

/// Storage Profiles section - CRUD for profiles
#[component]
pub fn StorageProfilesSection() -> Element {
    let app = use_app();

    // Pass lenses directly - don't read here!
    let store = app.state.storage_profiles();
    let profiles = store.profiles();
    let is_loading = store.loading();

    // Local UI state for editing
    let mut editing_profile = use_signal(|| Option::<StorageProfile>::None);
    let mut is_creating = use_signal(|| false);

    let display_editing = editing_profile.read().clone();

    // Handle save from the view
    let handle_save = {
        let app = app.clone();
        move |profile: StorageProfile| {
            app.save_storage_profile(profile);
            is_creating.set(false);
            editing_profile.set(None);
        }
    };

    let handle_delete = {
        let app = app.clone();
        move |profile_id: String| {
            app.delete_storage_profile(&profile_id);
        }
    };

    let handle_set_default = {
        let app = app.clone();
        move |profile_id: String| {
            app.set_default_storage_profile(&profile_id);
        }
    };

    let handle_edit = move |profile: StorageProfile| {
        editing_profile.set(Some(profile));
        is_creating.set(false);
    };

    rsx! {
        StorageProfilesSectionView {
            profiles,
            is_loading,
            editing_profile: display_editing,
            is_creating: *is_creating.read(),
            on_create: move |_| {
                is_creating.set(true);
                editing_profile.set(None);
            },
            on_edit: handle_edit,
            on_delete: handle_delete,
            on_set_default: handle_set_default,
            on_save: handle_save,
            on_cancel_edit: move |_| {
                editing_profile.set(None);
                is_creating.set(false);
            },
        }
    }
}
