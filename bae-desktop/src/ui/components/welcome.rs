//! Welcome screen for first-run setup
//!
//! Shown when no `~/.bae/active-library` pointer file exists. Offers two choices:
//! - Create a new library (writes pointer file with UUID, re-execs binary)
//! - Restore from cloud (downloads encrypted DB + covers, then re-execs)

use bae_core::keys::KeyService;
use bae_ui::components::button::{Button, ButtonSize, ButtonVariant};
use bae_ui::components::text_input::{TextInput, TextInputSize, TextInputType};
use dioxus::prelude::*;
use tracing::{error, info};

use crate::ui::app::MAIN_CSS;
use crate::ui::app::TAILWIND_CSS;

#[derive(Clone)]
struct WelcomeContext {
    dev_mode: bool,
}

/// Launch a minimal Dioxus app with just the welcome screen
pub fn launch_welcome(dev_mode: bool) {
    let config = dioxus::desktop::Config::default()
        .with_window(
            dioxus::desktop::WindowBuilder::new()
                .with_title("bae")
                .with_inner_size(dioxus::desktop::LogicalSize::new(600, 700))
                .with_resizable(false)
                .with_decorations(true)
                .with_transparent(true)
                .with_background_color((0x0f, 0x11, 0x16, 0xff)),
        )
        .with_background_color((0x0f, 0x11, 0x16, 0xff));

    LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context_provider(move || Box::new(WelcomeContext { dev_mode }))
        .launch(WelcomeApp);
}

#[component]
fn WelcomeApp() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        WelcomeScreen {}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum WelcomeMode {
    Choose,
    Restore,
}

#[derive(Clone, PartialEq)]
enum RestoreStatus {
    Idle,
    Restoring,
    Error(String),
}

#[component]
fn WelcomeScreen() -> Element {
    let mut mode = use_signal(|| WelcomeMode::Choose);
    let mut restore_status = use_signal(|| RestoreStatus::Idle);

    // Restore form fields
    let mut library_id = use_signal(String::new);
    let mut bucket = use_signal(String::new);
    let mut region = use_signal(String::new);
    let mut endpoint = use_signal(String::new);
    let mut access_key = use_signal(String::new);
    let mut secret_key = use_signal(String::new);
    let mut encryption_key = use_signal(String::new);

    let on_create_new = move |_| {
        let ctx = use_context::<WelcomeContext>();
        let config = bae_core::config::Config::create_new_library(ctx.dev_mode)
            .expect("Failed to create new library");

        config
            .save_active_library()
            .expect("Failed to write library pointer");

        relaunch();
    };

    let on_restore = {
        move |_| {
            let lid = library_id.read().clone();
            let b = bucket.read().clone();
            let r = region.read().clone();
            let ep = endpoint.read().clone();
            let ak = access_key.read().clone();
            let sk = secret_key.read().clone();
            let ek = encryption_key.read().clone();

            if lid.is_empty()
                || b.is_empty()
                || r.is_empty()
                || ak.is_empty()
                || sk.is_empty()
                || ek.is_empty()
            {
                restore_status.set(RestoreStatus::Error(
                    "All fields except endpoint are required".into(),
                ));
                return;
            }

            restore_status.set(RestoreStatus::Restoring);
            let ctx = use_context::<WelcomeContext>();

            spawn(async move {
                let key_service = KeyService::new(ctx.dev_mode, lid.clone());
                match do_restore(&key_service, lid, b, r, ep, ak, sk, ek).await {
                    Ok(()) => {
                        info!("Cloud restore complete, re-launching");
                        relaunch();
                    }
                    Err(e) => {
                        error!("Cloud restore failed: {}", e);
                        restore_status.set(RestoreStatus::Error(e.to_string()));
                    }
                }
            });
        }
    };

    rsx! {
        div { class: "flex flex-col items-center justify-center min-h-screen bg-gray-900 p-8",
            div { class: "max-w-lg w-full",
                h1 { class: "text-3xl font-bold text-white text-center mb-2", "bae" }
                p { class: "text-gray-400 text-center mb-8", "Music library manager" }

                match *mode.read() {
                    WelcomeMode::Choose => rsx! {
                        div { class: "space-y-4",
                            button {
                                class: "w-full p-6 bg-gray-800 hover:bg-gray-700 rounded-lg text-left transition-colors",
                                onclick: on_create_new,
                                h3 { class: "text-lg font-medium text-white mb-1", "Create new library" }
                                p { class: "text-sm text-gray-400", "Start fresh with an empty music library" }
                            }
                            button {
                                class: "w-full p-6 bg-gray-800 hover:bg-gray-700 rounded-lg text-left transition-colors",
                                onclick: move |_| mode.set(WelcomeMode::Restore),
                                h3 { class: "text-lg font-medium text-white mb-1", "Restore from cloud" }
                                p { class: "text-sm text-gray-400", "Download your library from S3 cloud backup" }
                            }
                        }
                    },
                    WelcomeMode::Restore => rsx! {
                        div { class: "space-y-4",
                            h2 { class: "text-xl font-semibold text-white", "Restore from Cloud" }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Library ID" }
                                TextInput {
                                    value: library_id.read().clone(),
                                    on_input: move |v| library_id.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                    placeholder: "UUID from your other device",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "S3 Bucket" }
                                TextInput {
                                    value: bucket.read().clone(),
                                    on_input: move |v| bucket.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                    placeholder: "my-bae-backup",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Region" }
                                TextInput {
                                    value: region.read().clone(),
                                    on_input: move |v| region.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                    placeholder: "us-east-1",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Endpoint (optional)" }
                                TextInput {
                                    value: endpoint.read().clone(),
                                    on_input: move |v| endpoint.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Text,
                                    placeholder: "https://s3.example.com",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Access Key" }
                                TextInput {
                                    value: access_key.read().clone(),
                                    on_input: move |v| access_key.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Password,
                                    placeholder: "Access key ID",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Secret Key" }
                                TextInput {
                                    value: secret_key.read().clone(),
                                    on_input: move |v| secret_key.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Password,
                                    placeholder: "Secret access key",
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-1", "Encryption Key" }
                                TextInput {
                                    value: encryption_key.read().clone(),
                                    on_input: move |v| encryption_key.set(v),
                                    size: TextInputSize::Medium,
                                    input_type: TextInputType::Password,
                                    placeholder: "Hex-encoded encryption key",
                                }
                            }
                            match restore_status.read().clone() {
                                RestoreStatus::Idle => rsx! {},
                                RestoreStatus::Restoring => rsx! {
                                    div { class: "p-3 bg-indigo-900/30 border border-indigo-700 rounded-lg text-sm text-indigo-300",
                                        "Downloading and decrypting your library..."
                                    }
                                },
                                RestoreStatus::Error(msg) => rsx! {
                                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                        "{msg}"
                                    }
                                },
                            }
                            div { class: "flex gap-3 pt-2",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Medium,
                                    disabled: *restore_status.read() == RestoreStatus::Restoring,
                                    loading: *restore_status.read() == RestoreStatus::Restoring,
                                    onclick: on_restore,
                                    "Restore"
                                }
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Medium,
                                    disabled: *restore_status.read() == RestoreStatus::Restoring,
                                    onclick: move |_| mode.set(WelcomeMode::Choose),
                                    "Back"
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Perform the full cloud restore: download DB + images, create home profile, write config + keyring + pointer file
async fn do_restore(
    key_service: &KeyService,
    library_id: String,
    bucket: String,
    region: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
    encryption_key_hex: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use bae_core::cloud_storage::{CloudStorage, S3CloudStorage, S3Config};
    use bae_core::encryption::EncryptionService;

    let encryption_service = EncryptionService::new(&encryption_key_hex)?;
    let fingerprint = encryption_service.fingerprint();

    let endpoint_opt = if endpoint.is_empty() {
        None
    } else {
        Some(endpoint.clone())
    };

    let s3_config = S3Config {
        bucket_name: bucket.clone(),
        region: region.clone(),
        access_key_id: access_key.clone(),
        secret_access_key: secret_key.clone(),
        endpoint_url: endpoint_opt.clone(),
    };
    let storage = S3CloudStorage::new_with_bucket_creation(s3_config, false).await?;

    // Download and decrypt manifest to validate key
    info!("Downloading manifest from cloud...");
    let encrypted_manifest = storage
        .download(&format!("s3://{}/manifest.json.enc", bucket))
        .await?;
    let manifest_bytes = encryption_service.decrypt(&encrypted_manifest)?;
    let manifest: bae_core::library_dir::Manifest = serde_json::from_slice(&manifest_bytes)?;

    // Validate fingerprint
    if let Some(ref expected_fp) = manifest.encryption_key_fingerprint {
        if *expected_fp != fingerprint {
            return Err(format!(
                "Encryption key fingerprint mismatch: expected {}, got {}",
                expected_fp, fingerprint
            )
            .into());
        }
    }

    info!("Key validated, downloading library...");

    // Set up local library directory
    let home_dir = dirs::home_dir().expect("Failed to get home directory");
    let bae_dir = home_dir.join(".bae");
    let library_dir =
        bae_core::library_dir::LibraryDir::new(bae_dir.join("libraries").join(&library_id));
    std::fs::create_dir_all(&*library_dir)?;

    // Download and decrypt DB
    let encrypted_db = storage
        .download(&format!("s3://{}/library.db.enc", bucket))
        .await?;
    let decrypted_db = encryption_service.decrypt(&encrypted_db)?;
    let db_path = library_dir.db_path();
    tokio::fs::write(&db_path, &decrypted_db).await?;

    info!("Restored DB ({} bytes)", decrypted_db.len());

    // Download and decrypt images
    let images_dir = library_dir.images_dir();
    download_images_encrypted(&storage, &encryption_service, &bucket, &images_dir).await?;

    // Write local manifest.json
    let home_manifest = bae_core::library_dir::Manifest {
        library_id: library_id.clone(),
        library_name: manifest.library_name.clone(),
        encryption_key_fingerprint: Some(fingerprint.clone()),
    };
    let manifest_json = serde_json::to_string_pretty(&home_manifest)?;
    tokio::fs::write(library_dir.manifest_path(), manifest_json).await?;

    // Write config.yaml
    let config = bae_core::config::Config {
        library_id: library_id.clone(),
        device_id: uuid::Uuid::new_v4().to_string(),
        library_dir: library_dir.clone(),
        library_name: manifest.library_name,
        keys_migrated: true,
        discogs_key_stored: false,
        encryption_key_stored: true,
        encryption_key_fingerprint: Some(fingerprint),
        torrent_bind_interface: None,
        torrent_listen_port: None,
        torrent_enable_upnp: false,
        torrent_enable_natpmp: false,
        torrent_enable_dht: false,
        torrent_max_connections: None,
        torrent_max_connections_per_torrent: None,
        torrent_max_uploads: None,
        torrent_max_uploads_per_torrent: None,
        network_participation: bae_core::sync::participation::ParticipationMode::Off,
        server_enabled: false,
        server_port: 4533,
        server_bind_address: "127.0.0.1".to_string(),
        server_auth_enabled: false,
        server_username: None,
        cloud_provider: None,
        cloud_home_s3_bucket: None,
        cloud_home_s3_region: None,
        cloud_home_s3_endpoint: None,
        cloud_home_s3_key_prefix: None,
        cloud_home_google_drive_folder_id: None,
        cloud_home_dropbox_folder_path: None,
        cloud_home_onedrive_drive_id: None,
        cloud_home_onedrive_folder_id: None,
        cloud_home_icloud_container_path: None,
        cloud_home_bae_cloud_url: None,
        cloud_home_bae_cloud_username: None,
        share_base_url: None,
        followed_libraries: vec![],
    };
    config.save_to_config_yaml()?;

    // Write secrets to keyring
    key_service.set_encryption_key(&encryption_key_hex)?;

    // Save cloud home S3 credentials
    {
        let creds = bae_core::keys::CloudHomeCredentials::S3 {
            access_key: access_key.clone(),
            secret_key: secret_key.clone(),
        };
        key_service.set_cloud_home_credentials(&creds)?;

        info!("Saved cloud home S3 credentials");
    }

    // Write pointer file last (makes this idempotent on failure)
    config.save_active_library()?;

    info!(
        "Cloud restore complete: library at {}",
        library_dir.display()
    );
    Ok(())
}

/// Download and decrypt all images from the cloud bucket.
async fn download_images_encrypted(
    storage: &bae_core::cloud_storage::S3CloudStorage,
    encryption: &bae_core::encryption::EncryptionService,
    bucket: &str,
    images_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use bae_core::cloud_storage::CloudStorage;

    tokio::fs::create_dir_all(images_dir).await?;

    let keys = storage.list_keys("images/").await?;

    info!("Found {} image(s) to download", keys.len());

    for key in &keys {
        // key looks like "images/ab/cd/<id>" — strip "images/" prefix to get the relative path
        let rel = key.strip_prefix("images/").unwrap_or(key);
        let target_path = images_dir.join(rel);

        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let location = format!("s3://{}/{}", bucket, key);
        let encrypted_data = storage.download(&location).await?;
        let decrypted_data = encryption.decrypt(&encrypted_data)?;
        tokio::fs::write(&target_path, &decrypted_data).await?;
    }

    info!(
        "Downloaded {} image(s) to {}",
        keys.len(),
        images_dir.display()
    );

    Ok(())
}

/// Re-exec the current binary to start the normal app flow
pub(crate) fn relaunch() {
    let exe = std::env::current_exe().expect("Failed to get current exe path");

    info!("Re-launching: {}", exe.display());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&exe).exec();
        error!("Failed to re-exec: {}", err);
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new(&exe)
            .spawn()
            .expect("Failed to relaunch");
        std::process::exit(0);
    }
}
