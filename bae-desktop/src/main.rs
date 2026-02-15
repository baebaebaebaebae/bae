use bae_core::db::Database;
use bae_core::image_server;
use bae_core::keys::KeyService;
use bae_core::library::SharedLibraryManager;
use bae_core::subsonic::create_router;
use bae_core::{audio_codec, cache, config, encryption, import, playback};
#[cfg(feature = "torrent")]
use bae_core::{network, torrent};
#[cfg(feature = "torrent")]
use tracing::warn;
use tracing::{error, info};

mod crash_report;
mod media_controls;
mod ui;
mod updater;

pub use ui::AppContext;

/// Initialize cache manager
async fn create_cache_manager() -> cache::CacheManager {
    let cache_manager = cache::CacheManager::new()
        .await
        .expect("Failed to create cache manager");
    info!("Cache manager created");
    cache_manager
}

/// Initialize database
async fn create_database(config: &config::Config) -> Database {
    std::fs::create_dir_all(&*config.library_dir).expect("Failed to create library directory");
    let db_path = config.library_dir.db_path();
    info!("Opening database at {}", db_path.display());
    let database = Database::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create database");
    info!("Database created");
    database
}

/// Initialize library manager with all dependencies
fn create_library_manager(
    database: Database,
    encryption_service: Option<encryption::EncryptionService>,
) -> SharedLibraryManager {
    let library_manager = bae_core::library::LibraryManager::new(database, encryption_service);
    info!("Library manager created");
    let shared_library = SharedLibraryManager::new(library_manager);
    info!("SharedLibraryManager created");
    shared_library
}

fn configure_logging() {
    use tracing_subscriber::prelude::*;

    // Default to info level if RUST_LOG not set
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true);

    // Always log to console. In release mode on macOS, also log to Console.app.
    #[cfg(target_os = "macos")]
    if !config::Config::is_dev_mode() {
        let oslog_layer = tracing_oslog::OsLogger::new("com.bae.app", "default");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(oslog_layer)
            .init();
        return;
    }

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

fn is_first_run() -> bool {
    let home_dir = dirs::home_dir().expect("Failed to get home directory");
    !home_dir.join(".bae").join("active-library").exists()
}

fn main() {
    crash_report::install_panic_hook();
    config::init_keyring();
    configure_logging();

    // Detect first run BEFORE Config::load() (which creates the pointer file)
    if is_first_run() {
        info!("First run detected — launching welcome screen");
        let dev_mode = config::Config::is_dev_mode();
        ui::components::welcome::launch_welcome(dev_mode);
        // launch_welcome returns when user closes the window.
        // If they chose "Create new" or "Restore", the process was re-exec'd.
        // If they just closed the window, exit cleanly.
        return;
    }

    let mut config = config::Config::load();
    crash_report::check_for_crash_report();

    // Initialize FFmpeg for audio processing
    audio_codec::init();

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let runtime_handle = runtime.handle().clone();

    info!("Building dependencies...");
    let cache_manager = runtime_handle.block_on(create_cache_manager());
    let database = runtime_handle.block_on(create_database(&config));

    let dev_mode = config::Config::is_dev_mode();
    let key_service = KeyService::new(dev_mode, config.library_id.clone());

    // One-time migration from global keyring entries to per-library namespaced entries
    if !config.keys_migrated {
        key_service.migrate_global_keys();
        config.keys_migrated = true;
        if let Err(e) = config.save() {
            error!("Failed to save config after key migration: {e}");
        }
    }

    // Load or generate user Ed25519 keypair (global identity for sync signing and invitations)
    let user_keypair = match key_service.get_or_create_user_keypair() {
        Ok(kp) => {
            info!(
                "User keypair loaded (pubkey: {}...)",
                &hex::encode(kp.public_key)[..8]
            );
            Some(kp)
        }
        Err(e) => {
            error!("Failed to load/create user keypair: {e}");
            None
        }
    };

    // If config says we have an encryption key but it's missing from the keyring,
    // show the unlock screen so the user can paste their recovery key.
    if config.encryption_key_stored {
        if let Some(ref fp) = config.encryption_key_fingerprint {
            if key_service.get_encryption_key().is_none() {
                info!("Encryption key missing from keyring — launching unlock screen");
                ui::components::unlock::launch_unlock(key_service, fp.clone());
                return;
            }
        }
    }

    // Create encryption service only if hint flag says a key is stored (avoids keyring prompt)
    let encryption_service = if config.encryption_key_stored {
        key_service.get_encryption_key().and_then(|key| {
            let service = encryption::EncryptionService::new(&key).ok()?;
            let fingerprint = service.fingerprint();

            match &config.encryption_key_fingerprint {
                Some(stored) if stored != &fingerprint => {
                    error!(
                        "Encryption key fingerprint mismatch! Expected {stored}, got {fingerprint}. \
                         Wrong key in keyring — encryption disabled."
                    );
                    None
                }
                None => {
                    // First run after upgrade — save fingerprint for future validation
                    info!("Saving encryption key fingerprint: {fingerprint}");
                    config.encryption_key_fingerprint = Some(fingerprint);
                    if let Err(e) = config.save() {
                        error!("Failed to save config with fingerprint: {e}");
                    }
                    Some(service)
                }
                Some(_) => Some(service),
            }
        })
    } else {
        None
    };
    let library_manager = create_library_manager(database.clone(), encryption_service.clone());

    // Initialize sync infrastructure if sync is configured and encryption is enabled
    let sync_handle = if config.sync_enabled(&key_service) {
        if let Some(ref enc) = encryption_service {
            runtime_handle.block_on(create_sync_handle(&config, &key_service, &database, enc))
        } else {
            info!(
                "Sync is configured but encryption is not enabled — skipping sync initialization"
            );
            None
        }
    } else {
        None
    };

    // Ensure manifest.json exists (idempotent, runs every startup)
    ensure_manifest(&config, encryption_service.as_ref());

    #[cfg(feature = "torrent")]
    let torrent_manager = {
        let torrent_options = torrent_options_from_config(&config);
        torrent::LazyTorrentManager::new(cache_manager.clone(), database.clone(), torrent_options)
    };

    #[cfg(feature = "torrent")]
    let import_handle = import::ImportService::start(
        runtime_handle.clone(),
        library_manager.clone(),
        encryption_service.clone(),
        torrent_manager.clone(),
        std::sync::Arc::new(database.clone()),
        key_service.clone(),
        config.library_dir.clone(),
    );
    #[cfg(not(feature = "torrent"))]
    let import_handle = import::ImportService::start(
        runtime_handle.clone(),
        library_manager.clone(),
        encryption_service.clone(),
        std::sync::Arc::new(database.clone()),
        key_service.clone(),
        config.library_dir.clone(),
    );

    let playback_handle = playback::PlaybackService::start(
        library_manager.get().clone(),
        encryption_service.clone(),
        config.library_dir.clone(),
        runtime_handle.clone(),
    );

    // Start image server (always on, OS-assigned port)
    let image_server = runtime_handle.block_on(image_server::start_image_server(
        library_manager.clone(),
        config.library_dir.clone(),
        encryption_service.clone(),
        "127.0.0.1",
    ));

    let media_controls = match media_controls::setup_media_controls(
        playback_handle.clone(),
        library_manager.clone(),
        image_server.clone(),
        runtime_handle.clone(),
    ) {
        Ok(controls) => {
            info!("Media controls setup successful");
            Some(controls)
        }
        Err(e) => {
            error!("Failed to setup media controls: {:?}", e);
            error!("Media key support will not be available");
            None
        }
    };
    let _keep_alive = media_controls;

    // Initialize navigation + playback + URL channels (must be before menu/handler setup)
    ui::shortcuts::init_nav_channel();
    ui::shortcuts::init_url_channel();

    #[cfg(target_os = "macos")]
    ui::shortcuts::init_playback_channel();

    #[cfg(target_os = "macos")]
    ui::window_activation::register_url_handler();

    // On cold launch, macOS may pass the URL as a CLI argument
    for arg in std::env::args().skip(1) {
        if arg.starts_with("bae://") {
            info!("URL from CLI argument: {arg}");
            ui::shortcuts::send_url(arg);
        }
    }

    if config.server_enabled {
        let subsonic_library = library_manager.clone();
        let subsonic_encryption = encryption_service.clone();
        let subsonic_port = config.server_port;
        let subsonic_bind_address = config.server_bind_address.clone();
        let subsonic_library_dir = config.library_dir.clone();
        let subsonic_key_service = key_service.clone();
        let subsonic_share_base_url = config.share_base_url.clone();
        let subsonic_share_signing_key_version = config.share_signing_key_version;

        let subsonic_auth = if config.server_auth_enabled {
            let password = key_service.get_server_password();
            bae_core::subsonic::SubsonicAuth {
                enabled: config.server_username.is_some() && password.is_some(),
                username: config.server_username.clone(),
                password,
            }
        } else {
            bae_core::subsonic::SubsonicAuth {
                enabled: false,
                username: None,
                password: None,
            }
        };

        runtime_handle.spawn(async move {
            start_subsonic_server(
                subsonic_library,
                subsonic_encryption,
                subsonic_port,
                subsonic_bind_address,
                subsonic_library_dir,
                subsonic_key_service,
                subsonic_share_base_url,
                subsonic_share_signing_key_version,
                subsonic_auth,
            )
            .await
        });
    }

    let ui_context = AppContext {
        library_manager: library_manager.clone(),
        config: config.clone(),
        import_handle,
        playback_handle,
        #[cfg(feature = "torrent")]
        torrent_manager,
        cache: cache_manager.clone(),
        key_service,
        image_server,
        user_keypair,
        sync_handle,
    };

    // Initialize auto-updater (checks for updates on launch)
    updater::start();

    info!("Starting UI");
    ui::launch_app(ui_context);
    info!("UI quit");
}

/// Ensure a home storage profile exists and manifest.json is present at the library root.
///
/// On first launch after library creation, no home profile exists yet. This creates one
/// and writes manifest.json. On subsequent launches, this is a no-op.
fn ensure_manifest(
    config: &config::Config,
    encryption_service: Option<&encryption::EncryptionService>,
) {
    let manifest_path = config.library_dir.manifest_path();
    if manifest_path.exists() {
        return;
    }

    info!("Writing manifest.json");
    let manifest = bae_core::library_dir::Manifest {
        library_id: config.library_id.clone(),
        library_name: config.library_name.clone(),
        encryption_key_fingerprint: encryption_service.map(|e| e.fingerprint()),
    };

    match serde_json::to_string_pretty(&manifest) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&manifest_path, json) {
                error!("Failed to write manifest.json: {e}");
            }
        }
        Err(e) => {
            error!("Failed to serialize manifest: {e}");
        }
    }
}

/// Start the Subsonic API server
async fn start_subsonic_server(
    library_manager: SharedLibraryManager,
    encryption_service: Option<encryption::EncryptionService>,
    port: u16,
    bind_address: String,
    library_dir: bae_core::library_dir::LibraryDir,
    key_service: bae_core::keys::KeyService,
    share_base_url: Option<String>,
    share_signing_key_version: u32,
    auth: bae_core::subsonic::SubsonicAuth,
) {
    info!("Starting Subsonic API server...");
    let app = create_router(
        library_manager,
        encryption_service,
        library_dir,
        key_service,
        share_base_url,
        share_signing_key_version,
        auth,
    );
    let addr = format!("{}:{}", bind_address, port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            info!("Subsonic API server listening on http://{}", addr);
            listener
        }
        Err(e) => {
            error!("Failed to bind Subsonic server: {}", e);
            return;
        }
    };
    if let Err(e) = axum::serve(listener, app).await {
        error!("Subsonic server error: {}", e);
    }
}

/// Create the sync handle if sync bucket credentials and configuration are available.
///
/// Extracts the raw sqlite3 write handle, creates the S3 bucket client, HLC,
/// and starts the initial sync session. Returns None if any step fails.
async fn create_sync_handle(
    config: &config::Config,
    key_service: &KeyService,
    database: &Database,
    encryption: &encryption::EncryptionService,
) -> Option<ui::app_context::SyncHandle> {
    use bae_core::cloud_home::s3::S3CloudHome;
    use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
    use bae_core::sync::hlc::Hlc;
    use bae_core::sync::session::SyncSession;

    let bucket = config.cloud_home_s3_bucket.as_ref()?;
    let region = config.cloud_home_s3_region.as_ref()?;
    let endpoint = config.cloud_home_s3_endpoint.clone();

    let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
        Some(bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        }) => (access_key, secret_key),
        _ => return None,
    };

    let cloud_home = match S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint,
        access_key,
        secret_key,
    )
    .await
    {
        Ok(home) => home,
        Err(e) => {
            error!("Failed to create cloud home: {e}");
            return None;
        }
    };

    let bucket_client = CloudHomeSyncBucket::new(Box::new(cloud_home), encryption.clone());

    let raw_db = match database.raw_write_handle().await {
        Ok(ptr) => ptr,
        Err(e) => {
            error!("Failed to extract raw write handle for sync: {e}");
            return None;
        }
    };

    // Start the initial sync session to begin recording changes
    let session = match unsafe { SyncSession::start(raw_db) } {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to start initial sync session: {e}");
            return None;
        }
    };

    let hlc = Hlc::new(config.device_id.clone());

    // Channel for manual sync trigger (Phase 5d). Capacity of 1 is sufficient
    // since multiple triggers collapse into one sync cycle.
    let (sync_trigger_tx, sync_trigger_rx) = tokio::sync::mpsc::channel::<()>(1);

    info!(
        "Sync initialized (bucket: {bucket}, device: {})",
        config.device_id
    );

    Some(ui::app_context::SyncHandle::new(
        bucket_client,
        hlc,
        config.device_id.clone(),
        raw_db,
        session,
        sync_trigger_tx,
        sync_trigger_rx,
    ))
}

/// Create torrent client options from application config
#[cfg(feature = "torrent")]
fn torrent_options_from_config(config: &config::Config) -> torrent::client::TorrentClientOptions {
    let bind_interface = if let Some(interface) = &config.torrent_bind_interface {
        match network::validate_network_interface(interface) {
            Ok(()) => {
                info!(
                    "Torrent client configured to bind to interface: {}",
                    interface
                );
                Some(interface.clone())
            }
            Err(e) => {
                warn!(
                    "Configured torrent interface '{}' not available: {}. Using default binding.",
                    interface, e
                );
                None
            }
        }
    } else {
        info!("Torrent client using default network binding");
        None
    };
    torrent::client::TorrentClientOptions {
        bind_interface,
        listen_port: config.torrent_listen_port,
        enable_upnp: config.torrent_enable_upnp,
        enable_natpmp: config.torrent_enable_natpmp,
        enable_dht: config.torrent_enable_dht,
        max_connections: config.torrent_max_connections,
        max_uploads: config.torrent_max_uploads,
    }
}
