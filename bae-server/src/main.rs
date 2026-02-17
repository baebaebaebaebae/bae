mod cloud_proxy;

use std::collections::HashMap;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;

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
use cloud_proxy::{ChainState, CloudProxyState};
use std::time::Duration;
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

/// bae headless server -- Subsonic API server with background sync.
///
/// Syncs from a sync bucket on boot and periodically thereafter:
/// downloads the latest snapshot, applies changesets, downloads images,
/// then serves the library via the Subsonic API.
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

    /// S3 key prefix (scopes all keys under this path within the bucket).
    #[arg(long, env = "BAE_S3_KEY_PREFIX")]
    s3_key_prefix: Option<String>,

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

    /// Background sync interval in seconds.
    #[arg(long, default_value = "30", env = "BAE_SYNC_INTERVAL")]
    sync_interval: u64,
}

// libsqlite3-sys doesn't expose sqlite3_close_v2 in its generated bindings,
// but the bundled SQLite library includes it. Declare the symbol directly.
unsafe extern "C" {
    fn sqlite3_close_v2(db: *mut libsqlite3_sys::sqlite3) -> std::os::raw::c_int;
}

/// Wrapper around a raw sqlite3 pointer that is Send.
///
/// Safety: the sync loop is the sole user of this write handle. The pointer
/// is sent once to the sync thread and never shared. All FFI calls happen
/// sequentially on that one thread.
struct SqliteWriteHandle(*mut libsqlite3_sys::sqlite3);
unsafe impl Send for SqliteWriteHandle {}

impl Drop for SqliteWriteHandle {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close_v2(self.0);
        }
    }
}

#[derive(serde::Serialize)]
struct HealthStatus {
    status: &'static str,
    last_sync: Option<String>,
    changesets_applied: u64,
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

    // Connect to sync bucket (two S3CloudHome instances: one for the encrypted
    // sync bucket used during bootstrap, one for the raw write proxy).
    let cloud_home_for_bucket = S3CloudHome::new(
        args.s3_bucket.clone(),
        args.s3_region.clone(),
        args.s3_endpoint.clone(),
        args.s3_access_key.clone(),
        args.s3_secret_key.clone(),
        args.s3_key_prefix.clone(),
    )
    .await
    .unwrap_or_else(|e| {
        error!("Failed to connect to cloud home: {e}");
        std::process::exit(1);
    });

    let cloud_home_for_proxy: Arc<dyn bae_core::cloud_home::CloudHome> = Arc::new(
        S3CloudHome::new(
            args.s3_bucket.clone(),
            args.s3_region.clone(),
            args.s3_endpoint.clone(),
            args.s3_access_key.clone(),
            args.s3_secret_key.clone(),
            args.s3_key_prefix.clone(),
        )
        .await
        .unwrap_or_else(|e| {
            error!("Failed to connect to cloud home (proxy): {e}");
            std::process::exit(1);
        }),
    );

    let bucket = CloudHomeSyncBucket::new(Box::new(cloud_home_for_bucket), encryption.clone());

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
    let bootstrap_result = bootstrap_from_snapshot(&bucket, &encryption, &db_path)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to bootstrap from snapshot: {e}");
            std::process::exit(1);
        });

    info!(
        "Snapshot restored ({} device cursors)",
        bootstrap_result.cursors.len()
    );

    // Step 2: Open DB read-write for changeset application.
    // This handle stays open for the background sync loop.
    let write_handle = open_write_handle(&db_path);
    set_wal_mode(write_handle.0);

    // Step 3: Pull changesets since the snapshot.
    let (cursors, initial_applied) = pull_new_changesets(
        write_handle.0,
        &bucket,
        &bootstrap_result.cursors,
        &library_dir,
    )
    .await;

    if initial_applied > 0 {
        info!("Applied {initial_applied} changesets since snapshot");
    } else {
        info!("No new changesets since snapshot");
    }

    // Step 4: Download images from the sync bucket.
    download_images(&bucket, &library_dir).await;

    // Load membership chain from the bucket for write proxy auth.
    let chain_state = match cloud_proxy::load_membership_chain(&bucket).await {
        Ok(Some(chain)) => {
            info!("Membership chain loaded for write proxy auth");
            ChainState::Valid(chain)
        }
        Ok(None) => {
            info!("No membership chain found; write proxy accepts any valid signature");
            ChainState::None
        }
        Err(reason) => {
            warn!("Membership chain is corrupt: {reason}; write proxy will reject all requests");
            ChainState::Invalid
        }
    };

    let cloud_proxy_state = CloudProxyState {
        cloud_home: cloud_home_for_proxy,
        chain_state,
    };

    // Step 5: Open database read-only for serving.
    // WAL mode allows concurrent reads while the sync loop writes.
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
        library_dir.clone(),
        key_service,
        args.share_base_url,
        args.share_signing_key_version,
        auth,
    );

    // Health status shared between sync loop and health endpoint.
    let health = Arc::new(RwLock::new(HealthStatus {
        status: "ok",
        last_sync: None,
        changesets_applied: initial_applied,
    }));

    // Step 6: Spawn background sync loop.
    info!(
        "Starting background sync (interval: {}s)",
        args.sync_interval
    );

    let sync_bucket = Arc::new(bucket);
    let sync_library_dir = library_dir;
    let sync_health = health.clone();
    let sync_interval = args.sync_interval.max(5);

    // Run the sync loop on a dedicated thread with a single-threaded tokio
    // runtime. The raw sqlite3 pointer isn't Send, so the async future from
    // pull_changes can't be spawned on the multi-threaded runtime. A dedicated
    // thread with its own runtime avoids the Send requirement entirely.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build sync runtime");
        rt.block_on(sync_loop(
            sync_bucket,
            write_handle,
            sync_library_dir,
            sync_interval,
            cursors,
            sync_health,
        ));
    });

    // Build router with health endpoint.
    let health_state = health.clone();
    let health_handler = axum::routing::get(move || {
        let health = health_state.clone();
        async move {
            let status = health.read().await;
            axum::Json(HealthStatus {
                status: status.status,
                last_sync: status.last_sync.clone(),
                changesets_applied: status.changesets_applied,
            })
        }
    });

    let cloud_router = cloud_proxy::cloud_proxy_router(cloud_proxy_state);

    // If --web-dir is provided, serve static files with SPA fallback.
    let app = if let Some(ref web_dir) = args.web_dir {
        info!("Serving web UI from {}", web_dir.display());
        let spa_fallback =
            ServeDir::new(web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
        axum::Router::new()
            .route("/health", health_handler)
            .merge(api_router)
            .merge(cloud_router)
            .fallback_service(spa_fallback)
    } else {
        axum::Router::new()
            .route("/health", health_handler)
            .merge(api_router)
            .merge(cloud_router)
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

/// Open a raw sqlite3 handle in read-write mode.
fn open_write_handle(db_path: &std::path::Path) -> SqliteWriteHandle {
    unsafe {
        let c_path = CString::new(db_path.to_str().unwrap()).unwrap();
        let mut db: *mut libsqlite3_sys::sqlite3 = std::ptr::null_mut();
        let rc = libsqlite3_sys::sqlite3_open(c_path.as_ptr(), &mut db);
        if rc != libsqlite3_sys::SQLITE_OK {
            error!("Failed to open database for changeset application");
            std::process::exit(1);
        }
        SqliteWriteHandle(db)
    }
}

/// Enable WAL journal mode on the write handle so readers aren't blocked.
fn set_wal_mode(db: *mut libsqlite3_sys::sqlite3) {
    unsafe {
        let sql = CString::new("PRAGMA journal_mode=WAL").unwrap();
        let rc = libsqlite3_sys::sqlite3_exec(
            db,
            sql.as_ptr(),
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if rc != libsqlite3_sys::SQLITE_OK {
            error!("Failed to set WAL journal mode");
            std::process::exit(1);
        }

        info!("Database journal mode set to WAL");
    }
}

/// Pull new changesets and return updated cursors + count of applied changesets.
async fn pull_new_changesets(
    db: *mut libsqlite3_sys::sqlite3,
    bucket: &CloudHomeSyncBucket,
    cursors: &HashMap<String, u64>,
    library_dir: &LibraryDir,
) -> (HashMap<String, u64>, u64) {
    let server_device_id = "__bae-server__";

    match unsafe { pull_changes(db, bucket, server_device_id, cursors, None, library_dir).await } {
        Ok((updated_cursors, pull_result)) => (updated_cursors, pull_result.changesets_applied),
        Err(e) => {
            warn!("Failed to pull changesets: {e}");
            (cursors.clone(), 0)
        }
    }
}

/// Background sync loop: periodically pulls changesets and downloads images.
///
/// Owns the write handle for its entire lifetime. The raw sqlite3 pointer
/// never crosses a thread boundary -- it stays within this task.
async fn sync_loop(
    bucket: Arc<CloudHomeSyncBucket>,
    write_handle: SqliteWriteHandle,
    library_dir: LibraryDir,
    interval_secs: u64,
    initial_cursors: HashMap<String, u64>,
    health: Arc<RwLock<HealthStatus>>,
) {
    let mut cursors = initial_cursors;

    loop {
        // TODO(phase-1a): add write-trigger channel — use tokio::select! to wake on
        // either the timer or a write notification from the cloud proxy.
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;

        let (updated_cursors, applied) =
            pull_new_changesets(write_handle.0, &bucket, &cursors, &library_dir).await;

        cursors = updated_cursors;

        if applied > 0 {
            info!("Sync: applied {applied} changesets");
        }

        // Update health status.
        let now = chrono::Utc::now().to_rfc3339();
        let mut status = health.write().await;
        status.last_sync = Some(now);
        status.changesets_applied += applied;
        drop(status);

        // Download new images (skips already-cached ones).
        download_images(&bucket, &library_dir).await;
    }
}

/// Download all images from the sync bucket.
/// Skips images that already exist locally. Only logs when there's
/// actual work (new downloads or failures).
async fn download_images(bucket: &CloudHomeSyncBucket, library_dir: &LibraryDir) {
    let image_keys = match bucket.list_image_keys().await {
        Ok(keys) => keys,
        Err(e) => {
            warn!("Failed to list images: {e} -- skipping image download");
            return;
        }
    };

    let mut downloaded = 0u64;
    let mut failed = 0u64;

    for key in &image_keys {
        // key is like "images/ab/cd/{id}"
        let dest = library_dir.join(key);

        if dest.exists() {
            continue;
        }

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

    if downloaded > 0 {
        info!("Downloaded {downloaded} images");
    }

    if failed > 0 {
        warn!("{failed} images failed to download");
    }
}
