use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use tokio::sync::{watch, Mutex, RwLock};
use tracing::{error, info, warn};

use bae_core::cloud_home::s3::S3CloudHome;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::keys::KeyService;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::library_dir::LibraryDir;
use bae_core::subsonic::create_router;
use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
use bae_core::sync::snapshot::bootstrap_from_snapshot;

use crate::cloud_proxy::{self, ChainState, CloudProxyState};
use crate::registry::LibraryConfig;
use crate::{download_images, open_write_handle, pull_new_changesets, set_wal_mode};

pub struct TenantCache {
    tenants: RwLock<HashMap<String, TenantState>>,
    /// Immutable after construction.
    registry: HashMap<String, LibraryConfig>,
}

enum TenantState {
    Loading,
    Ready(ReadyTenant),
}

struct ReadyTenant {
    router: Router,
    last_access: Mutex<Instant>,
    cache_timeout: Option<Duration>,
    /// Send to signal the sync loop to stop.
    sync_shutdown: watch::Sender<()>,
    /// Library path for cleanup on eviction.
    library_path: String,
}

pub enum TenantResolveResult {
    Ready(Router),
    Loading,
    NotFound,
}

/// Result of a successful bootstrap, sent from the bootstrap thread back
/// to the async task that inserts it into the tenant cache.
struct BootstrapResult {
    router: Router,
    sync_shutdown: watch::Sender<()>,
    cache_timeout: Option<Duration>,
    library_path: String,
}

impl TenantCache {
    pub fn new(registry: HashMap<String, LibraryConfig>) -> Self {
        Self {
            tenants: RwLock::new(HashMap::new()),
            registry,
        }
    }

    pub fn registered_count(&self) -> usize {
        self.registry.len()
    }

    pub async fn loaded_count(&self) -> usize {
        let tenants = self.tenants.read().await;
        tenants
            .values()
            .filter(|s| matches!(s, TenantState::Ready(_)))
            .count()
    }

    /// Look up a tenant by hostname. Returns the router if ready,
    /// starts bootstrap if unknown, or indicates loading.
    pub async fn resolve(self: &Arc<Self>, hostname: &str) -> TenantResolveResult {
        // Fast path: check if already loaded.
        {
            let tenants = self.tenants.read().await;
            match tenants.get(hostname) {
                Some(TenantState::Ready(ready)) => {
                    *ready.last_access.lock().await = Instant::now();
                    return TenantResolveResult::Ready(ready.router.clone());
                }
                Some(TenantState::Loading) => {
                    return TenantResolveResult::Loading;
                }
                None => {}
            }
        }

        // Not in cache -- check registry.
        let config = match self.registry.get(hostname) {
            Some(config) => config.clone(),
            None => return TenantResolveResult::NotFound,
        };

        // Mark as loading (under write lock, re-check to avoid double bootstrap).
        {
            let mut tenants = self.tenants.write().await;
            if tenants.contains_key(hostname) {
                // Another request got here first.
                return TenantResolveResult::Loading;
            }
            tenants.insert(hostname.to_string(), TenantState::Loading);
        }

        // Spawn bootstrap on a dedicated OS thread. The bootstrap involves
        // raw sqlite3 pointers (not Send), so it needs its own single-threaded
        // runtime. An async task waits for the result and updates the cache.
        let cache = self.clone();
        let hostname_owned = hostname.to_string();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build bootstrap runtime");
            rt.block_on(bootstrap_tenant_inner(&hostname_owned, &config, result_tx));
        });

        // Wait for the bootstrap result in a tokio task (doesn't block the request).
        let hostname_for_task = hostname.to_string();
        tokio::spawn(async move {
            match result_rx.await {
                Ok(Some(result)) => {
                    let ready = ReadyTenant {
                        router: result.router,
                        last_access: Mutex::new(Instant::now()),
                        cache_timeout: result.cache_timeout,
                        sync_shutdown: result.sync_shutdown,
                        library_path: result.library_path,
                    };
                    cache
                        .tenants
                        .write()
                        .await
                        .insert(hostname_for_task.clone(), TenantState::Ready(ready));

                    info!("Tenant {hostname_for_task}: ready");
                }
                Ok(None) => {
                    // Bootstrap failed; remove Loading entry.
                    cache.tenants.write().await.remove(&hostname_for_task);
                }
                Err(_) => {
                    // Bootstrap thread panicked.
                    error!("Tenant {hostname_for_task}: bootstrap thread panicked");
                    cache.tenants.write().await.remove(&hostname_for_task);
                }
            }
        });

        TenantResolveResult::Loading
    }

    /// Evict idle tenants. Called by the reaper.
    pub async fn evict_idle(&self) {
        let mut tenants = self.tenants.write().await;
        let now = Instant::now();

        let to_evict: Vec<String> = tenants
            .iter()
            .filter_map(|(hostname, state)| {
                if let TenantState::Ready(ready) = state {
                    if let Some(timeout) = ready.cache_timeout {
                        // try_lock to avoid blocking; skip if contested.
                        if let Ok(last) = ready.last_access.try_lock() {
                            if now.duration_since(*last) > timeout {
                                return Some(hostname.clone());
                            }
                        }
                    }
                }
                None
            })
            .collect();

        for hostname in to_evict {
            if let Some(TenantState::Ready(ready)) = tenants.remove(&hostname) {
                // Signal sync loop to stop.
                let _ = ready.sync_shutdown.send(());

                info!("Evicting idle tenant: {hostname}");

                // Clean up cached files in a background task.
                let library_path = ready.library_path.clone();
                tokio::spawn(async move {
                    if let Err(e) = tokio::fs::remove_dir_all(&library_path).await {
                        warn!(
                            "Failed to clean up evicted tenant files at {}: {e}",
                            library_path
                        );
                    }
                });
            }
        }
    }
}

/// Run the full bootstrap sequence for a single tenant.
///
/// Runs on a dedicated OS thread with a single-threaded tokio runtime so
/// the raw sqlite3 pointer used during changeset application doesn't need
/// to be Send. Sends the result back via a oneshot channel.
async fn bootstrap_tenant_inner(
    hostname: &str,
    config: &LibraryConfig,
    result_tx: tokio::sync::oneshot::Sender<Option<BootstrapResult>>,
) {
    info!("Bootstrapping tenant: {hostname}");

    let encryption = match EncryptionService::new(&config.recovery_key) {
        Ok(enc) => enc,
        Err(e) => {
            error!("Tenant {hostname}: invalid recovery key: {e}");
            let _ = result_tx.send(None);
            return;
        }
    };

    let library_dir = LibraryDir::new(&config.library_path);

    if let Err(e) = std::fs::create_dir_all(library_dir.as_ref()) {
        error!(
            "Tenant {hostname}: failed to create directory {}: {e}",
            config.library_path
        );
        let _ = result_tx.send(None);
        return;
    }

    // Create S3CloudHome for the sync bucket.
    let cloud_home_for_bucket = match S3CloudHome::new(
        config.s3_bucket.clone(),
        config.s3_region.clone(),
        config.s3_endpoint.clone(),
        config.s3_access_key.clone(),
        config.s3_secret_key.clone(),
        config.s3_key_prefix.clone(),
    )
    .await
    {
        Ok(ch) => ch,
        Err(e) => {
            error!("Tenant {hostname}: failed to connect to S3: {e}");
            let _ = result_tx.send(None);
            return;
        }
    };

    // Separate S3CloudHome for the write proxy (raw, not encrypted).
    let cloud_home_for_proxy: Arc<dyn bae_core::cloud_home::CloudHome> = match S3CloudHome::new(
        config.s3_bucket.clone(),
        config.s3_region.clone(),
        config.s3_endpoint.clone(),
        config.s3_access_key.clone(),
        config.s3_secret_key.clone(),
        config.s3_key_prefix.clone(),
    )
    .await
    {
        Ok(ch) => Arc::new(ch),
        Err(e) => {
            error!("Tenant {hostname}: failed to connect to S3 (proxy): {e}");
            let _ = result_tx.send(None);
            return;
        }
    };

    let bucket = CloudHomeSyncBucket::new(Box::new(cloud_home_for_bucket), encryption.clone());

    // Bootstrap from snapshot.
    let db_path = library_dir.db_path();

    info!("Tenant {hostname}: downloading snapshot...");

    let bootstrap_result = match bootstrap_from_snapshot(&bucket, &encryption, &db_path).await {
        Ok(r) => r,
        Err(e) => {
            error!("Tenant {hostname}: failed to bootstrap from snapshot: {e}");
            let _ = result_tx.send(None);
            return;
        }
    };

    info!(
        "Tenant {hostname}: snapshot restored ({} device cursors)",
        bootstrap_result.cursors.len()
    );

    // Open write handle and set WAL mode.
    let write_handle = open_write_handle(&db_path);
    set_wal_mode(write_handle.0);

    // Pull changesets since the snapshot.
    let (cursors, initial_applied) = pull_new_changesets(
        write_handle.0,
        &bucket,
        &bootstrap_result.cursors,
        &library_dir,
    )
    .await;

    if initial_applied > 0 {
        info!("Tenant {hostname}: applied {initial_applied} changesets since snapshot");
    }

    // Download images.
    download_images(&bucket, &library_dir).await;

    // Load membership chain for write proxy auth.
    let chain_state = match cloud_proxy::load_membership_chain(&bucket).await {
        Ok(Some(chain)) => {
            info!("Tenant {hostname}: membership chain loaded");
            ChainState::Valid(chain)
        }
        Ok(None) => {
            info!("Tenant {hostname}: no membership chain found");
            ChainState::None
        }
        Err(reason) => {
            warn!("Tenant {hostname}: membership chain corrupt: {reason}");
            ChainState::Invalid
        }
    };

    let cloud_proxy_state = CloudProxyState {
        cloud_home: cloud_home_for_proxy,
        chain_state,
    };

    // Open DB read-only for serving.
    let database = match Database::open_read_only(db_path.to_str().unwrap()).await {
        Ok(db) => db,
        Err(e) => {
            error!("Tenant {hostname}: failed to open database: {e}");
            let _ = result_tx.send(None);
            return;
        }
    };

    let key_service = KeyService::new(true, config.library_id.clone());
    let library_manager =
        SharedLibraryManager::new(LibraryManager::new(database, Some(encryption.clone())));

    let auth = bae_core::subsonic::SubsonicAuth {
        enabled: false,
        username: None,
        password: None,
    };

    let api_router = create_router(
        library_manager,
        Some(encryption),
        library_dir.clone(),
        key_service,
        None, // share_base_url
        1,    // share_signing_key_version
        auth,
    );

    let cloud_router = cloud_proxy::cloud_proxy_router(cloud_proxy_state);
    let router = Router::new().merge(api_router).merge(cloud_router);

    // Spawn sync loop on a SECOND dedicated OS thread. The write_handle
    // stays on the sync thread for its entire lifetime.
    let (shutdown_tx, shutdown_rx) = watch::channel(());
    let sync_bucket = Arc::new(bucket);
    let sync_library_dir = library_dir;
    let tenant_hostname = hostname.to_string();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build sync runtime");
        rt.block_on(tenant_sync_loop(
            sync_bucket,
            write_handle,
            sync_library_dir,
            30, // sync interval
            cursors,
            shutdown_rx,
            tenant_hostname,
        ));
    });

    let cache_timeout = config.cache_timeout_secs.map(Duration::from_secs);

    let _ = result_tx.send(Some(BootstrapResult {
        router,
        sync_shutdown: shutdown_tx,
        cache_timeout,
        library_path: config.library_path.clone(),
    }));
}

/// Background sync loop for a single tenant.
///
/// Same pattern as the single-tenant sync_loop, but with a shutdown signal.
async fn tenant_sync_loop(
    bucket: Arc<CloudHomeSyncBucket>,
    write_handle: crate::SqliteWriteHandle,
    library_dir: LibraryDir,
    interval_secs: u64,
    initial_cursors: HashMap<String, u64>,
    mut shutdown: watch::Receiver<()>,
    hostname: String,
) {
    let mut cursors = initial_cursors;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval_secs)) => {}
            _ = shutdown.changed() => {
                info!("Tenant {hostname}: sync loop shutting down");
                return;
            }
        }

        let (updated_cursors, applied) =
            pull_new_changesets(write_handle.0, &bucket, &cursors, &library_dir).await;

        cursors = updated_cursors;

        if applied > 0 {
            info!("Tenant {hostname}: sync applied {applied} changesets");
        }

        download_images(&bucket, &library_dir).await;
    }
}
