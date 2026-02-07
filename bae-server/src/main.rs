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

    // Validate library path
    let db_path = args.library_path.join("library.db");
    if !db_path.exists() {
        error!("Database not found at {}", db_path.display());
        error!("Provide a valid --library-path containing library.db");
        std::process::exit(1);
    }

    // Open database
    info!("Opening database at {}", db_path.display());
    let database = Database::new(db_path.to_str().unwrap())
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
