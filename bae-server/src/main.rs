use bae_core::cloud_sync::CloudSyncService;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::subsonic::create_router;
use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};

/// bae headless server — serves the subsonic API without a desktop UI.
#[derive(Parser)]
#[command(name = "bae-server")]
struct Args {
    /// Hex-encoded encryption key (64 hex chars = 32 bytes).
    /// Required if the library contains encrypted files.
    #[arg(long, env = "BAE_RECOVERY_KEY")]
    recovery_key: Option<String>,

    /// Path to the library directory (contains library.db).
    #[arg(long, env = "BAE_LIBRARY_PATH")]
    library_path: PathBuf,

    /// Port for the subsonic API server.
    #[arg(long, default_value = "4533", env = "BAE_PORT")]
    port: u16,

    /// Address to bind the server to.
    #[arg(long, default_value = "0.0.0.0", env = "BAE_BIND")]
    bind: String,

    /// Re-download the library from cloud even if library.db already exists.
    #[arg(long)]
    refresh: bool,

    /// Library ID (used as S3 key prefix).
    #[arg(long, env = "BAE_LIBRARY_ID")]
    library_id: Option<String>,

    /// S3 bucket for cloud sync.
    #[arg(long, env = "BAE_CLOUD_BUCKET")]
    cloud_bucket: Option<String>,

    /// S3 region for cloud sync.
    #[arg(long, env = "BAE_CLOUD_REGION")]
    cloud_region: Option<String>,

    /// S3 endpoint for cloud sync (for S3-compatible services).
    #[arg(long, env = "BAE_CLOUD_ENDPOINT")]
    cloud_endpoint: Option<String>,

    /// S3 access key for cloud sync.
    #[arg(long, env = "BAE_CLOUD_ACCESS_KEY")]
    cloud_access_key: Option<String>,

    /// S3 secret key for cloud sync.
    #[arg(long, env = "BAE_CLOUD_SECRET_KEY")]
    cloud_secret_key: Option<String>,
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

#[tokio::main]
async fn main() {
    configure_logging();
    let args = Args::parse();

    info!("bae-server starting");

    let db_path = args.library_path.join("library.db");
    let needs_download = !db_path.exists() || args.refresh;

    // Download from cloud if library.db is missing or --refresh was passed
    if needs_download {
        download_from_cloud(&args).await;
    }

    if !db_path.exists() {
        error!("Database not found at {}", db_path.display());
        error!("Provide a valid --library-path or use cloud args to download");
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

    // Start subsonic server
    let app = create_router(library_manager, encryption_service);
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

async fn download_from_cloud(args: &Args) {
    // All cloud args + recovery key are required for download
    let recovery_key = args.recovery_key.as_deref().unwrap_or_else(|| {
        error!("--recovery-key is required to download from cloud");
        std::process::exit(1);
    });
    let library_id = args.library_id.as_deref().unwrap_or_else(|| {
        error!("--library-id is required to download from cloud");
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

    info!("Downloading library from cloud (bucket: {bucket}, library: {library_id})");

    let cloud = CloudSyncService::new(
        bucket.to_string(),
        region.to_string(),
        args.cloud_endpoint.clone(),
        access_key.to_string(),
        secret_key.to_string(),
        library_id.to_string(),
        encryption_service,
    )
    .await
    .unwrap_or_else(|e| {
        error!("Failed to create cloud sync service: {e}");
        std::process::exit(1);
    });

    // Validate encryption key fingerprint against meta.json
    info!("Validating encryption key fingerprint...");
    cloud.validate_key().await.unwrap_or_else(|e| {
        error!("Key validation failed: {e}");
        std::process::exit(1);
    });

    // Create library directory
    std::fs::create_dir_all(&args.library_path).unwrap_or_else(|e| {
        error!(
            "Failed to create library directory {}: {e}",
            args.library_path.display()
        );
        std::process::exit(1);
    });

    // Download and decrypt database
    let db_path = args.library_path.join("library.db");
    info!("Downloading database...");
    cloud.download_db(&db_path).await.unwrap_or_else(|e| {
        error!("Failed to download database: {e}");
        std::process::exit(1);
    });

    // Download and decrypt covers
    let covers_path = args.library_path.join("covers");
    info!("Downloading covers...");
    cloud
        .download_covers(&covers_path)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to download covers: {e}");
            std::process::exit(1);
        });

    info!("Cloud download complete");
}
