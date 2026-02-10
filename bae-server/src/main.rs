use bae_core::cloud_storage::{CloudStorage, S3CloudStorage, S3Config};
use bae_core::config::ConfigYaml;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::keys::KeyService;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::library_dir::{LibraryDir, Manifest};
use bae_core::subsonic::create_router;
use clap::Parser;
use std::path::{Path, PathBuf};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

/// bae headless server — read-only Subsonic API server.
///
/// Two boot modes (mutually exclusive):
///
/// 1. Local profile: --library-path /path/to/profile/
///    Reads manifest.json (or config.yaml) and library.db directly.
///
/// 2. Cloud profile: --s3-bucket + --s3-access-key + --s3-secret-key + --recovery-key
///    Downloads encrypted metadata from S3, decrypts locally, serves from --library-path.
#[derive(Parser)]
#[command(name = "bae-server")]
struct Args {
    /// Path to the local working directory.
    /// For local profiles: the profile directory containing library.db.
    /// For cloud profiles: where downloaded metadata is stored.
    #[arg(long, env = "BAE_LIBRARY_PATH")]
    library_path: PathBuf,

    /// Hex-encoded encryption key (64 hex chars = 32 bytes).
    /// Required for cloud profiles and for streaming encrypted files.
    #[arg(long, env = "BAE_RECOVERY_KEY")]
    recovery_key: Option<String>,

    /// S3 bucket name (enables cloud profile mode).
    #[arg(long, env = "BAE_S3_BUCKET")]
    s3_bucket: Option<String>,

    /// S3 region.
    #[arg(long, env = "BAE_S3_REGION")]
    s3_region: Option<String>,

    /// S3 endpoint URL (for S3-compatible services like MinIO).
    #[arg(long, env = "BAE_S3_ENDPOINT")]
    s3_endpoint: Option<String>,

    /// S3 access key.
    #[arg(long, env = "BAE_S3_ACCESS_KEY")]
    s3_access_key: Option<String>,

    /// S3 secret key.
    #[arg(long, env = "BAE_S3_SECRET_KEY")]
    s3_secret_key: Option<String>,

    /// Port for the Subsonic API server.
    #[arg(long, default_value = "4533", env = "BAE_PORT")]
    port: u16,

    /// Address to bind the server to.
    #[arg(long, default_value = "0.0.0.0", env = "BAE_BIND")]
    bind: String,

    /// Re-download metadata from cloud even if library.db already exists locally.
    #[arg(long)]
    refresh: bool,

    /// Path to the built bae-web dist directory.
    /// When provided, serves the web UI at / alongside the API at /rest/*.
    #[arg(long, env = "BAE_WEB_DIR")]
    web_dir: Option<PathBuf>,
}

/// Minimal config extracted from either config.yaml or manifest.json.
/// Contains only what bae-server needs to boot.
struct LibraryBootConfig {
    library_id: String,
    encryption_key_fingerprint: Option<String>,
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

/// Try loading boot config from config.yaml, falling back to manifest.json.
fn load_boot_config(library_path: &Path) -> LibraryBootConfig {
    // Try config.yaml first (library home directories have this)
    let config_path = library_path.join("config.yaml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
            error!("Failed to read {}: {e}", config_path.display());
            std::process::exit(1);
        });
        let yaml: ConfigYaml = serde_yaml::from_str(&content).unwrap_or_else(|e| {
            error!("Failed to parse {}: {e}", config_path.display());
            std::process::exit(1);
        });

        info!("Loaded config from config.yaml");
        return LibraryBootConfig {
            library_id: yaml.library_id,
            encryption_key_fingerprint: yaml.encryption_key_fingerprint,
        };
    }

    // Fall back to manifest.json (replicated profile directories have this)
    let manifest_path = library_path.join("manifest.json");
    if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
            error!("Failed to read {}: {e}", manifest_path.display());
            std::process::exit(1);
        });
        let manifest: Manifest = serde_json::from_str(&content).unwrap_or_else(|e| {
            error!("Failed to parse {}: {e}", manifest_path.display());
            std::process::exit(1);
        });

        info!(
            "Loaded config from manifest.json (profile: {})",
            manifest.profile_id
        );
        return LibraryBootConfig {
            library_id: manifest.library_id,
            encryption_key_fingerprint: manifest.encryption_key_fingerprint,
        };
    }

    error!(
        "Neither config.yaml nor manifest.json found at {}",
        library_path.display()
    );
    std::process::exit(1);
}

/// Boot config extracted from a cloud manifest during download.
fn boot_config_from_manifest(manifest: &Manifest) -> LibraryBootConfig {
    LibraryBootConfig {
        library_id: manifest.library_id.clone(),
        encryption_key_fingerprint: manifest.encryption_key_fingerprint.clone(),
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

    let library_dir = LibraryDir::new(args.library_path.clone());
    let is_cloud_mode = args.s3_bucket.is_some();
    let db_path = library_dir.db_path();
    let needs_download = is_cloud_mode && (!db_path.exists() || args.refresh);

    // Determine boot config: either from cloud download or local files
    let boot_config = if needs_download {
        let manifest = download_from_cloud(&args, &library_dir).await;
        boot_config_from_manifest(&manifest)
    } else {
        load_boot_config(&args.library_path)
    };

    if !db_path.exists() {
        error!("Database not found at {}", db_path.display());
        if is_cloud_mode {
            error!("Cloud download did not produce a database — check S3 credentials and bucket contents");
        } else {
            error!("Ensure the profile directory is populated, or use --s3-bucket to download from a cloud profile");
        }
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

    // Validate recovery key fingerprint against what the library expects
    if let (Some(ref expected_fp), Some(ref enc)) =
        (&boot_config.encryption_key_fingerprint, &encryption_service)
    {
        let actual_fp = enc.fingerprint();
        if *expected_fp != actual_fp {
            error!("Recovery key fingerprint mismatch: expected {expected_fp}, got {actual_fp}");
            std::process::exit(1);
        }

        info!("Encryption enabled (fingerprint: {actual_fp})");
    } else if encryption_service.is_some() {
        info!("Encryption enabled");
    } else {
        info!("No recovery key provided — encrypted files will not be streamable");
    }

    // Create library manager
    let library_manager =
        SharedLibraryManager::new(LibraryManager::new(database, encryption_service.clone()));

    // Expose CLI-provided S3 credentials as the env vars that KeyService reads in dev mode.
    // This allows the subsonic handler to construct S3CloudStorage for streaming audio
    // from cloud profiles using the server's credentials.
    if let Some(ak) = &args.s3_access_key {
        std::env::set_var("BAE_S3_ACCESS_KEY", ak);
    }
    if let Some(sk) = &args.s3_secret_key {
        std::env::set_var("BAE_S3_SECRET_KEY", sk);
    }

    // Create a dev-mode KeyService backed by env vars.
    // bae-server is headless, so we use dev mode + env vars instead of OS keyring.
    let key_service = KeyService::new(true, boot_config.library_id.clone());

    info!("Library ID: {}", boot_config.library_id);

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

/// Download metadata from a cloud profile: manifest, database, and images.
/// Returns the decrypted manifest for boot config extraction.
async fn download_from_cloud(args: &Args, library_dir: &LibraryDir) -> Manifest {
    let recovery_key = args.recovery_key.as_deref().unwrap_or_else(|| {
        error!("--recovery-key is required for cloud profile mode");
        std::process::exit(1);
    });
    let bucket = args.s3_bucket.as_deref().unwrap_or_else(|| {
        error!("--s3-bucket is required for cloud profile mode");
        std::process::exit(1);
    });
    let region = args.s3_region.as_deref().unwrap_or_else(|| {
        error!("--s3-region is required for cloud profile mode");
        std::process::exit(1);
    });
    let access_key = args.s3_access_key.as_deref().unwrap_or_else(|| {
        error!("--s3-access-key is required for cloud profile mode");
        std::process::exit(1);
    });
    let secret_key = args.s3_secret_key.as_deref().unwrap_or_else(|| {
        error!("--s3-secret-key is required for cloud profile mode");
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
        endpoint_url: args.s3_endpoint.clone(),
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
    let manifest_location = format!("s3://{}/manifest.json.enc", bucket);
    let encrypted_manifest = storage
        .download(&manifest_location)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to download manifest: {e}");
            std::process::exit(1);
        });
    let manifest_bytes = encryption_service
        .decrypt(&encrypted_manifest)
        .unwrap_or_else(|e| {
            error!("Failed to decrypt manifest (wrong key?): {e}");
            std::process::exit(1);
        });
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).unwrap_or_else(|e| {
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

    info!(
        "Key validated (library: {}, profile: {})",
        manifest.library_id, manifest.profile_id
    );

    // Create working directory
    std::fs::create_dir_all(library_dir.as_ref()).unwrap_or_else(|e| {
        error!(
            "Failed to create working directory {}: {e}",
            library_dir.display()
        );
        std::process::exit(1);
    });

    // Save decrypted manifest locally
    std::fs::write(library_dir.manifest_path(), &manifest_bytes).unwrap_or_else(|e| {
        error!("Failed to write manifest: {e}");
        std::process::exit(1);
    });

    // Download and decrypt database
    info!("Downloading database...");
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

    info!("Restored database ({} bytes)", decrypted_db.len());

    // Download and decrypt images
    download_images(&storage, &encryption_service, library_dir, bucket).await;

    info!("Cloud download complete");
    manifest
}

/// Download all images from a cloud profile, decrypting each one.
async fn download_images(
    storage: &S3CloudStorage,
    encryption_service: &EncryptionService,
    library_dir: &LibraryDir,
    bucket: &str,
) {
    info!("Downloading images...");

    let image_keys = match storage.list_keys("images/").await {
        Ok(keys) => keys,
        Err(e) => {
            warn!("Failed to list images: {e} — skipping image download");
            return;
        }
    };

    if image_keys.is_empty() {
        info!("No images to download");
        return;
    }

    info!("Found {} images to download", image_keys.len());
    let mut downloaded = 0;
    let mut failed = 0;

    for key in &image_keys {
        let location = format!("s3://{}/{}", bucket, key);
        let encrypted_data = match storage.download(&location).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to download image {}: {e}", key);
                failed += 1;
                continue;
            }
        };

        let decrypted_data = match encryption_service.decrypt(&encrypted_data) {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to decrypt image {}: {e}", key);
                failed += 1;
                continue;
            }
        };

        // key is like "images/ab/cd/{id}" — write to library_dir/{key}
        let dest = library_dir.join(key);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Failed to create directory for {}: {e}", key);
                failed += 1;
                continue;
            }
        }

        if let Err(e) = std::fs::write(&dest, &decrypted_data) {
            warn!("Failed to write image {}: {e}", dest.display());
            failed += 1;
            continue;
        }

        downloaded += 1;
    }

    info!("Downloaded {} images", downloaded);
    if failed > 0 {
        warn!("{} images failed to download", failed);
    }
}
