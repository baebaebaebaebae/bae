//! Application context types for passing services through the Dioxus context boundary.
//!
//! Note: The main application service is `AppService` in `app_service.rs`.
//! This file contains the `AppServices` struct for passing backend service handles
//! from main.rs through the launch boundary.

use std::sync::{Arc, RwLock};

use bae_core::cache;
use bae_core::config;
use bae_core::encryption::EncryptionService;
use bae_core::image_server::ImageServerHandle;
use bae_core::import;
use bae_core::keys::{KeyService, UserKeypair};
use bae_core::library::SharedLibraryManager;
use bae_core::playback;
use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
use bae_core::sync::hlc::Hlc;
use bae_core::sync::session::SyncSession;
#[cfg(feature = "torrent")]
use bae_core::torrent;

/// Handle for sync infrastructure, created at startup if sync is configured.
///
/// Holds the S3 bucket client, hybrid logical clock, raw sqlite3 write pointer
/// (for session extension operations), and the active sync session.
///
/// The raw pointer is extracted once from the Database's dedicated write
/// connection. It's stable for the lifetime of the Database because the
/// connection is heap-allocated and never moved.
#[derive(Clone)]
pub struct SyncHandle {
    /// Sync bucket client for pushing/pulling changesets
    pub bucket_client: Arc<CloudHomeSyncBucket>,
    /// Hybrid logical clock for causal ordering of writes
    pub hlc: Arc<Hlc>,
    /// Device ID for this device (used in push, cursors, and SyncService)
    pub device_id: String,
    /// Shared encryption service (same instance as bucket_client's).
    /// Wrapped in Arc<RwLock> so key rotation can update it atomically.
    pub encryption: Arc<RwLock<EncryptionService>>,
    /// Cached raw sqlite3 write connection pointer for session extension ops.
    /// Valid for the lifetime of the Database.
    raw_db: *mut libsqlite3_sys::sqlite3,
    /// The active sync session recording changes to synced tables.
    /// Wrapped in Mutex<Option> so the sync loop can take/replace it.
    pub session: Arc<tokio::sync::Mutex<Option<SyncSession>>>,
    /// Channel sender for triggering a sync cycle from the UI (Phase 5d)
    pub sync_trigger: tokio::sync::mpsc::Sender<()>,
    /// Channel receiver for the sync trigger. The sync loop (Phase 5c) takes
    /// this once via `take_trigger_rx()`.
    sync_trigger_rx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<()>>>>,
}

// SAFETY: The raw sqlite3 pointer is only used for session extension operations
// which are serialized through the sync loop. The pointer itself is stable
// (heap-allocated write connection inside Arc<DatabaseInner>).
unsafe impl Send for SyncHandle {}
unsafe impl Sync for SyncHandle {}

impl SyncHandle {
    pub fn new(
        bucket_client: CloudHomeSyncBucket,
        hlc: Hlc,
        device_id: String,
        raw_db: *mut libsqlite3_sys::sqlite3,
        session: SyncSession,
        sync_trigger: tokio::sync::mpsc::Sender<()>,
        sync_trigger_rx: tokio::sync::mpsc::Receiver<()>,
    ) -> Self {
        // Share the encryption lock with the bucket client so key rotation
        // is visible to both the sync loop and the S3 operations.
        let encryption = bucket_client.shared_encryption();
        SyncHandle {
            bucket_client: Arc::new(bucket_client),
            hlc: Arc::new(hlc),
            device_id,
            encryption,
            raw_db,
            session: Arc::new(tokio::sync::Mutex::new(Some(session))),
            sync_trigger,
            sync_trigger_rx: Arc::new(tokio::sync::Mutex::new(Some(sync_trigger_rx))),
        }
    }

    /// The cached raw sqlite3 write connection pointer.
    pub fn raw_db(&self) -> *mut libsqlite3_sys::sqlite3 {
        self.raw_db
    }

    /// Take the sync trigger receiver. Returns `None` if already taken.
    /// Called once by the sync loop (Phase 5c) to own the receive end.
    pub async fn take_trigger_rx(&self) -> Option<tokio::sync::mpsc::Receiver<()>> {
        self.sync_trigger_rx.lock().await.take()
    }

    /// Replace the encryption key (used after member revocation / key rotation).
    /// This updates the shared lock so both the sync loop and the bucket client
    /// see the new key immediately.
    pub fn update_encryption_key(&self, new_key: [u8; 32]) {
        let mut enc = self.encryption.write().unwrap();
        *enc = EncryptionService::from_key(new_key);
    }
}

/// Service handles provided at app launch (Send + Sync safe).
///
/// These are the backend service handles that can be passed through
/// the context provider boundary. The reactive state is created separately
/// inside the Dioxus component tree by `AppService`.
#[derive(Clone)]
pub struct AppServices {
    /// Library manager for database operations
    pub library_manager: SharedLibraryManager,
    /// Application configuration
    pub config: config::Config,
    /// Import service handle for submitting imports
    pub import_handle: import::ImportServiceHandle,
    /// Playback service handle for audio control
    pub playback_handle: playback::PlaybackHandle,
    /// Cache manager for images/files
    pub cache: cache::CacheManager,
    /// Torrent manager (feature-gated)
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
    /// Key service for secret management
    pub key_service: KeyService,
    /// Image server connection handle
    pub image_server: ImageServerHandle,
    /// User's Ed25519 keypair for signing and key exchange
    pub user_keypair: Option<UserKeypair>,
    /// Sync infrastructure handle (present when sync is configured and encryption is enabled)
    pub sync_handle: Option<SyncHandle>,
}

#[derive(Clone)]
pub struct AppContext {
    pub library_manager: SharedLibraryManager,
    pub config: config::Config,
    pub import_handle: import::ImportServiceHandle,
    pub playback_handle: playback::PlaybackHandle,
    pub cache: cache::CacheManager,
    #[cfg(feature = "torrent")]
    pub torrent_manager: torrent::LazyTorrentManager,
    pub key_service: KeyService,
    pub image_server: ImageServerHandle,
    pub user_keypair: Option<UserKeypair>,
    pub sync_handle: Option<SyncHandle>,
}
