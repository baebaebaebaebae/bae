//! Library settings section — business logic wrapper

use bae_core::config::Config;
use bae_core::encryption::EncryptionService;
use bae_core::keys::KeyService;
use bae_core::library_dir::LibraryDir;
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::pull::pull_changes;
use bae_core::sync::s3_bucket::S3SyncBucketClient;
use bae_core::sync::snapshot::bootstrap_from_snapshot;
use bae_ui::{JoinLibraryView, JoinStatus, LibrarySectionView};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::ffi::CString;
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
    let mut showing_join = use_signal(|| false);

    // Join form state
    let mut join_bucket = use_signal(String::new);
    let mut join_region = use_signal(String::new);
    let mut join_endpoint = use_signal(String::new);
    let mut join_access_key = use_signal(String::new);
    let mut join_secret_key = use_signal(String::new);
    let mut join_status = use_signal(|| Option::<JoinStatus>::None);

    let on_switch = {
        let app = app.clone();
        move |path: String| {
            let library_path = PathBuf::from(&path);
            let target_id = match Config::read_library_id(&library_path) {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to read library ID at {path}: {e}");
                    return;
                }
            };

            let mut config = app.config.clone();
            config.library_id = target_id;
            if let Err(e) = config.save_active_library() {
                error!("Failed to save active library: {e}");
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

            if let Err(e) = config.save_active_library() {
                error!("Failed to save active library: {e}");
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

                // Read the added library's UUID and switch to it
                let target_id = match Config::read_library_id(&path) {
                    Ok(id) => id,
                    Err(e) => {
                        error!("Failed to read library ID: {e}");
                        return;
                    }
                };

                let mut config = config;
                config.library_id = target_id;
                if let Err(e) = config.save_active_library() {
                    error!("Failed to save active library: {e}");
                    return;
                }

                info!("Added and switching to existing library");
                super::super::welcome::relaunch();
            });
        }
    };

    let on_join_start = move |_| {
        // Reset join form state and show the form
        join_bucket.set(String::new());
        join_region.set(String::new());
        join_endpoint.set(String::new());
        join_access_key.set(String::new());
        join_secret_key.set(String::new());
        join_status.set(None);
        showing_join.set(true);
    };

    let on_join_cancel = move |_| {
        showing_join.set(false);
    };

    let on_join_submit = {
        let app = app.clone();
        move |_| {
            let bucket = join_bucket.read().clone();
            let region = join_region.read().clone();
            let endpoint = join_endpoint.read().clone();
            let access_key = join_access_key.read().clone();
            let secret_key = join_secret_key.read().clone();
            let key_service = app.key_service.clone();

            join_status.set(Some(JoinStatus::Joining(
                "Connecting to sync bucket...".to_string(),
            )));

            spawn(async move {
                match join_shared_library(
                    bucket,
                    region,
                    endpoint,
                    access_key,
                    secret_key,
                    key_service,
                    join_status,
                )
                .await
                {
                    Ok(config) => {
                        join_status.set(Some(JoinStatus::Success));

                        if let Err(e) = config.save_active_library() {
                            error!("Failed to save active library: {e}");
                        }

                        super::super::welcome::relaunch();
                    }
                    Err(e) => {
                        error!("Failed to join shared library: {e}");
                        join_status.set(Some(JoinStatus::Error(e)));
                    }
                }
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
            error!("Failed to remove library: {e}");
            return;
        }

        info!("Removed library {path}");
        libraries.set(discover_ui_libraries());
    };

    if *showing_join.read() {
        rsx! {
            JoinLibraryView {
                bucket: join_bucket.read().clone(),
                region: join_region.read().clone(),
                endpoint: join_endpoint.read().clone(),
                access_key: join_access_key.read().clone(),
                secret_key: join_secret_key.read().clone(),
                status: join_status.read().clone(),
                on_bucket_change: move |v| join_bucket.set(v),
                on_region_change: move |v| join_region.set(v),
                on_endpoint_change: move |v| join_endpoint.set(v),
                on_access_key_change: move |v| join_access_key.set(v),
                on_secret_key_change: move |v| join_secret_key.set(v),
                on_join: on_join_submit,
                on_cancel: on_join_cancel,
            }
        }
    } else {
        rsx! {
            LibrarySectionView {
                libraries: libraries.read().clone(),
                on_switch,
                on_create,
                on_add_existing,
                on_join: on_join_start,
                on_rename,
                on_remove,
            }
        }
    }
}

/// Perform the full join-shared-library bootstrap sequence.
///
/// Steps:
/// 1. Load the user's Ed25519 keypair (must exist -- Phase 6a).
/// 2. Create an S3SyncBucketClient with a dummy encryption key (only used
///    to access the wrapped key, which is stored as raw sealed-box bytes).
/// 3. `accept_invitation()` to unwrap the library encryption key.
/// 4. Recreate the bucket client with the real encryption key.
/// 5. Create a new library directory and config.
/// 6. `bootstrap_from_snapshot()` to get the initial database.
/// 7. Pull any changesets since the snapshot.
/// 8. Save sync bucket config and credentials.
/// 9. Return the Config so the caller can switch to it.
async fn join_shared_library(
    bucket: String,
    region: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
    key_service: KeyService,
    mut status: Signal<Option<JoinStatus>>,
) -> Result<Config, String> {
    use bae_core::sync::invite::accept_invitation;

    // Step 1: Load user keypair.
    let user_keypair = key_service
        .get_or_create_user_keypair()
        .map_err(|e| format!("Failed to load user keypair: {e}"))?;

    // Step 2: Create bucket client with a dummy encryption key.
    // We only need get_wrapped_key() which stores sealed-box bytes raw (no library-key encryption).
    let ep = if endpoint.is_empty() {
        None
    } else {
        Some(endpoint.clone())
    };
    let dummy_key = [0u8; 32];
    let dummy_encryption = EncryptionService::new(&hex::encode(dummy_key))
        .map_err(|e| format!("Failed to create encryption service: {e}"))?;

    let dummy_bucket = S3SyncBucketClient::new(
        bucket.clone(),
        region.clone(),
        ep.clone(),
        access_key.clone(),
        secret_key.clone(),
        dummy_encryption,
    )
    .await
    .map_err(|e| format!("Failed to connect to sync bucket: {e}"))?;

    // Step 3: Accept invitation to get the library encryption key.
    status.set(Some(JoinStatus::Joining(
        "Accepting invitation...".to_string(),
    )));

    let encryption_key = accept_invitation(&dummy_bucket, &user_keypair)
        .await
        .map_err(|e| format!("Failed to accept invitation: {e}"))?;

    let encryption_key_hex = hex::encode(encryption_key);

    // Step 4: Create the real bucket client with the actual encryption key.
    status.set(Some(JoinStatus::Joining(
        "Downloading library snapshot...".to_string(),
    )));

    let encryption = EncryptionService::new(&encryption_key_hex)
        .map_err(|e| format!("Invalid encryption key: {e}"))?;

    let real_bucket = S3SyncBucketClient::new(
        bucket.clone(),
        region.clone(),
        ep,
        access_key.clone(),
        secret_key.clone(),
        encryption.clone(),
    )
    .await
    .map_err(|e| format!("Failed to reconnect to sync bucket: {e}"))?;

    // Step 5: Create a new library directory.
    let home_dir = dirs::home_dir().ok_or("Failed to get home directory")?;
    let bae_dir = home_dir.join(".bae");
    let library_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    let library_dir = LibraryDir::new(bae_dir.join("libraries").join(&library_id));

    std::fs::create_dir_all(&*library_dir)
        .map_err(|e| format!("Failed to create library directory: {e}"))?;

    // All steps after directory creation are wrapped so we can clean up on failure.
    let result = bootstrap_library(
        &real_bucket,
        &encryption,
        &encryption_key_hex,
        &library_dir,
        &library_id,
        &device_id,
        &bucket,
        &region,
        &endpoint,
        &access_key,
        &secret_key,
        &key_service,
        &mut status,
    )
    .await;

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&*library_dir);
    }

    result
}

/// Inner bootstrap logic — separated so the caller can clean up the library directory on failure.
async fn bootstrap_library(
    real_bucket: &S3SyncBucketClient,
    encryption: &EncryptionService,
    encryption_key_hex: &str,
    library_dir: &LibraryDir,
    library_id: &str,
    device_id: &str,
    bucket: &str,
    region: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    key_service: &KeyService,
    status: &mut Signal<Option<JoinStatus>>,
) -> Result<Config, String> {
    // Step 6: Bootstrap from snapshot.
    let db_path = library_dir.db_path();
    let bucket_dyn: &dyn SyncBucketClient = real_bucket;
    let snapshot_seq = bootstrap_from_snapshot(bucket_dyn, encryption, &db_path)
        .await
        .map_err(|e| format!("Failed to bootstrap from snapshot: {e}"))?;

    info!("Bootstrapped from snapshot (snapshot_seq: {snapshot_seq})");

    // Step 7: Pull changesets since the snapshot.
    status.set(Some(JoinStatus::Joining(
        "Applying recent changes...".to_string(),
    )));

    // Build cursors before opening the raw connection to avoid leaking the handle.
    let heads = bucket_dyn
        .list_heads()
        .await
        .map_err(|e| format!("Failed to list heads: {e}"))?;

    let mut cursors = HashMap::new();
    for head in &heads {
        cursors.insert(head.device_id.clone(), snapshot_seq);
    }

    let changesets_applied = unsafe {
        let c_path = CString::new(db_path.to_str().unwrap()).unwrap();
        let mut db: *mut libsqlite3_sys::sqlite3 = std::ptr::null_mut();
        let rc = libsqlite3_sys::sqlite3_open(c_path.as_ptr(), &mut db);
        if rc != libsqlite3_sys::SQLITE_OK {
            return Err("Failed to open database for changeset application".to_string());
        }

        let result = match pull_changes(db, bucket_dyn, device_id, &cursors, None).await {
            Ok((_updated_cursors, pull_result)) => pull_result.changesets_applied,
            Err(e) => {
                libsqlite3_sys::sqlite3_close(db);
                return Err(format!("Failed to pull changesets: {e}"));
            }
        };

        libsqlite3_sys::sqlite3_close(db);
        result
    };

    if changesets_applied > 0 {
        info!("Applied {changesets_applied} changesets since snapshot");
    }

    // Step 8: Save encryption key to keyring and create config.
    status.set(Some(JoinStatus::Joining(
        "Saving configuration...".to_string(),
    )));

    let new_key_service = KeyService::new(key_service.is_dev_mode(), library_id.to_string());
    new_key_service
        .set_encryption_key(encryption_key_hex)
        .map_err(|e| format!("Failed to save encryption key: {e}"))?;

    // Save sync bucket credentials to keyring.
    new_key_service
        .set_sync_access_key(access_key)
        .map_err(|e| format!("Failed to save sync access key: {e}"))?;

    new_key_service
        .set_sync_secret_key(secret_key)
        .map_err(|e| format!("Failed to save sync secret key: {e}"))?;

    let config = Config {
        library_id: library_id.to_string(),
        device_id: device_id.to_string(),
        library_dir: library_dir.clone(),
        library_name: None,
        keys_migrated: true,
        discogs_key_stored: false,
        encryption_key_stored: true,
        encryption_key_fingerprint: Some(encryption.fingerprint()),
        torrent_bind_interface: None,
        torrent_listen_port: None,
        torrent_enable_upnp: true,
        torrent_enable_natpmp: true,
        torrent_enable_dht: false,
        torrent_max_connections: None,
        torrent_max_connections_per_torrent: None,
        torrent_max_uploads: None,
        torrent_max_uploads_per_torrent: None,
        network_participation: bae_core::sync::participation::ParticipationMode::Off,
        subsonic_enabled: true,
        subsonic_port: 4533,
        sync_s3_bucket: Some(bucket.to_string()),
        sync_s3_region: Some(region.to_string()),
        sync_s3_endpoint: if endpoint.is_empty() {
            None
        } else {
            Some(endpoint.to_string())
        },
    };

    config
        .save_to_config_yaml()
        .map_err(|e| format!("Failed to save config: {e}"))?;

    Config::add_known_library(library_dir)
        .map_err(|e| format!("Failed to register library: {e}"))?;

    info!("Joined shared library: {}", library_dir.display());

    Ok(config)
}
