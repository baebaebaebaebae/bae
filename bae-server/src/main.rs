use bae_core::config::ConfigYaml;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::keys::KeyService;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::library_dir::LibraryDir;
use bae_core::subsonic::create_router;
use clap::Parser;
use std::path::{Path, PathBuf};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info};

/// bae headless server — serves the subsonic API without a desktop UI.
///
/// Reads library configuration from {library-path}/config.yaml.
/// Only secrets (recovery key, S3 credentials) are provided via CLI or env vars.
#[derive(Parser)]
#[command(name = "bae-server")]
struct Args {
    /// Absolute path to the library directory (contains library.db and config.yaml).
    #[arg(long, env = "BAE_LIBRARY_PATH")]
    library_path: PathBuf,

    /// Hex-encoded encryption key (64 hex chars = 32 bytes).
    /// Required if the library contains encrypted files.
    #[arg(long, env = "BAE_RECOVERY_KEY")]
    recovery_key: Option<String>,

    /// S3 bucket name for cloud download (--refresh or first-time setup).
    #[arg(long, env = "BAE_CLOUD_BUCKET")]
    cloud_bucket: Option<String>,

    /// S3 region for cloud download.
    #[arg(long, env = "BAE_CLOUD_REGION")]
    cloud_region: Option<String>,

    /// S3 endpoint URL for cloud download (S3-compatible services like MinIO).
    #[arg(long, env = "BAE_CLOUD_ENDPOINT")]
    cloud_endpoint: Option<String>,

    /// S3 access key for cloud download.
    #[arg(long, env = "BAE_CLOUD_ACCESS_KEY")]
    cloud_access_key: Option<String>,

    /// S3 secret key for cloud download.
    #[arg(long, env = "BAE_CLOUD_SECRET_KEY")]
    cloud_secret_key: Option<String>,

    /// Port for the subsonic API server.
    #[arg(long, default_value = "4533", env = "BAE_PORT")]
    port: u16,

    /// Address to bind the server to.
    #[arg(long, default_value = "0.0.0.0", env = "BAE_BIND")]
    bind: String,

    /// Re-download the library from cloud even if library.db already exists.
    #[arg(long)]
    refresh: bool,

    /// Path to the built bae-web dist directory.
    /// When provided, serves the web UI at / alongside the API at /rest/*.
    #[arg(long, env = "BAE_WEB_DIR")]
    web_dir: Option<PathBuf>,
}

fn configure_logging() {
    use tracing_subscriber::prelude::*;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

fn load_library_config(library_path: &Path) -> ConfigYaml {
    let config_path = library_path.join("config.yaml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
            error!("Failed to read {}: {e}", config_path.display());
            std::process::exit(1);
        });
        serde_yaml::from_str(&content).unwrap_or_else(|e| {
            error!("Failed to parse {}: {e}", config_path.display());
            std::process::exit(1);
        })
    } else {
        error!(
            "config.yaml not found at {} — required for library_id",
            library_path.display()
        );
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    configure_logging();
    let args = Args::parse();

    // Validate library path is absolute
    if !args.library_path.is_absolute() {
        error!(
            "--library-path must be an absolute path, got: {}",
            args.library_path.display()
        );
        std::process::exit(1);
    }

    info!("bae-server starting");
    info!("Library path: {}", args.library_path.display());

    let config = load_library_config(&args.library_path);
    let library_dir = LibraryDir::new(args.library_path.clone());
    let db_path = library_dir.db_path();
    let needs_download = !db_path.exists() || args.refresh;

    // Download from cloud if library.db is missing or --refresh was passed
    if needs_download {
        download_from_cloud(&args, &library_dir).await;
    }

    if !db_path.exists() {
        error!("Database not found at {}", db_path.display());
        error!("Ensure the library directory is populated or use --cloud-bucket/--cloud-access-key/--cloud-secret-key to download from cloud");
        std::process::exit(1);
    }

    // Open database read-only (no migrations, no writes)
    info!("Opening database at {}", db_path.display());
    let database = Database::open_read_only(db_path.to_str().unwrap())
        .await
        .unwrap_or_else(|e| {
            error!("Failed to open database: {e}");
            std::process::exit(1);
        });

    // Create encryption service from recovery key
    let encryption_service = args.recovery_key.as_deref().map(|key| {
        EncryptionService::new(key).unwrap_or_else(|e| {
            error!("Invalid recovery key: {e}");
            std::process::exit(1);
        })
    });

    if encryption_service.is_some() {
        info!("Encryption enabled");
    } else {
        info!("No recovery key provided — encrypted files will not be streamable");
    }

    // Create library manager
    let library_manager =
        SharedLibraryManager::new(LibraryManager::new(database, encryption_service.clone()));

    // Expose CLI-provided S3 credentials as the env vars that KeyService reads in dev mode.
    // KeyService falls back to BAE_S3_ACCESS_KEY / BAE_S3_SECRET_KEY when no per-profile var exists.
    if let Some(ak) = &args.cloud_access_key {
        std::env::set_var("BAE_S3_ACCESS_KEY", ak);
    }
    if let Some(sk) = &args.cloud_secret_key {
        std::env::set_var("BAE_S3_SECRET_KEY", sk);
    }

    // Create a dev-mode KeyService backed by env vars.
    // bae-server is headless, so we use dev mode + env vars instead of OS keyring.
    let key_service = KeyService::new(true, config.library_id.clone());

    // Build the API router
    let api_router = create_router(
        library_manager,
        encryption_service,
        library_dir,
        key_service,
    );

    // If --web-dir is provided, serve static files with SPA fallback
    let app = if let Some(ref web_dir) = args.web_dir {
        info!("Serving web UI from {}", web_dir.display());
        let spa_fallback =
            ServeDir::new(web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
        axum::Router::new()
            .merge(api_router)
            .fallback_service(spa_fallback)
    } else {
        api_router
    };

    let addr = format!("{}:{}", args.bind, args.port);

    info!("Binding to {addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    info!("bae-server listening on http://{addr}");
    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {e}");
        std::process::exit(1);
    }
}

async fn download_from_cloud(args: &Args, library_dir: &LibraryDir) {
    use bae_core::cloud_storage::{CloudStorage, S3CloudStorage, S3Config};

    let recovery_key = args.recovery_key.as_deref().unwrap_or_else(|| {
        error!("--recovery-key is required to download from cloud");
        std::process::exit(1);
    });
    let bucket = args.cloud_bucket.as_deref().unwrap_or_else(|| {
        error!("--cloud-bucket is required to download from cloud");
        std::process::exit(1);
    });
    let region = args.cloud_region.as_deref().unwrap_or_else(|| {
        error!("--cloud-region is required to download from cloud");
        std::process::exit(1);
    });
    let access_key = args.cloud_access_key.as_deref().unwrap_or_else(|| {
        error!("--cloud-access-key is required to download from cloud");
        std::process::exit(1);
    });
    let secret_key = args.cloud_secret_key.as_deref().unwrap_or_else(|| {
        error!("--cloud-secret-key is required to download from cloud");
        std::process::exit(1);
    });

    let encryption_service = EncryptionService::new(recovery_key).unwrap_or_else(|e| {
        error!("Invalid recovery key: {e}");
        std::process::exit(1);
    });
    let fingerprint = encryption_service.fingerprint();

    let s3_config = S3Config {
        bucket_name: bucket.to_string(),
        region: region.to_string(),
        access_key_id: access_key.to_string(),
        secret_access_key: secret_key.to_string(),
        endpoint_url: args.cloud_endpoint.clone(),
    };

    info!("Downloading library from cloud (bucket: {bucket})");

    let storage = S3CloudStorage::new_with_bucket_creation(s3_config, false)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to connect to S3: {e}");
            std::process::exit(1);
        });

    // Download and decrypt manifest to validate key
    info!("Downloading manifest...");
    let location = format!("s3://{}/manifest.json.enc", bucket);
    let encrypted_manifest = storage.download(&location).await.unwrap_or_else(|e| {
        error!("Failed to download manifest: {e}");
        std::process::exit(1);
    });
    let manifest_bytes = encryption_service
        .decrypt(&encrypted_manifest)
        .unwrap_or_else(|e| {
            error!("Failed to decrypt manifest (wrong key?): {e}");
            std::process::exit(1);
        });
    let manifest: bae_core::library_dir::Manifest = serde_json::from_slice(&manifest_bytes)
        .unwrap_or_else(|e| {
            error!("Failed to parse manifest: {e}");
            std::process::exit(1);
        });

    // Validate fingerprint
    if let Some(ref expected_fp) = manifest.encryption_key_fingerprint {
        if *expected_fp != fingerprint {
            error!(
                "Encryption key fingerprint mismatch: expected {}, got {}",
                expected_fp, fingerprint
            );
            std::process::exit(1);
        }
    }

    info!("Key validated, downloading library...");

    // Create library directory
    std::fs::create_dir_all(&args.library_path).unwrap_or_else(|e| {
        error!(
            "Failed to create library directory {}: {e}",
            args.library_path.display()
        );
        std::process::exit(1);
    });

    // Download and decrypt database
    let db_location = format!("s3://{}/library.db.enc", bucket);
    let encrypted_db = storage.download(&db_location).await.unwrap_or_else(|e| {
        error!("Failed to download database: {e}");
        std::process::exit(1);
    });
    let decrypted_db = encryption_service
        .decrypt(&encrypted_db)
        .unwrap_or_else(|e| {
            error!("Failed to decrypt database: {e}");
            std::process::exit(1);
        });

    let db_path = library_dir.db_path();
    std::fs::write(&db_path, &decrypted_db).unwrap_or_else(|e| {
        error!("Failed to write database: {e}");
        std::process::exit(1);
    });

    info!("Restored DB ({} bytes)", decrypted_db.len());

    // Create images directory (images will be served from cloud profiles on demand)
    let images_dir = library_dir.images_dir();
    std::fs::create_dir_all(&images_dir).unwrap_or_else(|e| {
        error!("Failed to create images directory: {e}");
        std::process::exit(1);
    });

    info!("Cloud download complete");
}
