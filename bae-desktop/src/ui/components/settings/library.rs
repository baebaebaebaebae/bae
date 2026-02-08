//! Library settings section — business logic wrapper

use bae_core::config::Config;
use bae_ui::LibrarySectionView;
use dioxus::prelude::*;
use std::path::PathBuf;
use tracing::{error, info};

use crate::ui::app_service::use_app;

/// Convert bae-core LibraryInfo to bae-ui LibraryInfo (PathBuf -> String)
fn discover_ui_libraries() -> Vec<bae_ui::LibraryInfo> {
    Config::discover_libraries()
        .into_iter()
        .map(|lib| bae_ui::LibraryInfo {
            id: lib.id,
            name: lib.name,
            path: lib.path.to_string_lossy().to_string(),
            is_active: lib.is_active,
        })
        .collect()
}

#[component]
pub fn LibrarySection() -> Element {
    let app = use_app();
    let mut libraries = use_signal(discover_ui_libraries);

    let on_switch = {
        let app = app.clone();
        move |path: String| {
            let library_path = PathBuf::from(&path);
            let mut config = app.config.clone();
            config.library_path = library_path;
            if let Err(e) = config.save_library_path() {
                error!("Failed to save library path: {e}");
                return;
            }

            info!("Switching to library at {path}");
            super::super::welcome::relaunch();
        }
    };

    let on_create = {
        let app = app.clone();
        move |_| {
            let dev_mode = app.key_service.is_dev_mode();
            let config = match Config::create_new_library(dev_mode) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to create new library: {e}");
                    return;
                }
            };

            if let Err(e) = config.save_library_path() {
                error!("Failed to save library pointer: {e}");
                return;
            }

            super::super::welcome::relaunch();
        }
    };

    let on_add_existing = {
        let app = app.clone();
        move |_| {
            let config = app.config.clone();
            spawn(async move {
                let picked = rfd::AsyncFileDialog::new()
                    .set_title("Choose a folder containing a bae library")
                    .pick_folder()
                    .await;

                let folder = match picked {
                    Some(f) => f,
                    None => return,
                };

                let path = PathBuf::from(folder.path());
                let config_path = path.join("config.yaml");

                if !config_path.exists() {
                    error!("Selected folder has no config.yaml: {}", path.display());
                    return;
                }

                if let Err(e) = Config::add_known_library(&path) {
                    error!("Failed to register library: {e}");
                    return;
                }

                // Switch to the added library
                let mut config = config;
                config.library_path = path;
                if let Err(e) = config.save_library_path() {
                    error!("Failed to save library path: {e}");
                    return;
                }

                info!("Added and switching to existing library");
                super::super::welcome::relaunch();
            });
        }
    };

    let on_rename = move |(path, new_name): (String, String)| {
        let library_path = PathBuf::from(&path);
        if let Err(e) = Config::rename_library(&library_path, &new_name) {
            error!("Failed to rename library: {e}");
            return;
        }

        info!("Renamed library at {path} to '{new_name}'");
        libraries.set(discover_ui_libraries());
    };

    let on_remove = move |path: String| {
        let library_path = PathBuf::from(&path);
        if let Err(e) = Config::remove_known_library(&library_path) {
            error!("Failed to remove library from known list: {e}");
            return;
        }

        info!("Removed library {path} from known list");
        libraries.set(discover_ui_libraries());
    };

    rsx! {
        LibrarySectionView {
            libraries: libraries.read().clone(),
            on_switch,
            on_create,
            on_add_existing,
            on_rename,
            on_remove,
        }
    }
}
