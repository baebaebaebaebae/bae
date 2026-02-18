mod proxy;
mod registry;
mod s3;
mod share;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

use proxy::{proxy_router, ProxyState};
use registry::Registry;

#[derive(Parser)]
#[command(name = "bae-proxy")]
struct Args {
    /// Path to the registry YAML file.
    #[arg(long, env = "BAE_REGISTRY_PATH")]
    registry_path: PathBuf,

    /// Port to listen on.
    #[arg(long, default_value = "4535", env = "BAE_PORT")]
    port: u16,

    /// Address to bind to.
    #[arg(long, default_value = "0.0.0.0", env = "BAE_BIND")]
    bind: String,

    /// Path to the built bae-web dist directory (enables share link UI).
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

#[tokio::main]
async fn main() {
    configure_logging();
    let args = Args::parse();

    let registry = Registry::load(&args.registry_path).unwrap_or_else(|e| {
        error!("failed to load registry: {e}");
        std::process::exit(1);
    });

    info!(
        "loaded registry with {} libraries",
        registry.libraries.len()
    );

    let registry = Arc::new(RwLock::new(registry));
    let s3_clients = Arc::new(RwLock::new(HashMap::new()));

    // Start file watcher for registry hot-reload.
    let watcher_registry = registry.clone();
    let watcher_s3_clients = s3_clients.clone();
    let watcher_path = args.registry_path.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let _watcher = {
        let tx = tx.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        let _ = tx.blocking_send(());
                    }
                }
            },
            notify::Config::default(),
        )
        .unwrap_or_else(|e| {
            error!("failed to create file watcher: {e}");
            std::process::exit(1);
        });

        watcher
            .watch(&args.registry_path, RecursiveMode::NonRecursive)
            .unwrap_or_else(|e| {
                error!("failed to watch registry file: {e}");
                std::process::exit(1);
            });

        info!("watching registry file: {}", args.registry_path.display());
        watcher
    };

    // Spawn the reload task.
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            // Drain any extra notifications that arrived while we were processing.
            while rx.try_recv().is_ok() {}

            match Registry::load(&watcher_path) {
                Ok(new_registry) => {
                    info!(
                        "registry reloaded ({} libraries)",
                        new_registry.libraries.len()
                    );
                    *watcher_registry.write().await = new_registry;

                    // Clear cached S3 clients so they're recreated with fresh config.
                    watcher_s3_clients.write().await.clear();
                }
                Err(e) => {
                    warn!("failed to reload registry: {e}");
                }
            }
        }
    });

    let state = Arc::new(ProxyState {
        registry,
        s3_clients,
    });

    let router = proxy_router(state);

    let app = if let Some(ref web_dir) = args.web_dir {
        info!("serving web UI from {}", web_dir.display());
        let spa_fallback =
            ServeDir::new(web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
        router.fallback_service(spa_fallback)
    } else {
        router
    };

    let addr = format!("{}:{}", args.bind, args.port);

    info!("binding to {addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            error!("failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    info!("bae-proxy listening on http://{addr}");
    if let Err(e) = axum::serve(listener, app).await {
        error!("server error: {e}");
        std::process::exit(1);
    }
}
