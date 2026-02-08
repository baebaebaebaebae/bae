use bae_core::db::Database;
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
    info!(
        "Creating library directory: {}",
        config.library_dir.display()
    );
    std::fs::create_dir_all(&*config.library_dir).expect("Failed to create library directory");
    let db_path = config.library_dir.db_path();
    info!("Initializing database at: {}", db_path.display());
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
    // In dev mode, Config::from_env() handles everything — no pointer file needed
    if config::Config::is_dev_mode() {
        return false;
    }
    let home_dir = dirs::home_dir().expect("Failed to get home directory");
    !home_dir.join(".bae").join("library").exists()
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
        runtime_handle.clone(),
    );

    let media_controls = match media_controls::setup_media_controls(
        playback_handle.clone(),
        library_manager.clone(),
        config.library_dir.clone(),
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

    // Initialize navigation + playback channels for menu shortcuts (must be before menu setup)
    ui::shortcuts::init_nav_channel();

    #[cfg(target_os = "macos")]
    ui::shortcuts::init_playback_channel();

    let ui_context = AppContext {
        library_manager: library_manager.clone(),
        config: config.clone(),
        import_handle,
        playback_handle,
        #[cfg(feature = "torrent")]
        torrent_manager,
        cache: cache_manager.clone(),
        key_service,
    };

    if config.subsonic_enabled {
        let subsonic_library = library_manager.clone();
        let subsonic_encryption = encryption_service.clone();
        let subsonic_port = config.subsonic_port;
        let subsonic_library_dir = config.library_dir.clone();
        runtime_handle.spawn(async move {
            start_subsonic_server(
                subsonic_library,
                subsonic_encryption,
                subsonic_port,
                subsonic_library_dir,
            )
            .await
        });
    }

    // Initialize auto-updater (checks for updates on launch)
    updater::start();

    info!("Starting UI");
    ui::launch_app(ui_context);
    info!("UI quit");
}

/// Start the Subsonic API server
async fn start_subsonic_server(
    library_manager: SharedLibraryManager,
    encryption_service: Option<encryption::EncryptionService>,
    port: u16,
    library_dir: bae_core::library_dir::LibraryDir,
) {
    info!("Starting Subsonic API server...");
    let app = create_router(library_manager, encryption_service, library_dir);
    let addr = format!("127.0.0.1:{}", port);
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
        max_connections: config.torrent_max_connections,
        max_uploads: config.torrent_max_uploads,
    }
}
