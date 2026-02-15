use bae_core::cloud_home::s3::S3CloudHome;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::keys::KeyService;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::library_dir::LibraryDir;
use bae_core::subsonic::create_router;
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
use bae_core::sync::pull::pull_changes;
use bae_core::sync::snapshot::bootstrap_from_snapshot;
use clap::Parser;
use std::collections::HashMap;
use std::ffi::CString;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

/// bae headless server -- read-only Subsonic API server.
///
/// Syncs from a sync bucket on boot: downloads the latest snapshot,
/// applies any new changesets, downloads images, then serves the
/// library via the Subsonic API.
///
/// Requires sync bucket S3 coordinates and the library encryption key.
#[derive(Parser)]
#[command(name = "bae-server")]
struct Args {
    /// Local working directory where the database and images are cached.
    #[arg(long, env = "BAE_LIBRARY_PATH")]
    library_path: PathBuf,

    /// Hex-encoded encryption key (64 hex chars = 32 bytes).
    #[arg(long, env = "BAE_RECOVERY_KEY")]
    recovery_key: String,

    /// Sync bucket name.
    #[arg(long, env = "BAE_S3_BUCKET")]
    s3_bucket: String,

    /// S3 region.
    #[arg(long, env = "BAE_S3_REGION")]
    s3_region: String,

    /// S3 endpoint URL (for S3-compatible services like MinIO).
    #[arg(long, env = "BAE_S3_ENDPOINT")]
    s3_endpoint: Option<String>,

    /// S3 access key.
    #[arg(long, env = "BAE_S3_ACCESS_KEY")]
    s3_access_key: String,

    /// S3 secret key.
    #[arg(long, env = "BAE_S3_SECRET_KEY")]
    s3_secret_key: String,

    /// Port for the Subsonic API server.
    #[arg(long, default_value = "4533", env = "BAE_PORT")]
    port: u16,

    /// Address to bind the server to.
    #[arg(long, default_value = "0.0.0.0", env = "BAE_BIND")]
    bind: String,

    /// Library ID (used for KeyService namespace).
    #[arg(long, env = "BAE_LIBRARY_ID")]
    library_id: String,

    /// Path to the built bae-web dist directory.
    /// When provided, serves the web UI at / alongside the API at /rest/*.
    #[arg(long, env = "BAE_WEB_DIR")]
    web_dir: Option<PathBuf>,

    /// Base URL for share links (e.g. "https://listen.example.com").
    #[arg(long, env = "BAE_SHARE_BASE_URL")]
    share_base_url: Option<String>,

    /// Share token signing key version. Increment to invalidate all outstanding share links.
    #[arg(long, default_value = "1", env = "BAE_SHARE_SIGNING_KEY_VERSION")]
    share_signing_key_version: u32,

    /// Server username for authentication.
    /// When both username and password are provided, authentication is required.
    #[arg(long, env = "BAE_SERVER_USERNAME")]
    server_username: Option<String>,

    /// Server password for authentication.
    /// When both username and password are provided, authentication is required.
    #[arg(long, env = "BAE_SERVER_PASSWORD")]
    server_password: Option<String>,
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

    if !args.library_path.is_absolute() {
        error!(
            "--library-path must be an absolute path, got: {}",
            args.library_path.display()
        );
        std::process::exit(1);
    }

    info!("bae-server starting");
    info!("Library path: {}", args.library_path.display());
    info!("Library ID: {}", args.library_id);

    let library_dir = LibraryDir::new(args.library_path.clone());

    let encryption = EncryptionService::new(&args.recovery_key).unwrap_or_else(|e| {
        error!("Invalid recovery key: {e}");
        std::process::exit(1);
    });

    info!(
        "Encryption enabled (fingerprint: {})",
        encryption.fingerprint()
    );

    // Connect to sync bucket.
    let cloud_home = S3CloudHome::new(
        args.s3_bucket.clone(),
        args.s3_region.clone(),
        args.s3_endpoint.clone(),
        args.s3_access_key.clone(),
        args.s3_secret_key.clone(),
    )
    .await
    .unwrap_or_else(|e| {
        error!("Failed to connect to cloud home: {e}");
        std::process::exit(1);
    });

    let bucket = CloudHomeSyncBucket::new(Box::new(cloud_home), encryption.clone());

    info!("Connected to sync bucket: {}", args.s3_bucket);

    // Create working directory.
    std::fs::create_dir_all(library_dir.as_ref()).unwrap_or_else(|e| {
        error!(
            "Failed to create working directory {}: {e}",
            library_dir.display()
        );
        std::process::exit(1);
    });

    // Step 1: Bootstrap from snapshot.
    let db_path = library_dir.db_path();

    info!("Downloading snapshot...");
    let snapshot_seq = bootstrap_from_snapshot(&bucket, &encryption, &db_path)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to bootstrap from snapshot: {e}");
            std::process::exit(1);
        });

    info!("Snapshot restored (snapshot_seq: {snapshot_seq})");

    // Step 2: Pull changesets since the snapshot.
    // Open DB read-write via raw sqlite3 for changeset application.
    let changesets_applied = apply_changesets(&bucket, &db_path, snapshot_seq, &library_dir).await;
    if changesets_applied > 0 {
        info!("Applied {changesets_applied} changesets since snapshot");
    } else {
        info!("No new changesets since snapshot");
    }

    // Step 3: Download images from the sync bucket.
    download_images(&bucket, &library_dir).await;

    // Step 4: Open database read-only for serving.
    info!("Opening database read-only at {}", db_path.display());
    let database = Database::open_read_only(db_path.to_str().unwrap())
        .await
        .unwrap_or_else(|e| {
            error!("Failed to open database: {e}");
            std::process::exit(1);
        });

    // Expose S3 credentials as env vars for KeyService (dev mode).
    std::env::set_var("BAE_S3_ACCESS_KEY", &args.s3_access_key);
    std::env::set_var("BAE_S3_SECRET_KEY", &args.s3_secret_key);

    let key_service = KeyService::new(true, args.library_id.clone());
    let library_manager =
        SharedLibraryManager::new(LibraryManager::new(database, Some(encryption.clone())));

    let auth = match (&args.server_username, &args.server_password) {
        (Some(username), Some(password)) => {
            info!("Authentication enabled for user: {username}");
            bae_core::subsonic::SubsonicAuth {
                enabled: true,
                username: Some(username.clone()),
                password: Some(password.clone()),
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            warn!("Both --server-username and --server-password must be set to enable auth; running without authentication");
            bae_core::subsonic::SubsonicAuth {
                enabled: false,
                username: None,
                password: None,
            }
        }
        (None, None) => bae_core::subsonic::SubsonicAuth {
            enabled: false,
            username: None,
            password: None,
        },
    };

    let api_router = create_router(
        library_manager,
        Some(encryption),
        library_dir,
        key_service,
        args.share_base_url,
        args.share_signing_key_version,
        auth,
    );

    // If --web-dir is provided, serve static files with SPA fallback.
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

/// Open the database with raw sqlite3, pull and apply changesets, then close.
///
/// Returns the number of changesets applied.
async fn apply_changesets(
    bucket: &CloudHomeSyncBucket,
    db_path: &std::path::Path,
    snapshot_seq: u64,
    library_dir: &LibraryDir,
) -> u64 {
    unsafe {
        let c_path = CString::new(db_path.to_str().unwrap()).unwrap();
        let mut db: *mut libsqlite3_sys::sqlite3 = std::ptr::null_mut();
        let rc = libsqlite3_sys::sqlite3_open(c_path.as_ptr(), &mut db);
        if rc != libsqlite3_sys::SQLITE_OK {
            error!("Failed to open database for changeset application");
            std::process::exit(1);
        }

        // Build cursors: since we bootstrapped from a fresh snapshot,
        // we know all devices' data is covered up to snapshot_seq.
        // We need to pull only changesets with seq > snapshot_seq.
        let heads = match bucket.list_heads().await {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to list heads for changeset pull: {e}");
                libsqlite3_sys::sqlite3_close(db);
                return 0;
            }
        };

        let mut cursors = HashMap::new();
        for head in &heads {
            // The snapshot covers everything up to snapshot_seq,
            // so we set all cursors to snapshot_seq.
            cursors.insert(head.device_id.clone(), snapshot_seq);
        }

        // bae-server is a passive consumer -- use a device ID that won't
        // match any real device so pull_changes doesn't skip any heads.
        let server_device_id = "__bae-server__";

        let result =
            match pull_changes(db, bucket, server_device_id, &cursors, None, library_dir).await {
                Ok((_updated_cursors, pull_result)) => pull_result.changesets_applied,
                Err(e) => {
                    warn!("Failed to pull changesets: {e}");
                    0
                }
            };

        libsqlite3_sys::sqlite3_close(db);
        result
    }
}

/// Download all images from the sync bucket.
/// The bucket client handles decryption internally.
async fn download_images(bucket: &CloudHomeSyncBucket, library_dir: &LibraryDir) {
    info!("Downloading images...");

    let image_keys = match bucket.list_image_keys().await {
        Ok(keys) => keys,
        Err(e) => {
            warn!("Failed to list images: {e} -- skipping image download");
            return;
        }
    };

    if image_keys.is_empty() {
        info!("No images to download");
        return;
    }

    info!("Found {} images to download", image_keys.len());
    let mut downloaded = 0u64;
    let mut skipped = 0u64;
    let mut failed = 0u64;

    for key in &image_keys {
        // key is like "images/ab/cd/{id}"
        let dest = library_dir.join(key);

        // Skip images that already exist locally (from previous runs).
        if dest.exists() {
            skipped += 1;
            continue;
        }

        // Download the encrypted blob and decrypt.
        let id = match key.rsplit('/').next() {
            Some(id) => id,
            None => {
                warn!("Unexpected image key format: {key}");
                failed += 1;
                continue;
            }
        };

        let decrypted = match bucket.download_image(id).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to download image {id}: {e}");
                failed += 1;
                continue;
            }
        };

        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Failed to create directory for {key}: {e}");
                failed += 1;
                continue;
            }
        }

        if let Err(e) = std::fs::write(&dest, &decrypted) {
            warn!("Failed to write image {}: {e}", dest.display());
            failed += 1;
            continue;
        }

        downloaded += 1;
    }

    info!("Images: {downloaded} downloaded, {skipped} already cached");
    if failed > 0 {
        warn!("{failed} images failed to download");
    }
}
