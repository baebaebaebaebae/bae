//! Library settings section — business logic wrapper

use bae_core::cloud_home::s3::S3CloudHome;
use bae_core::cloud_home::JoinInfo;
use bae_core::config::{Config, FollowedLibrary};
use bae_core::encryption::EncryptionService;
use bae_core::join_code;
use bae_core::keys::KeyService;
use bae_core::library_dir::LibraryDir;
use bae_core::subsonic_client::SubsonicClient;
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
use bae_core::sync::pull::pull_changes;
use bae_core::sync::snapshot::bootstrap_from_snapshot;
use bae_ui::stores::config::{FollowedLibraryInfo, LibrarySource};
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt, LibraryStateStoreExt};
use bae_ui::{
    FollowLibraryView, FollowTestStatus, JoinLibraryView, JoinStatus, LibrarySectionView,
};
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

/// Which sub-view the library settings section is showing.
enum LibrarySubView {
    /// Main library list with followed servers.
    Main,
    /// Join shared library form.
    Join,
    /// Follow server form.
    Follow,
}

#[component]
pub fn LibrarySection() -> Element {
    let app = use_app();
    let mut libraries = use_signal(discover_ui_libraries);
    let mut sub_view = use_signal(|| LibrarySubView::Main);

    // Read followed libraries and active source from store
    let followed_libraries = app.state.config().followed_libraries().read().clone();
    let active_source = app.state.library().active_source().read().clone();

    // Join form state
    let mut join_invite_code = use_signal(String::new);
    let mut join_decoded_name = use_signal(|| Option::<String>::None);
    let mut join_decoded_owner = use_signal(|| Option::<String>::None);
    let mut join_decoded_cloud = use_signal(|| Option::<String>::None);
    let mut join_decode_error = use_signal(|| Option::<String>::None);
    let mut join_status = use_signal(|| Option::<JoinStatus>::None);

    // Follow form state
    let mut follow_name = use_signal(String::new);
    let mut follow_url = use_signal(String::new);
    let mut follow_username = use_signal(String::new);
    let mut follow_password = use_signal(String::new);
    let mut follow_test_status = use_signal(|| Option::<FollowTestStatus>::None);
    let mut follow_saving = use_signal(|| false);

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

            // Tell the re-exec'd process to open Settings instead of the main view
            unsafe { std::env::set_var("BAE_OPEN_SETTINGS", "1") };
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

    let on_join_start = move |_| {
        join_invite_code.set(String::new());
        join_decoded_name.set(None);
        join_decoded_owner.set(None);
        join_decoded_cloud.set(None);
        join_decode_error.set(None);
        join_status.set(None);
        sub_view.set(LibrarySubView::Join);
    };

    let on_join_cancel = move |_| {
        sub_view.set(LibrarySubView::Main);
    };

    let on_join_code_change = move |value: String| {
        join_invite_code.set(value.clone());
        if value.trim().is_empty() {
            join_decoded_name.set(None);
            join_decoded_owner.set(None);
            join_decoded_cloud.set(None);
            join_decode_error.set(None);
            return;
        }
        match join_code::decode(&value) {
            Ok(code) => {
                join_decoded_name.set(Some(code.library_name));
                join_decoded_owner.set(Some(code.owner_pubkey));
                join_decoded_cloud.set(Some(cloud_home_display(&code.join_info)));
                join_decode_error.set(None);
            }
            Err(e) => {
                join_decoded_name.set(None);
                join_decoded_owner.set(None);
                join_decoded_cloud.set(None);
                join_decode_error.set(Some(e.to_string()));
            }
        }
    };

    let on_join_submit = {
        let app = app.clone();
        move |_| {
            let code_str = join_invite_code.read().clone();
            let key_service = app.key_service.clone();

            let code = match join_code::decode(&code_str) {
                Ok(c) => c,
                Err(e) => {
                    join_status.set(Some(JoinStatus::Error(e.to_string())));
                    return;
                }
            };

            join_status.set(Some(JoinStatus::Joining(
                "Connecting to cloud home...".to_string(),
            )));

            spawn(async move {
                match join_shared_library_from_code(code, key_service, join_status).await {
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

    // Follow form callbacks
    let on_follow_start = move |_| {
        follow_name.set(String::new());
        follow_url.set(String::new());
        follow_username.set(String::new());
        follow_password.set(String::new());
        follow_test_status.set(None);
        follow_saving.set(false);
        sub_view.set(LibrarySubView::Follow);
    };

    let on_follow_cancel = move |_| {
        sub_view.set(LibrarySubView::Main);
    };

    let on_follow_test = move |_| {
        let url = follow_url.read().clone();
        let username = follow_username.read().clone();
        let password = follow_password.read().clone();
        follow_test_status.set(Some(FollowTestStatus::Testing));

        spawn(async move {
            let client = SubsonicClient::new(url, username, password);
            match client.ping().await {
                Ok(()) => follow_test_status.set(Some(FollowTestStatus::Success)),
                Err(e) => follow_test_status.set(Some(FollowTestStatus::Error(format!("{e}")))),
            }
        });
    };

    let on_follow_save = {
        let app = app.clone();
        move |_| {
            let name = follow_name.read().clone();
            let url = follow_url.read().clone();
            let username = follow_username.read().clone();
            let password = follow_password.read().clone();
            let mut config = app.config.clone();
            let key_service = app.key_service.clone();
            let state = app.state;

            follow_saving.set(true);

            let id = uuid::Uuid::new_v4().to_string();

            // Save password to keyring
            if let Err(e) = key_service.set_followed_password(&id, &password) {
                error!("Failed to save followed library password: {e}");
                follow_saving.set(false);
                return;
            }

            // Save to config
            let followed = FollowedLibrary {
                id: id.clone(),
                name: name.clone(),
                server_url: url.clone(),
                username: username.clone(),
            };

            if let Err(e) = config.add_followed_library(followed) {
                error!("Failed to save followed library config: {e}");
                follow_saving.set(false);
                return;
            }

            info!("Added followed library '{name}' at {url}");

            // Update store
            let mut followed_list = state.read().config.followed_libraries.clone();
            followed_list.push(FollowedLibraryInfo {
                id,
                name,
                server_url: url,
                username,
            });
            state.config().followed_libraries().set(followed_list);

            follow_saving.set(false);
            sub_view.set(LibrarySubView::Main);
        }
    };

    let on_unfollow = {
        let app = app.clone();
        move |id: String| {
            let mut config = app.config.clone();
            let key_service = app.key_service.clone();
            let state = app.state;

            // Delete password from keyring
            if let Err(e) = key_service.delete_followed_password(&id) {
                error!("Failed to delete followed library password: {e}");
            }

            // Remove from config
            if let Err(e) = config.remove_followed_library(&id) {
                error!("Failed to remove followed library config: {e}");
                return;
            }

            info!("Removed followed library {id}");

            // Update store
            let mut followed_list = state.read().config.followed_libraries.clone();
            followed_list.retain(|f| f.id != id);
            state.config().followed_libraries().set(followed_list);

            // If we were browsing this library, switch back to local
            if state.read().library.active_source == LibrarySource::Followed(id) {
                state.library().active_source().set(LibrarySource::Local);
                app.load_library();
            }
        }
    };

    let on_switch_source = {
        let app = app.clone();
        move |source: LibrarySource| {
            let state = app.state;
            state.library().active_source().set(source.clone());

            match source {
                LibrarySource::Local => {
                    // Reload local library data
                    app.load_library();
                }
                LibrarySource::Followed(ref id) => {
                    // Load followed library data
                    app.load_followed_library(id);
                }
            }
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
        if let Err(e) = std::fs::remove_dir_all(&library_path) {
            error!("Failed to remove library directory: {e}");
            return;
        }

        info!("Removed library {path}");
        libraries.set(discover_ui_libraries());
    };

    let is_join = matches!(&*sub_view.read(), LibrarySubView::Join);
    let is_follow = matches!(&*sub_view.read(), LibrarySubView::Follow);

    if is_join {
        rsx! {
            JoinLibraryView {
                invite_code: join_invite_code.read().clone(),
                status: join_status.read().clone(),
                decoded_library_name: join_decoded_name.read().clone(),
                decoded_owner_pubkey: join_decoded_owner.read().clone(),
                decoded_cloud_home: join_decoded_cloud.read().clone(),
                decode_error: join_decode_error.read().clone(),
                on_code_change: on_join_code_change,
                on_join: on_join_submit,
                on_cancel: on_join_cancel,
            }
        }
    } else if is_follow {
        rsx! {
            FollowLibraryView {
                name: follow_name.read().clone(),
                server_url: follow_url.read().clone(),
                username: follow_username.read().clone(),
                password: follow_password.read().clone(),
                test_status: follow_test_status.read().clone(),
                is_saving: *follow_saving.read(),
                on_name_change: move |v| follow_name.set(v),
                on_server_url_change: move |v| follow_url.set(v),
                on_username_change: move |v| follow_username.set(v),
                on_password_change: move |v| follow_password.set(v),
                on_test: on_follow_test,
                on_save: on_follow_save,
                on_cancel: on_follow_cancel,
            }
        }
    } else {
        rsx! {
            LibrarySectionView {
                libraries: libraries.read().clone(),
                followed_libraries,
                active_source,
                on_switch,
                on_create,
                on_join: on_join_start,
                on_follow: on_follow_start,
                on_unfollow,
                on_switch_source,
                on_rename,
                on_remove,
            }
        }
    }
}

/// Return a human-readable label for the cloud home type in a JoinInfo.
fn cloud_home_display(join_info: &JoinInfo) -> String {
    match join_info {
        JoinInfo::S3 { endpoint, .. } => {
            if let Some(ep) = endpoint {
                format!("S3 ({})", ep)
            } else {
                "S3".to_string()
            }
        }
        JoinInfo::GoogleDrive { .. } => "Google Drive".to_string(),
        JoinInfo::Dropbox { .. } => "Dropbox".to_string(),
        JoinInfo::OneDrive { .. } => "OneDrive".to_string(),
        JoinInfo::PCloud { .. } => "pCloud".to_string(),
    }
}

/// Perform the full join-shared-library bootstrap sequence from a decoded invite code.
async fn join_shared_library_from_code(
    code: join_code::InviteCode,
    key_service: KeyService,
    mut status: Signal<Option<JoinStatus>>,
) -> Result<Config, String> {
    use bae_core::sync::invite::accept_invitation;

    // Extract S3 credentials from the invite code.
    let (bucket, region, endpoint, access_key, secret_key) = match &code.join_info {
        JoinInfo::S3 {
            bucket,
            region,
            endpoint,
            access_key,
            secret_key,
        } => (
            bucket.clone(),
            region.clone(),
            endpoint.clone(),
            access_key.clone(),
            secret_key.clone(),
        ),
        _ => return Err("Only S3 cloud homes are supported for joining at this time".to_string()),
    };

    // Step 1: Load user keypair.
    let user_keypair = key_service
        .get_or_create_user_keypair()
        .map_err(|e| format!("Failed to load user keypair: {e}"))?;

    // Step 2: Create bucket client with a dummy encryption key.
    // We only need get_wrapped_key() which stores sealed-box bytes raw (no library-key encryption).
    let dummy_key = [0u8; 32];
    let dummy_encryption = EncryptionService::new(&hex::encode(dummy_key))
        .map_err(|e| format!("Failed to create encryption service: {e}"))?;

    let dummy_home = S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint.clone(),
        access_key.clone(),
        secret_key.clone(),
    )
    .await
    .map_err(|e| format!("Failed to connect to cloud home: {e}"))?;

    let dummy_bucket = CloudHomeSyncBucket::new(Box::new(dummy_home), dummy_encryption);

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

    let real_home = S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint.clone(),
        access_key.clone(),
        secret_key.clone(),
    )
    .await
    .map_err(|e| format!("Failed to reconnect to cloud home: {e}"))?;

    let real_bucket = CloudHomeSyncBucket::new(Box::new(real_home), encryption.clone());

    // Step 5: Create a new library directory.
    let home_dir = dirs::home_dir().ok_or("Failed to get home directory")?;
    let bae_dir = home_dir.join(".bae");
    let library_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    let library_dir = LibraryDir::new(bae_dir.join("libraries").join(&library_id));

    std::fs::create_dir_all(&*library_dir)
        .map_err(|e| format!("Failed to create library directory: {e}"))?;

    let endpoint_str = endpoint.as_deref().unwrap_or("");

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
        endpoint_str,
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
    real_bucket: &CloudHomeSyncBucket,
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
        .set_cloud_home_access_key(access_key)
        .map_err(|e| format!("Failed to save sync access key: {e}"))?;

    new_key_service
        .set_cloud_home_secret_key(secret_key)
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
        subsonic_bind_address: "127.0.0.1".to_string(),
        subsonic_auth_enabled: false,
        subsonic_username: None,
        cloud_provider: Some(bae_core::config::CloudProvider::S3),
        cloud_home_s3_bucket: Some(bucket.to_string()),
        cloud_home_s3_region: Some(region.to_string()),
        cloud_home_s3_endpoint: if endpoint.is_empty() {
            None
        } else {
            Some(endpoint.to_string())
        },
        cloud_home_google_drive_folder_id: None,
        cloud_home_dropbox_folder_path: None,
        cloud_home_onedrive_drive_id: None,
        cloud_home_onedrive_folder_id: None,
        cloud_home_pcloud_folder_id: None,
        share_base_url: None,
        share_default_expiry_days: None,
        share_signing_key_version: 1,
        followed_libraries: vec![],
    };

    config
        .save_to_config_yaml()
        .map_err(|e| format!("Failed to save config: {e}"))?;

    info!("Joined shared library: {}", library_dir.display());

    Ok(config)
}
