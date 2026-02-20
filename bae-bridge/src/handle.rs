use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bae_core::cloud_home::CloudHome;
use bae_core::config::Config;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::image_server::{self, ImageServerHandle};
use bae_core::import::{ImportProgress, ImportService, ImportServiceHandle, ScanEvent};
use bae_core::keys::{KeyService, UserKeypair};
use bae_core::library::SharedLibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::playback::{PlaybackHandle, PlaybackProgress, PlaybackService, PlaybackState};
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;
use bae_core::sync::hlc::Hlc;
use bae_core::sync::service::SyncService;
use bae_core::sync::session::SyncSession;
use tracing::{info, warn};

use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtist,
    BridgeArtistSearchResult, BridgeAudioContent, BridgeCandidateFiles, BridgeConfig,
    BridgeCoverArt, BridgeCoverSelection, BridgeCueFlacPair, BridgeDiscIdResult, BridgeError,
    BridgeFile, BridgeFileInfo, BridgeFollowedLibrary, BridgeImportCandidate, BridgeImportStatus,
    BridgeLibraryInfo, BridgeMember, BridgeMetadataResult, BridgePlaybackState, BridgeQueueItem,
    BridgeRelease, BridgeReleaseDetail, BridgeReleaseTrack, BridgeRemoteCover, BridgeRepeatMode,
    BridgeSaveSyncConfig, BridgeSearchResults, BridgeSortCriterion, BridgeSortDirection,
    BridgeSortField, BridgeSyncConfig, BridgeSyncStatus, BridgeTrack, BridgeTrackSearchResult,
};

fn bridge_sort_to_core(c: &BridgeSortCriterion) -> bae_core::db::AlbumSortCriterion {
    bae_core::db::AlbumSortCriterion {
        field: match c.field {
            BridgeSortField::Title => bae_core::db::AlbumSortField::Title,
            BridgeSortField::Artist => bae_core::db::AlbumSortField::Artist,
            BridgeSortField::Year => bae_core::db::AlbumSortField::Year,
            BridgeSortField::DateAdded => bae_core::db::AlbumSortField::DateAdded,
        },
        direction: match c.direction {
            BridgeSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
            BridgeSortDirection::Descending => bae_core::db::SortDirection::Descending,
        },
    }
}

/// Check Cover Art Archive for cover art thumbnails.
/// Does HEAD requests to CAA; returns redirect Location URLs (250px thumbnails).
async fn check_cover_art(release_ids: &[String]) -> Vec<Option<String>> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let futures: Vec<_> = release_ids
        .iter()
        .map(|id| {
            let client = &client;
            let url = format!("https://coverartarchive.org/release/{id}/front-250");
            async move {
                match client.head(&url).send().await {
                    Ok(resp) if resp.status() == reqwest::StatusCode::TEMPORARY_REDIRECT => resp
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                    _ => None,
                }
            }
        })
        .collect();

    futures::future::join_all(futures).await
}

/// Discover all libraries in ~/.bae/libraries/.
#[uniffi::export]
pub fn discover_libraries() -> Result<Vec<BridgeLibraryInfo>, BridgeError> {
    let libraries = Config::discover_libraries();
    Ok(libraries
        .into_iter()
        .map(|lib| BridgeLibraryInfo {
            id: lib.id,
            name: lib.name,
            path: lib.path.to_string_lossy().to_string(),
        })
        .collect())
}

/// Create a new library with an optional name.
/// Returns the new library's info. The library is set as the active library.
#[uniffi::export]
pub fn create_library(name: Option<String>) -> Result<BridgeLibraryInfo, BridgeError> {
    let dev_mode = Config::is_dev_mode();
    let mut config = Config::create_new_library(dev_mode).map_err(|e| BridgeError::Config {
        msg: format!("{e}"),
    })?;

    if let Some(ref n) = name {
        config.library_name = Some(n.clone());
        config.save().map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })?;
    }

    config
        .save_active_library()
        .map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })?;

    Ok(BridgeLibraryInfo {
        id: config.library_id,
        name: config.library_name,
        path: config.library_dir.to_string_lossy().to_string(),
    })
}

/// Restore a library from a cloud S3 backup.
///
/// Downloads and decrypts the manifest, database, and images from the given
/// S3 bucket using the provided encryption key. Creates the local library
/// directory, writes config.yaml, saves credentials to the keyring, and
/// sets this library as the active one.
#[uniffi::export]
pub fn restore_from_cloud(
    library_id: String,
    bucket: String,
    region: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
    encryption_key_hex: String,
) -> Result<BridgeLibraryInfo, BridgeError> {
    // Validate required fields
    if library_id.is_empty()
        || bucket.is_empty()
        || region.is_empty()
        || access_key.is_empty()
        || secret_key.is_empty()
        || encryption_key_hex.is_empty()
    {
        return Err(BridgeError::Config {
            msg: "All fields except endpoint are required".to_string(),
        });
    }

    // Validate hex key
    if encryption_key_hex.len() != 64 {
        return Err(BridgeError::Config {
            msg: "Encryption key must be 64 hex characters (32 bytes)".to_string(),
        });
    }
    if hex::decode(&encryption_key_hex).is_err() {
        return Err(BridgeError::Config {
            msg: "Invalid hex encoding in encryption key".to_string(),
        });
    }

    let runtime = tokio::runtime::Runtime::new().map_err(|e| BridgeError::Internal {
        msg: format!("Failed to create runtime: {e}"),
    })?;

    runtime.block_on(async {
        use bae_core::cloud_storage::{CloudStorage, S3CloudStorage, S3Config};

        let encryption_service =
            EncryptionService::new(&encryption_key_hex).map_err(|e| BridgeError::Config {
                msg: format!("Invalid encryption key: {e}"),
            })?;
        let fingerprint = encryption_service.fingerprint();

        let endpoint_opt = if endpoint.is_empty() {
            None
        } else {
            Some(endpoint.clone())
        };

        let s3_config = S3Config {
            bucket_name: bucket.clone(),
            region: region.clone(),
            access_key_id: access_key.clone(),
            secret_access_key: secret_key.clone(),
            endpoint_url: endpoint_opt.clone(),
        };
        let storage = S3CloudStorage::new_with_bucket_creation(s3_config, false)
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to connect to S3: {e}"),
            })?;

        // Download and decrypt manifest to validate the key
        info!("Downloading manifest from cloud...");
        let encrypted_manifest = storage
            .download(&format!("s3://{}/manifest.json.enc", bucket))
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to download manifest: {e}"),
            })?;
        let manifest_bytes = encryption_service
            .decrypt(&encrypted_manifest)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to decrypt manifest (wrong key?): {e}"),
            })?;
        let manifest: bae_core::library_dir::Manifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to parse manifest: {e}"),
            })?;

        // Validate fingerprint
        if let Some(ref expected_fp) = manifest.encryption_key_fingerprint {
            if *expected_fp != fingerprint {
                return Err(BridgeError::Config {
                    msg: format!(
                        "Encryption key fingerprint mismatch: expected {}, got {}",
                        expected_fp, fingerprint
                    ),
                });
            }
        }

        info!("Key validated, downloading library...");

        // Set up local library directory
        let home_dir = dirs::home_dir().ok_or_else(|| BridgeError::Config {
            msg: "Failed to get home directory".to_string(),
        })?;
        let bae_dir = home_dir.join(".bae");
        let library_dir =
            bae_core::library_dir::LibraryDir::new(bae_dir.join("libraries").join(&library_id));
        std::fs::create_dir_all(&*library_dir).map_err(|e| BridgeError::Internal {
            msg: format!("Failed to create library directory: {e}"),
        })?;

        // Download and decrypt DB
        let encrypted_db = storage
            .download(&format!("s3://{}/library.db.enc", bucket))
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to download database: {e}"),
            })?;
        let decrypted_db =
            encryption_service
                .decrypt(&encrypted_db)
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Failed to decrypt database: {e}"),
                })?;
        let db_path = library_dir.db_path();
        tokio::fs::write(&db_path, &decrypted_db)
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to write database: {e}"),
            })?;

        info!("Restored DB ({} bytes)", decrypted_db.len());

        // Download and decrypt images
        let images_dir = library_dir.images_dir();
        tokio::fs::create_dir_all(&images_dir)
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to create images directory: {e}"),
            })?;

        let image_keys = storage
            .list_keys("images/")
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to list images: {e}"),
            })?;

        info!("Found {} image(s) to download", image_keys.len());

        for key in &image_keys {
            let rel = key.strip_prefix("images/").unwrap_or(key);
            let target_path = images_dir.join(rel);

            if let Some(parent) = target_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to create image directory: {e}"),
                    })?;
            }

            let location = format!("s3://{}/{}", bucket, key);
            let encrypted_data =
                storage
                    .download(&location)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to download image {key}: {e}"),
                    })?;
            let decrypted_data =
                encryption_service
                    .decrypt(&encrypted_data)
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to decrypt image {key}: {e}"),
                    })?;
            tokio::fs::write(&target_path, &decrypted_data)
                .await
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Failed to write image: {e}"),
                })?;
        }

        info!(
            "Downloaded {} image(s) to {}",
            image_keys.len(),
            images_dir.display()
        );

        // Write local manifest.json
        let home_manifest = bae_core::library_dir::Manifest {
            library_id: library_id.clone(),
            library_name: manifest.library_name.clone(),
            encryption_key_fingerprint: Some(fingerprint.clone()),
        };
        let manifest_json =
            serde_json::to_string_pretty(&home_manifest).map_err(|e| BridgeError::Internal {
                msg: format!("Failed to serialize manifest: {e}"),
            })?;
        tokio::fs::write(library_dir.manifest_path(), manifest_json)
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to write manifest: {e}"),
            })?;

        // Write config.yaml
        let config = Config {
            library_id: library_id.clone(),
            device_id: uuid::Uuid::new_v4().to_string(),
            library_dir: library_dir.clone(),
            library_name: manifest.library_name.clone(),
            keys_migrated: true,
            discogs_key_stored: false,
            encryption_key_stored: true,
            encryption_key_fingerprint: Some(fingerprint.clone()),
            torrent_bind_interface: None,
            torrent_listen_port: None,
            torrent_enable_upnp: false,
            torrent_enable_natpmp: false,
            torrent_enable_dht: false,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            network_participation: bae_core::sync::participation::ParticipationMode::Off,
            server_enabled: false,
            server_port: 4533,
            server_bind_address: "127.0.0.1".to_string(),
            server_auth_enabled: false,
            server_username: None,
            cloud_provider: None,
            cloud_home_s3_bucket: None,
            cloud_home_s3_region: None,
            cloud_home_s3_endpoint: None,
            cloud_home_s3_key_prefix: None,
            cloud_home_google_drive_folder_id: None,
            cloud_home_dropbox_folder_path: None,
            cloud_home_onedrive_drive_id: None,
            cloud_home_onedrive_folder_id: None,
            cloud_home_icloud_container_path: None,
            cloud_home_bae_cloud_url: None,
            cloud_home_bae_cloud_username: None,
            share_base_url: None,
            followed_libraries: vec![],
        };
        config
            .save_to_config_yaml()
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to write config: {e}"),
            })?;

        // Save secrets to keyring
        let dev_mode = Config::is_dev_mode();
        let key_service = KeyService::new(dev_mode, library_id.clone());
        key_service
            .set_encryption_key(&encryption_key_hex)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to save encryption key: {e}"),
            })?;

        let creds = bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        };
        key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to save S3 credentials: {e}"),
            })?;

        info!("Saved credentials to keyring");

        // Write pointer file last (makes this idempotent on failure)
        config
            .save_active_library()
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to write active library pointer: {e}"),
            })?;

        info!(
            "Cloud restore complete: library at {}",
            library_dir.display()
        );

        Ok(BridgeLibraryInfo {
            id: library_id,
            name: manifest.library_name,
            path: library_dir.to_string_lossy().to_string(),
        })
    })
}

/// Unlock a library by providing the encryption key hex.
/// Validates the key against the stored fingerprint, then saves it to the keyring.
#[uniffi::export]
pub fn unlock_library(library_id: String, key_hex: String) -> Result<(), BridgeError> {
    // Validate hex encoding: must be 64 hex chars = 32 bytes
    if key_hex.len() != 64 {
        return Err(BridgeError::Config {
            msg: "Encryption key must be 64 hex characters (32 bytes)".to_string(),
        });
    }
    if hex::decode(&key_hex).is_err() {
        return Err(BridgeError::Config {
            msg: "Invalid hex encoding".to_string(),
        });
    }

    // Compute fingerprint and compare to stored one
    let fingerprint = bae_core::encryption::compute_key_fingerprint(&key_hex).ok_or_else(|| {
        BridgeError::Config {
            msg: "Failed to compute key fingerprint".to_string(),
        }
    })?;

    // Load config for this library to get the stored fingerprint
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| BridgeError::NotFound {
            msg: format!("Library '{library_id}' not found"),
        })?;

    // Read and parse config.yaml to get the stored fingerprint
    let config_path = lib_info.path.join("config.yaml");
    let config_str = std::fs::read_to_string(&config_path).map_err(|e| BridgeError::Config {
        msg: format!("Failed to read config: {e}"),
    })?;
    let yaml_config: bae_core::config::ConfigYaml =
        serde_yaml::from_str(&config_str).map_err(|e| BridgeError::Config {
            msg: format!("Failed to parse config: {e}"),
        })?;

    if let Some(ref stored_fp) = yaml_config.encryption_key_fingerprint {
        if *stored_fp != fingerprint {
            return Err(BridgeError::Config {
                msg: "Encryption key fingerprint mismatch".to_string(),
            });
        }
    }

    // Save key to keyring
    let dev_mode = Config::is_dev_mode();
    let key_service = KeyService::new(dev_mode, library_id);
    key_service
        .set_encryption_key(&key_hex)
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to save encryption key: {e}"),
        })?;

    Ok(())
}

/// Callback interface for playback and import events. Implemented by Swift.
#[uniffi::export(callback_interface)]
pub trait AppEventHandler: Send + Sync {
    fn on_playback_state_changed(&self, state: BridgePlaybackState);
    fn on_playback_progress(&self, position_ms: u64, duration_ms: u64, track_id: String);
    fn on_queue_updated(&self, track_ids: Vec<String>);
    fn on_scan_result(&self, candidate: BridgeImportCandidate);
    fn on_import_progress(&self, folder_path: String, status: BridgeImportStatus);
    fn on_library_changed(&self);
    fn on_sync_status_changed(&self, status: BridgeSyncStatus);
    fn on_error(&self, message: String);
}

/// Sync infrastructure handle for the bridge.
///
/// Mirrors bae-desktop's SyncHandle but without Dioxus store dependencies.
/// Holds everything needed to run sync cycles: bucket client, HLC, raw sqlite3
/// pointer, active sync session, and a user keypair for signing changesets.
struct BridgeSyncHandle {
    bucket_client: Arc<CloudHomeSyncBucket>,
    hlc: Arc<Hlc>,
    device_id: String,
    encryption: Arc<RwLock<EncryptionService>>,
    raw_db: *mut libsqlite3_sys::sqlite3,
    session: tokio::sync::Mutex<Option<SyncSession>>,
    user_keypair: UserKeypair,
}

// SAFETY: The raw sqlite3 pointer is only used for session extension operations
// which are serialized through the sync loop. The pointer itself is stable
// (heap-allocated write connection inside Arc<DatabaseInner>).
unsafe impl Send for BridgeSyncHandle {}
unsafe impl Sync for BridgeSyncHandle {}

/// The central handle to the bae backend. Owns the tokio runtime and all services.
#[derive(uniffi::Object)]
pub struct AppHandle {
    runtime: tokio::runtime::Runtime,
    config: Config,
    library_manager: SharedLibraryManager,
    key_service: KeyService,
    encryption_service: Option<EncryptionService>,
    cloud_home: Option<Arc<dyn CloudHome>>,
    image_server: ImageServerHandle,
    playback_handle: PlaybackHandle,
    import_handle: ImportServiceHandle,
    event_handler: Mutex<Option<Arc<dyn AppEventHandler>>>,
    subsonic_server_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    sync_handle: Option<Arc<BridgeSyncHandle>>,
    sync_loop_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Maps import_id (UUID) → folder_path so progress events can be keyed by folder.
    import_id_to_folder: Arc<Mutex<HashMap<String, String>>>,
}

#[uniffi::export]
impl AppHandle {
    /// Get the library ID.
    pub fn library_id(&self) -> String {
        self.config.library_id.clone()
    }

    /// Get the library name.
    pub fn library_name(&self) -> Option<String> {
        self.config.library_name.clone()
    }

    /// Get the library path.
    pub fn library_path(&self) -> String {
        self.config.library_dir.to_string_lossy().to_string()
    }

    /// Whether this library has encryption configured.
    pub fn is_encrypted(&self) -> bool {
        self.config.encryption_key_stored
    }

    /// Get the encryption key fingerprint, if available.
    pub fn get_encryption_fingerprint(&self) -> Option<String> {
        self.config.encryption_key_fingerprint.clone()
    }

    /// Check if the encryption key is available in the keyring.
    /// Returns false if the key is configured but not present (needs unlock).
    pub fn check_encryption_key_available(&self) -> bool {
        self.key_service.get_encryption_key().is_some()
    }

    /// Get all albums with their artist names, sorted by the given criteria.
    ///
    /// Pass an empty vec for default sort (newest first).
    pub fn get_albums(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
    ) -> Result<Vec<BridgeAlbum>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let sort: Vec<bae_core::db::AlbumSortCriterion> =
                sort_criteria.iter().map(bridge_sort_to_core).collect();
            let albums = lm
                .get_albums(&sort)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;

            let mut result = Vec::with_capacity(albums.len());
            for album in &albums {
                let artist_names = match lm.get_artists_for_album(&album.id).await {
                    Ok(artists) => artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(_) => String::new(),
                };
                result.push(BridgeAlbum {
                    id: album.id.clone(),
                    title: album.title.clone(),
                    year: album.year,
                    is_compilation: album.is_compilation,
                    cover_release_id: album.cover_release_id.clone(),
                    artist_names,
                });
            }

            Ok(result)
        })
    }

    /// Get all artists that have at least one album.
    pub fn get_artists(&self) -> Result<Vec<BridgeArtist>, BridgeError> {
        self.runtime.block_on(async {
            let artists = self
                .library_manager
                .get()
                .get_artists_with_albums()
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;

            Ok(artists
                .into_iter()
                .map(|a| BridgeArtist {
                    id: a.id,
                    name: a.name,
                })
                .collect())
        })
    }

    /// Get albums for a specific artist, sorted by year descending.
    pub fn get_artist_albums(&self, artist_id: String) -> Result<Vec<BridgeAlbum>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let albums =
                lm.get_albums_for_artist(&artist_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let mut result = Vec::with_capacity(albums.len());
            for album in &albums {
                let artist_names = match lm.get_artists_for_album(&album.id).await {
                    Ok(artists) => artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(_) => String::new(),
                };
                result.push(BridgeAlbum {
                    id: album.id.clone(),
                    title: album.title.clone(),
                    year: album.year,
                    is_compilation: album.is_compilation,
                    cover_release_id: album.cover_release_id.clone(),
                    artist_names,
                });
            }

            Ok(result)
        })
    }

    /// Get the image URL for an image, if the image file exists on disk.
    /// Returns nil if no image file is present.
    pub fn get_image_url(&self, image_id: String) -> Option<String> {
        self.image_server.image_url_if_exists(&image_id)
    }

    /// Get a signed URL for serving a release file by its ID.
    pub fn get_file_url(&self, file_id: String) -> String {
        self.image_server.file_url(&file_id)
    }

    /// Get full album detail including releases, tracks, files, and artists.
    pub fn get_album_detail(&self, album_id: String) -> Result<BridgeAlbumDetail, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();

            let album = lm
                .get_album_by_id(&album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Album '{album_id}' not found"),
                })?;

            let artists =
                lm.get_artists_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let album_artist_names = artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            let db_releases =
                lm.get_releases_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            let mut releases = Vec::with_capacity(db_releases.len());
            for rel in &db_releases {
                let db_tracks =
                    lm.get_tracks(&rel.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;

                let mut tracks = Vec::with_capacity(db_tracks.len());
                for t in &db_tracks {
                    let track_artists = lm.get_artists_for_track(&t.id).await.map_err(|e| {
                        BridgeError::Database {
                            msg: format!("{e}"),
                        }
                    })?;

                    let artist_names = if track_artists.is_empty() {
                        album_artist_names.clone()
                    } else {
                        track_artists
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };

                    tracks.push(BridgeTrack {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        disc_number: t.disc_number,
                        track_number: t.track_number,
                        duration_ms: t.duration_ms,
                        artist_names,
                    });
                }

                let db_files =
                    lm.get_files_for_release(&rel.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;

                let files = db_files
                    .into_iter()
                    .map(|f| BridgeFile {
                        id: f.id,
                        original_filename: f.original_filename,
                        file_size: f.file_size,
                        content_type: f.content_type.to_string(),
                    })
                    .collect();

                releases.push(BridgeRelease {
                    id: rel.id.clone(),
                    album_id: rel.album_id.clone(),
                    release_name: rel.release_name.clone(),
                    year: rel.year,
                    format: rel.format.clone(),
                    label: rel.label.clone(),
                    catalog_number: rel.catalog_number.clone(),
                    country: rel.country.clone(),
                    managed_locally: rel.managed_locally,
                    managed_in_cloud: rel.managed_in_cloud,
                    unmanaged_path: rel.unmanaged_path.clone(),
                    tracks,
                    files,
                });
            }

            let bridge_album = BridgeAlbum {
                id: album.id,
                title: album.title,
                year: album.year,
                is_compilation: album.is_compilation,
                cover_release_id: album.cover_release_id,
                artist_names: album_artist_names,
            };

            let bridge_artists = artists
                .into_iter()
                .map(|a| BridgeArtist {
                    id: a.id,
                    name: a.name,
                })
                .collect();

            Ok(BridgeAlbumDetail {
                album: bridge_album,
                artists: bridge_artists,
                releases,
            })
        })
    }

    /// Search across artists, albums, and tracks.
    pub fn search(&self, query: String) -> Result<BridgeSearchResults, BridgeError> {
        self.runtime.block_on(async {
            let results = self
                .library_manager
                .get()
                .search_library(&query, 20)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;

            Ok(BridgeSearchResults {
                artists: results
                    .artists
                    .into_iter()
                    .map(|a| BridgeArtistSearchResult {
                        id: a.id,
                        name: a.name,
                        album_count: a.album_count,
                    })
                    .collect(),
                albums: results
                    .albums
                    .into_iter()
                    .map(|a| BridgeAlbumSearchResult {
                        id: a.id,
                        title: a.title,
                        year: a.year,
                        cover_release_id: a.cover_release_id,
                        artist_name: a.artist_name,
                    })
                    .collect(),
                tracks: results
                    .tracks
                    .into_iter()
                    .map(|t| BridgeTrackSearchResult {
                        id: t.id,
                        title: t.title,
                        duration_ms: t.duration_ms,
                        album_id: t.album_id,
                        album_title: t.album_title,
                        artist_name: t.artist_name,
                    })
                    .collect(),
            })
        })
    }

    /// Play an entire album, optionally starting from a specific track index.
    /// When shuffle is true, tracks are played in random order and start_track_index is ignored.
    pub fn play_album(
        &self,
        album_id: String,
        start_track_index: Option<u32>,
        shuffle: bool,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let releases =
                lm.get_releases_for_album(&album_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            // Collect all tracks across releases, sorted by disc then track number
            let mut all_tracks = Vec::new();
            for rel in &releases {
                let tracks = lm
                    .get_tracks(&rel.id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;
                all_tracks.extend(tracks);
            }

            all_tracks.sort_by(|a, b| {
                let disc_a = a.disc_number.unwrap_or(1);
                let disc_b = b.disc_number.unwrap_or(1);
                disc_a.cmp(&disc_b).then_with(|| {
                    a.track_number
                        .unwrap_or(0)
                        .cmp(&b.track_number.unwrap_or(0))
                })
            });

            let mut track_ids: Vec<String> = all_tracks.into_iter().map(|t| t.id).collect();
            if track_ids.is_empty() {
                return Err(BridgeError::NotFound {
                    msg: format!("No tracks found for album '{album_id}'"),
                });
            }

            if shuffle {
                use rand::seq::SliceRandom;
                let mut rng = rand::rng();
                track_ids.shuffle(&mut rng);
            } else if let Some(idx) = start_track_index {
                // Rotate so the start track is first
                let idx = idx as usize;
                if idx < track_ids.len() {
                    track_ids.rotate_left(idx);
                }
            }

            self.playback_handle.play_album(track_ids);
            Ok(())
        })
    }

    /// Play a list of tracks in order.
    pub fn play_tracks(&self, track_ids: Vec<String>) {
        self.playback_handle.play_album(track_ids);
    }

    pub fn pause(&self) {
        self.playback_handle.pause();
    }

    pub fn resume(&self) {
        self.playback_handle.resume();
    }

    pub fn stop(&self) {
        self.playback_handle.stop();
    }

    pub fn next_track(&self) {
        self.playback_handle.next();
    }

    pub fn previous_track(&self) {
        self.playback_handle.previous();
    }

    /// Seek to a position in the current track, in milliseconds.
    pub fn seek(&self, position_ms: u64) {
        self.playback_handle
            .seek(Duration::from_millis(position_ms));
    }

    pub fn set_volume(&self, volume: f32) {
        self.playback_handle.set_volume(volume);
    }

    pub fn set_repeat_mode(&self, mode: BridgeRepeatMode) {
        let core_mode = match mode {
            BridgeRepeatMode::None => bae_core::playback::RepeatMode::None,
            BridgeRepeatMode::Track => bae_core::playback::RepeatMode::Track,
            BridgeRepeatMode::Album => bae_core::playback::RepeatMode::Album,
        };
        self.playback_handle.set_repeat_mode(core_mode);
    }

    // MARK: - Queue

    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        self.playback_handle.add_to_queue(track_ids);
    }

    pub fn add_next(&self, track_ids: Vec<String>) {
        self.playback_handle.add_next(track_ids);
    }

    pub fn remove_from_queue(&self, index: u32) {
        self.playback_handle.remove_from_queue(index as usize);
    }

    pub fn reorder_queue(&self, from_index: u32, to_index: u32) {
        self.playback_handle
            .reorder_queue(from_index as usize, to_index as usize);
    }

    pub fn clear_queue(&self) {
        self.playback_handle.clear_queue();
    }

    pub fn skip_to_queue_index(&self, index: u32) {
        self.playback_handle.skip_to(index as usize);
    }

    /// Enrich a list of track IDs with metadata for queue display.
    /// The Swift side passes track IDs received from on_queue_updated.
    pub fn get_queue_items(&self, track_ids: Vec<String>) -> Vec<BridgeQueueItem> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let mut items = Vec::new();

            for track_id in &track_ids {
                let track = match lm.get_track(track_id).await {
                    Ok(Some(t)) => t,
                    _ => continue,
                };

                let artists = lm.get_artists_for_track(track_id).await.unwrap_or_default();
                let artist_names = artists
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");

                let mut album_title = String::new();
                let mut cover_image_id = None;

                if let Ok(album_id) = lm.get_album_id_for_release(&track.release_id).await {
                    if let Ok(Some(album)) = lm.get_album_by_id(&album_id).await {
                        album_title = album.title;
                        cover_image_id = album.cover_release_id;
                    }
                }

                items.push(BridgeQueueItem {
                    track_id: track_id.clone(),
                    title: track.title,
                    artist_names,
                    duration_ms: track.duration_ms,
                    album_title,
                    cover_image_id,
                });
            }

            items
        })
    }

    // MARK: - Import

    /// Scan a folder for importable music. Results arrive via on_scan_result callbacks.
    pub fn scan_folder(&self, path: String) -> Result<(), BridgeError> {
        self.import_handle
            .enqueue_folder_scan(PathBuf::from(path))
            .map_err(|e| BridgeError::Import { msg: e })
    }

    /// Search MusicBrainz for metadata matching a candidate.
    pub fn search_musicbrainz(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        self.runtime.block_on(async {
            let params = bae_core::musicbrainz::ReleaseSearchParams {
                artist: Some(artist),
                album: Some(album),
                year,
                label,
                ..Default::default()
            };
            let releases = bae_core::musicbrainz::search_releases_with_params(&params)
                .await
                .map_err(|e| BridgeError::Import {
                    msg: format!("MusicBrainz search failed: {e}"),
                })?;

            let release_ids: Vec<String> = releases.iter().map(|r| r.release_id.clone()).collect();
            let cover_urls = check_cover_art(&release_ids).await;

            Ok(releases
                .into_iter()
                .zip(cover_urls)
                .map(|(r, cover_url)| {
                    let year = r
                        .date
                        .as_ref()
                        .and_then(|d| d.split('-').next())
                        .and_then(|y| y.parse::<i32>().ok());
                    BridgeMetadataResult {
                        source: "musicbrainz".to_string(),
                        release_id: r.release_id,
                        title: r.title,
                        artist: r.artist,
                        year,
                        format: r.format,
                        label: r.label,
                        track_count: 0,
                        cover_url,
                    }
                })
                .collect())
        })
    }

    /// Search Discogs for metadata matching a candidate.
    pub fn search_discogs(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        self.runtime.block_on(async {
            let api_key =
                self.key_service
                    .get_discogs_key()
                    .ok_or_else(|| BridgeError::Import {
                        msg: "Discogs API key not configured".to_string(),
                    })?;
            let client = bae_core::discogs::DiscogsClient::new(api_key);
            let params = bae_core::discogs::client::DiscogsSearchParams {
                artist: Some(artist),
                release_title: Some(album),
                year,
                label,
                ..Default::default()
            };
            let results =
                client
                    .search_with_params(&params)
                    .await
                    .map_err(|e| BridgeError::Import {
                        msg: format!("Discogs search failed: {e}"),
                    })?;

            Ok(results
                .into_iter()
                .map(|r| {
                    let year = r.year.as_ref().and_then(|y| y.parse::<i32>().ok());
                    let format = r.format.as_ref().map(|f| f.join(", "));
                    let label = r.label.as_ref().and_then(|l| l.first().cloned());
                    let cover_url = r.thumb.clone();
                    BridgeMetadataResult {
                        source: "discogs".to_string(),
                        release_id: r.id.to_string(),
                        title: r.title,
                        artist: String::new(),
                        year,
                        format,
                        label,
                        track_count: 0,
                        cover_url,
                    }
                })
                .collect())
        })
    }

    /// Search by catalog number on MusicBrainz or Discogs.
    pub fn search_by_catalog_number(
        &self,
        catalog_number: String,
        source: String,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        if source == "musicbrainz" {
            self.runtime.block_on(async {
                let params = bae_core::musicbrainz::ReleaseSearchParams {
                    catalog_number: Some(catalog_number),
                    ..Default::default()
                };
                let releases = bae_core::musicbrainz::search_releases_with_params(&params)
                    .await
                    .map_err(|e| BridgeError::Import {
                        msg: format!("MusicBrainz search failed: {e}"),
                    })?;

                let release_ids: Vec<String> =
                    releases.iter().map(|r| r.release_id.clone()).collect();
                let cover_urls = check_cover_art(&release_ids).await;

                Ok(releases
                    .into_iter()
                    .zip(cover_urls)
                    .map(|(r, cover_url)| {
                        let year = r
                            .date
                            .as_ref()
                            .and_then(|d| d.split('-').next())
                            .and_then(|y| y.parse::<i32>().ok());
                        BridgeMetadataResult {
                            source: "musicbrainz".to_string(),
                            release_id: r.release_id,
                            title: r.title,
                            artist: r.artist,
                            year,
                            format: r.format,
                            label: r.label,
                            track_count: 0,
                            cover_url,
                        }
                    })
                    .collect())
            })
        } else {
            self.runtime.block_on(async {
                let api_key =
                    self.key_service
                        .get_discogs_key()
                        .ok_or_else(|| BridgeError::Import {
                            msg: "Discogs API key not configured".to_string(),
                        })?;
                let client = bae_core::discogs::DiscogsClient::new(api_key);
                let params = bae_core::discogs::client::DiscogsSearchParams {
                    catno: Some(catalog_number),
                    ..Default::default()
                };
                let results =
                    client
                        .search_with_params(&params)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Discogs search failed: {e}"),
                        })?;

                Ok(results
                    .into_iter()
                    .map(|r| {
                        let year = r.year.as_ref().and_then(|y| y.parse::<i32>().ok());
                        let format = r.format.as_ref().map(|f| f.join(", "));
                        let label = r.label.as_ref().and_then(|l| l.first().cloned());
                        let cover_url = r.thumb.clone();
                        BridgeMetadataResult {
                            source: "discogs".to_string(),
                            release_id: r.id.to_string(),
                            title: r.title,
                            artist: String::new(),
                            year,
                            format,
                            label,
                            track_count: 0,
                            cover_url,
                        }
                    })
                    .collect())
            })
        }
    }

    /// Search by barcode on MusicBrainz or Discogs.
    pub fn search_by_barcode(
        &self,
        barcode: String,
        source: String,
    ) -> Result<Vec<BridgeMetadataResult>, BridgeError> {
        if source == "musicbrainz" {
            self.runtime.block_on(async {
                let params = bae_core::musicbrainz::ReleaseSearchParams {
                    barcode: Some(barcode),
                    ..Default::default()
                };
                let releases = bae_core::musicbrainz::search_releases_with_params(&params)
                    .await
                    .map_err(|e| BridgeError::Import {
                        msg: format!("MusicBrainz search failed: {e}"),
                    })?;

                let release_ids: Vec<String> =
                    releases.iter().map(|r| r.release_id.clone()).collect();
                let cover_urls = check_cover_art(&release_ids).await;

                Ok(releases
                    .into_iter()
                    .zip(cover_urls)
                    .map(|(r, cover_url)| {
                        let year = r
                            .date
                            .as_ref()
                            .and_then(|d| d.split('-').next())
                            .and_then(|y| y.parse::<i32>().ok());
                        BridgeMetadataResult {
                            source: "musicbrainz".to_string(),
                            release_id: r.release_id,
                            title: r.title,
                            artist: r.artist,
                            year,
                            format: r.format,
                            label: r.label,
                            track_count: 0,
                            cover_url,
                        }
                    })
                    .collect())
            })
        } else {
            self.runtime.block_on(async {
                let api_key =
                    self.key_service
                        .get_discogs_key()
                        .ok_or_else(|| BridgeError::Import {
                            msg: "Discogs API key not configured".to_string(),
                        })?;
                let client = bae_core::discogs::DiscogsClient::new(api_key);
                let params = bae_core::discogs::client::DiscogsSearchParams {
                    barcode: Some(barcode),
                    ..Default::default()
                };
                let results =
                    client
                        .search_with_params(&params)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Discogs search failed: {e}"),
                        })?;

                Ok(results
                    .into_iter()
                    .map(|r| {
                        let year = r.year.as_ref().and_then(|y| y.parse::<i32>().ok());
                        let format = r.format.as_ref().map(|f| f.join(", "));
                        let label = r.label.as_ref().and_then(|l| l.first().cloned());
                        let cover_url = r.thumb.clone();
                        BridgeMetadataResult {
                            source: "discogs".to_string(),
                            release_id: r.id.to_string(),
                            title: r.title,
                            artist: String::new(),
                            year,
                            format,
                            label,
                            track_count: 0,
                            cover_url,
                        }
                    })
                    .collect())
            })
        }
    }

    /// Get categorized files for a candidate folder.
    pub fn get_candidate_files(
        &self,
        folder_path: String,
    ) -> Result<BridgeCandidateFiles, BridgeError> {
        use bae_core::import::folder_scanner::{collect_release_files, AudioContent};
        use std::path::Path;

        let files =
            collect_release_files(Path::new(&folder_path)).map_err(|e| BridgeError::Import {
                msg: format!("Failed to scan folder: {e}"),
            })?;

        let audio = match files.audio {
            AudioContent::CueFlacPairs(pairs) => BridgeAudioContent::CueFlacPairs {
                pairs: pairs
                    .into_iter()
                    .map(|p| BridgeCueFlacPair {
                        cue_name: p.cue_file.relative_path,
                        flac_name: p.audio_file.relative_path,
                        total_size: p.cue_file.size + p.audio_file.size,
                        track_count: p.track_count as u32,
                    })
                    .collect(),
            },
            AudioContent::TrackFiles(tracks) => BridgeAudioContent::TrackFiles {
                files: tracks
                    .into_iter()
                    .map(|f| BridgeFileInfo {
                        name: f.relative_path.clone(),
                        path: f.path.to_string_lossy().to_string(),
                        size: f.size,
                    })
                    .collect(),
            },
        };

        let artwork = files
            .artwork
            .into_iter()
            .map(|f| BridgeFileInfo {
                name: f.relative_path.clone(),
                path: f.path.to_string_lossy().to_string(),
                size: f.size,
            })
            .collect();

        let documents = files
            .documents
            .into_iter()
            .map(|f| BridgeFileInfo {
                name: f.relative_path.clone(),
                path: f.path.to_string_lossy().to_string(),
                size: f.size,
            })
            .collect();

        Ok(BridgeCandidateFiles {
            audio,
            artwork,
            documents,
            bad_audio_count: files.bad_audio_count as u32,
            bad_image_count: files.bad_image_count as u32,
        })
    }

    /// Start importing a folder with the selected metadata release.
    pub fn commit_import(
        &self,
        folder_path: String,
        release_id: String,
        source: String,
        selected_cover: Option<BridgeCoverSelection>,
        managed: bool,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let import_id = uuid::Uuid::new_v4().to_string();
            self.import_id_to_folder
                .lock()
                .unwrap()
                .insert(import_id.clone(), folder_path.clone());
            let folder = PathBuf::from(&folder_path);

            let cover = selected_cover.map(|c| match c {
                BridgeCoverSelection::ReleaseImage { file_id } => {
                    bae_core::import::CoverSelection::Local(file_id)
                }
                BridgeCoverSelection::RemoteCover { url, .. } => {
                    bae_core::import::CoverSelection::Remote(url)
                }
            });

            let request = if source == "musicbrainz" {
                bae_core::import::ImportRequest::Folder {
                    import_id,
                    discogs_release: None,
                    mb_release: Some(bae_core::musicbrainz::MbRelease {
                        release_id,
                        release_group_id: String::new(),
                        title: String::new(),
                        artist: String::new(),
                        date: None,
                        first_release_date: None,
                        format: None,
                        country: None,
                        label: None,
                        catalog_number: None,
                        barcode: None,
                        is_compilation: false,
                    }),
                    folder,
                    master_year: 0,
                    managed,
                    selected_cover: cover,
                }
            } else {
                // Discogs: need to fetch the full release first
                let api_key =
                    self.key_service
                        .get_discogs_key()
                        .ok_or_else(|| BridgeError::Import {
                            msg: "Discogs API key not configured".to_string(),
                        })?;
                let client = bae_core::discogs::DiscogsClient::new(api_key);
                let discogs_release =
                    client
                        .get_release(&release_id)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Failed to fetch Discogs release: {e}"),
                        })?;
                bae_core::import::ImportRequest::Folder {
                    import_id,
                    discogs_release: Some(discogs_release),
                    mb_release: None,
                    folder,
                    master_year: 0,
                    managed,
                    selected_cover: cover,
                }
            };

            self.import_handle
                .send_request(request)
                .await
                .map_err(|e| BridgeError::Import { msg: e })?;
            Ok(())
        })
    }

    /// Lookup releases by MusicBrainz disc ID.
    pub fn lookup_discid(&self, discid: String) -> Result<BridgeDiscIdResult, BridgeError> {
        self.runtime.block_on(async {
            match bae_core::musicbrainz::lookup_by_discid(&discid).await {
                Ok((releases, _external_urls)) => {
                    if releases.is_empty() {
                        return Ok(BridgeDiscIdResult::NoMatches);
                    }

                    let release_ids: Vec<String> =
                        releases.iter().map(|r| r.release_id.clone()).collect();
                    let cover_urls = check_cover_art(&release_ids).await;

                    let results: Vec<BridgeMetadataResult> = releases
                        .into_iter()
                        .zip(cover_urls)
                        .map(|(r, cover_url)| {
                            let year = r
                                .date
                                .as_ref()
                                .and_then(|d| d.split('-').next())
                                .and_then(|y| y.parse::<i32>().ok());
                            BridgeMetadataResult {
                                source: "musicbrainz".to_string(),
                                release_id: r.release_id,
                                title: r.title,
                                artist: r.artist,
                                year,
                                format: r.format,
                                label: r.label,
                                track_count: 0,
                                cover_url,
                            }
                        })
                        .collect();

                    if results.len() == 1 {
                        Ok(BridgeDiscIdResult::SingleMatch {
                            result: results.into_iter().next().unwrap(),
                        })
                    } else {
                        Ok(BridgeDiscIdResult::MultipleMatches { results })
                    }
                }
                Err(bae_core::musicbrainz::MusicBrainzError::NotFound(_)) => {
                    Ok(BridgeDiscIdResult::NoMatches)
                }
                Err(e) => Err(BridgeError::Import {
                    msg: format!("DiscID lookup failed: {e}"),
                }),
            }
        })
    }

    /// Prefetch full release details for the confirmation step.
    pub fn prefetch_release(
        &self,
        release_id: String,
        source: String,
    ) -> Result<BridgeReleaseDetail, BridgeError> {
        self.runtime.block_on(async {
            if source == "musicbrainz" {
                let (_mb_release, _external_urls, mb_response) =
                    bae_core::musicbrainz::lookup_release_by_id(&release_id)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Failed to fetch release: {e}"),
                        })?;

                let mb_release = mb_response.to_mb_release();
                let year = mb_release
                    .date
                    .as_ref()
                    .and_then(|d| d.split('-').next())
                    .and_then(|y| y.parse::<i32>().ok());

                let tracks: Vec<BridgeReleaseTrack> = mb_response
                    .media
                    .iter()
                    .flat_map(|medium| {
                        medium.tracks.iter().map(|t| BridgeReleaseTrack {
                            title: t
                                .title
                                .clone()
                                .or_else(|| t.recording.as_ref().and_then(|r| r.title.clone()))
                                .unwrap_or_default(),
                            artist: None,
                            duration_ms: t.length,
                            position: t.number.clone().unwrap_or_else(|| {
                                t.position.map(|p| p.to_string()).unwrap_or_default()
                            }),
                        })
                    })
                    .collect();

                let cover_url = format!(
                    "https://coverartarchive.org/release/{}/front-500",
                    release_id
                );
                let cover_art = vec![BridgeCoverArt {
                    url: cover_url,
                    source: "musicbrainz".to_string(),
                }];

                Ok(BridgeReleaseDetail {
                    release_id: mb_response.id,
                    source: "musicbrainz".to_string(),
                    title: mb_response.title,
                    artist: mb_release.artist,
                    year,
                    format: mb_release.format,
                    label: mb_release.label,
                    catalog_number: mb_release.catalog_number,
                    track_count: tracks.len() as u32,
                    tracks,
                    cover_art,
                })
            } else {
                let api_key =
                    self.key_service
                        .get_discogs_key()
                        .ok_or_else(|| BridgeError::Import {
                            msg: "Discogs API key not configured".to_string(),
                        })?;
                let client = bae_core::discogs::DiscogsClient::new(api_key);
                let release =
                    client
                        .get_release(&release_id)
                        .await
                        .map_err(|e| BridgeError::Import {
                            msg: format!("Failed to fetch Discogs release: {e}"),
                        })?;

                let tracks: Vec<BridgeReleaseTrack> = release
                    .tracklist
                    .iter()
                    .map(|t| BridgeReleaseTrack {
                        title: t.title.clone(),
                        artist: None,
                        duration_ms: t.duration.as_ref().and_then(|d| parse_duration_to_ms(d)),
                        position: t.position.clone(),
                    })
                    .collect();

                let mut cover_art = Vec::new();
                if let Some(ref url) = release.cover_image {
                    cover_art.push(BridgeCoverArt {
                        url: url.clone(),
                        source: "discogs".to_string(),
                    });
                }

                let year = release.year.map(|y| y as i32);
                let artist = release
                    .artists
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");

                Ok(BridgeReleaseDetail {
                    release_id: release.id.clone(),
                    source: "discogs".to_string(),
                    title: release.title.clone(),
                    artist,
                    year,
                    format: if release.format.is_empty() {
                        None
                    } else {
                        Some(release.format.join(", "))
                    },
                    label: release.label.first().cloned(),
                    catalog_number: release.catno.clone(),
                    track_count: tracks.len() as u32,
                    tracks,
                    cover_art,
                })
            }
        })
    }

    // MARK: - Settings

    /// Get current configuration.
    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig {
            library_id: self.config.library_id.clone(),
            library_name: self.config.library_name.clone(),
            library_path: self.config.library_dir.to_string_lossy().to_string(),
            has_discogs_token: self.key_service.get_discogs_key().is_some(),
            subsonic_port: self.config.server_port,
            subsonic_bind_address: self.config.server_bind_address.clone(),
            subsonic_username: self.config.server_username.clone(),
        }
    }

    /// Rename the library.
    pub fn rename_library(&self, name: String) -> Result<(), BridgeError> {
        Config::rename_library(&self.config.library_dir, &name).map_err(|e| BridgeError::Config {
            msg: format!("{e}"),
        })
    }

    /// Save Discogs API token to the OS keyring.
    pub fn save_discogs_token(&self, token: String) -> Result<(), BridgeError> {
        self.key_service
            .set_discogs_key(&token)
            .map_err(|e| BridgeError::Config {
                msg: format!("{e}"),
            })
    }

    /// Check if a Discogs API token is configured.
    pub fn has_discogs_token(&self) -> bool {
        self.key_service.get_discogs_key().is_some()
    }

    /// Get the Discogs API token (for display in settings).
    pub fn get_discogs_token(&self) -> Option<String> {
        self.key_service.get_discogs_key()
    }

    /// Remove the Discogs API token from the OS keyring.
    pub fn remove_discogs_token(&self) -> Result<(), BridgeError> {
        self.key_service
            .delete_discogs_key()
            .map_err(|e| BridgeError::Config {
                msg: format!("{e}"),
            })
    }

    // MARK: - Events

    /// Register a callback handler for playback and import events.
    /// Spawns background tasks that forward events to the handler.
    pub fn set_event_handler(&self, handler: Box<dyn AppEventHandler>) {
        let handler: Arc<dyn AppEventHandler> = Arc::from(handler);
        let prev = self.event_handler.lock().unwrap().replace(handler.clone());
        drop(prev);

        // Playback events
        let rx = self.playback_handle.subscribe_progress();
        let lm = self.library_manager.clone();
        self.runtime
            .spawn(dispatch_playback_events(rx, handler.clone(), lm));

        // Scan events
        let scan_rx = self.import_handle.subscribe_folder_scan_events();
        self.runtime
            .spawn(dispatch_scan_events(scan_rx, handler.clone()));

        // Import progress events
        let import_rx = self.import_handle.subscribe_all_imports();
        let id_map = self.import_id_to_folder.clone();
        self.runtime
            .spawn(dispatch_import_events(import_rx, handler, id_map));
    }

    /// Fetch available remote cover art options for a release.
    /// Checks MusicBrainz Cover Art Archive and Discogs.
    pub fn fetch_remote_covers(
        &self,
        release_id: String,
    ) -> Result<Vec<BridgeRemoteCover>, BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();

            // Get the release to find its album
            let release = lm
                .database()
                .get_release_by_id(&release_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Release '{release_id}' not found"),
                })?;

            // Get the album to access MusicBrainz/Discogs IDs
            let album = lm
                .get_album_by_id(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: format!("Album '{}' not found", release.album_id),
                })?;

            // Check current cover source to skip duplicates
            let current_source = lm
                .get_library_image(&release_id, &bae_core::db::LibraryImageType::Cover)
                .await
                .ok()
                .flatten()
                .map(|img| img.source);

            let mut covers = Vec::new();

            // Try MusicBrainz Cover Art Archive
            if let Some(ref mb) = album.musicbrainz_release {
                if current_source.as_deref() != Some("musicbrainz") {
                    if let Some(url) =
                        bae_core::import::cover_art::fetch_cover_art_from_archive(&mb.release_id)
                            .await
                    {
                        covers.push(BridgeRemoteCover {
                            url: url.clone(),
                            thumbnail_url: url,
                            label: "MusicBrainz".to_string(),
                            source: "musicbrainz".to_string(),
                        });
                    }
                }
            }

            // Try Discogs
            if let Some(ref discogs_id) = release.discogs_release_id {
                if current_source.as_deref() != Some("discogs") {
                    if let Some(token) = self.key_service.get_discogs_key() {
                        let client = bae_core::discogs::client::DiscogsClient::new(token);
                        if let Ok(discogs_release) = client.get_release(discogs_id).await {
                            if let Some(cover_url) = discogs_release
                                .cover_image
                                .or(discogs_release.thumb.clone())
                            {
                                let thumb =
                                    discogs_release.thumb.unwrap_or_else(|| cover_url.clone());
                                covers.push(BridgeRemoteCover {
                                    url: cover_url,
                                    thumbnail_url: thumb,
                                    label: "Discogs".to_string(),
                                    source: "discogs".to_string(),
                                });
                            }
                        }
                    }
                }
            }

            Ok(covers)
        })
    }

    /// Change the cover art for an album's release.
    pub fn change_cover(
        &self,
        album_id: String,
        release_id: String,
        selection: BridgeCoverSelection,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let lm = self.library_manager.get();
            let library_dir = &self.config.library_dir;

            match selection {
                BridgeCoverSelection::ReleaseImage { file_id } => {
                    let file = lm
                        .get_file_by_id(&file_id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?
                        .ok_or_else(|| BridgeError::NotFound {
                            msg: format!("File '{file_id}' not found"),
                        })?;

                    let release = lm
                        .database()
                        .get_release_by_id(&file.release_id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?
                        .ok_or_else(|| BridgeError::NotFound {
                            msg: format!("Release '{}' not found", file.release_id),
                        })?;

                    let source_path = if release.managed_locally {
                        file.local_storage_path(library_dir)
                    } else if let Some(ref unmanaged) = release.unmanaged_path {
                        std::path::PathBuf::from(unmanaged).join(&file.original_filename)
                    } else {
                        return Err(BridgeError::Internal {
                            msg: "Release has no local file storage".to_string(),
                        });
                    };

                    let bytes = std::fs::read(&source_path).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to read file: {e}"),
                    })?;

                    let content_type = file.content_type.clone();
                    write_cover_and_update_db(
                        lm,
                        library_dir,
                        &album_id,
                        &release_id,
                        &bytes,
                        content_type,
                        "local",
                        Some(format!("release://{}", file.original_filename)),
                    )
                    .await?;
                }
                BridgeCoverSelection::RemoteCover { url, source } => {
                    let (bytes, content_type) =
                        bae_core::import::cover_art::download_cover_art_bytes(&url)
                            .await
                            .map_err(|e| BridgeError::Internal {
                                msg: format!("Failed to download cover: {e}"),
                            })?;

                    write_cover_and_update_db(
                        lm,
                        library_dir,
                        &album_id,
                        &release_id,
                        &bytes,
                        content_type,
                        &source,
                        Some(url),
                    )
                    .await?;
                }
            }

            Ok(())
        })
    }

    /// Get the configured Subsonic server port.
    pub fn server_port(&self) -> u16 {
        self.config.server_port
    }

    /// Get the configured Subsonic server bind address.
    pub fn server_bind_address(&self) -> String {
        self.config.server_bind_address.clone()
    }

    /// Whether the Subsonic server is currently running.
    pub fn is_subsonic_running(&self) -> bool {
        let guard = self.subsonic_server_handle.lock().unwrap();
        match &*guard {
            Some(handle) => !handle.is_finished(),
            None => false,
        }
    }

    /// Start the Subsonic server. No-op if already running.
    pub fn start_subsonic_server(&self) -> Result<(), BridgeError> {
        {
            let guard = self.subsonic_server_handle.lock().unwrap();
            if let Some(handle) = &*guard {
                if !handle.is_finished() {
                    return Ok(());
                }
            }
        }

        let auth = if self.config.server_auth_enabled {
            let password = self.key_service.get_server_password();
            bae_core::subsonic::SubsonicAuth {
                enabled: self.config.server_username.is_some() && password.is_some(),
                username: self.config.server_username.clone(),
                password,
            }
        } else {
            bae_core::subsonic::SubsonicAuth {
                enabled: false,
                username: None,
                password: None,
            }
        };

        let mut app = bae_core::subsonic::create_router(
            self.library_manager.clone(),
            self.encryption_service.clone(),
            self.config.library_dir.clone(),
            self.key_service.clone(),
            auth,
        );

        if let Some(ref ch) = self.cloud_home {
            let cloud_state = Arc::new(bae_core::cloud_routes::CloudRouteState {
                cloud_home: ch.clone(),
            });
            let cloud_router = bae_core::cloud_routes::create_cloud_router(cloud_state);
            app = app.merge(cloud_router);
        }

        let addr = format!(
            "{}:{}",
            self.config.server_bind_address, self.config.server_port
        );
        let handle = self.runtime.spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => {
                    info!("Subsonic server listening on http://{}", addr);
                    l
                }
                Err(e) => {
                    warn!("Failed to bind Subsonic server to {}: {}", addr, e);
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app).await {
                warn!("Subsonic server error: {}", e);
            }
        });

        let mut guard = self.subsonic_server_handle.lock().unwrap();
        *guard = Some(handle);

        Ok(())
    }

    /// Stop the Subsonic server if running.
    pub fn stop_subsonic_server(&self) {
        let mut guard = self.subsonic_server_handle.lock().unwrap();
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    /// Create a share link for a release. Requires cloud home and encryption.
    /// Returns the share URL on success.
    pub fn create_share_link(&self, release_id: String) -> Result<String, BridgeError> {
        self.runtime.block_on(async {
            let cloud_home = self
                .cloud_home
                .as_ref()
                .ok_or_else(|| BridgeError::Config {
                    msg: "Cloud storage not configured".to_string(),
                })?;
            let encryption =
                self.encryption_service
                    .as_ref()
                    .ok_or_else(|| BridgeError::Config {
                        msg: "Encryption not configured".to_string(),
                    })?;
            let base_url =
                self.config
                    .share_base_url
                    .as_deref()
                    .ok_or_else(|| BridgeError::Config {
                        msg: "Share base URL not configured".to_string(),
                    })?;

            let db = self.library_manager.get().database().clone();

            // Verify release exists and is in the cloud
            let release = db
                .get_release_by_id(&release_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: "Release not found".to_string(),
                })?;
            if !release.managed_in_cloud {
                return Err(BridgeError::Config {
                    msg: "Release must be managed in the cloud to share".to_string(),
                });
            }

            // Get album metadata
            let album = db
                .get_album_by_id(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?
                .ok_or_else(|| BridgeError::NotFound {
                    msg: "Album not found".to_string(),
                })?;
            let artists = db
                .get_artists_for_album(&release.album_id)
                .await
                .map_err(|e| BridgeError::Database {
                    msg: format!("{e}"),
                })?;
            let artist_name = artists
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string());

            // Get tracks and files
            let tracks = db.get_tracks_for_release(&release_id).await.map_err(|e| {
                BridgeError::Database {
                    msg: format!("{e}"),
                }
            })?;
            let files =
                db.get_files_for_release(&release_id)
                    .await
                    .map_err(|e| BridgeError::Database {
                        msg: format!("{e}"),
                    })?;

            // Build track list with file keys
            use bae_core::sync::share_format;

            let mut share_tracks = Vec::new();
            let mut manifest_files = Vec::new();

            for track in &tracks {
                let audio_format =
                    db.get_audio_format_by_track_id(&track.id)
                        .await
                        .map_err(|e| BridgeError::Database {
                            msg: format!("{e}"),
                        })?;
                if let Some(af) = audio_format {
                    let file = files
                        .iter()
                        .find(|f| af.file_id.as_deref() == Some(f.id.as_str()));
                    if let Some(file) = file {
                        let file_key = bae_core::storage::storage_path(&file.id);
                        let format = share_format::format_for_content_type(&af.content_type);
                        share_tracks.push(share_format::ShareMetaTrack {
                            number: track.track_number,
                            title: track.title.clone(),
                            duration_secs: track.duration_ms.map(|ms| ms / 1000),
                            file_key: file_key.clone(),
                            format: format.to_string(),
                        });
                        manifest_files.push(file_key);
                    }
                }
            }

            // Cover image key
            let cover_release_id = album.cover_release_id.as_deref().unwrap_or(&release_id);
            let cover_image_key = find_cover_image_key(&db, cover_release_id).await;
            if let Some(ref key) = cover_image_key {
                manifest_files.push(key.clone());
            }

            // Per-release encryption key
            use base64::Engine;

            let release_enc = encryption.derive_release_encryption(&release_id);
            let release_key_b64 =
                base64::engine::general_purpose::STANDARD.encode(release_enc.key_bytes());

            // Build ShareMeta
            let meta = share_format::ShareMeta {
                album_name: album.title,
                artist: artist_name,
                year: album.year,
                cover_image_key,
                tracks: share_tracks,
                release_key_b64,
            };
            let meta_json = serde_json::to_vec(&meta).map_err(|e| BridgeError::Internal {
                msg: format!("Serialize error: {e}"),
            })?;

            // Generate per-share key and encrypt
            let per_share_key = bae_core::encryption::generate_random_key();
            let per_share_enc = EncryptionService::from_key(per_share_key);
            let meta_encrypted = per_share_enc.encrypt_chunked(&meta_json);

            // Build manifest
            let manifest = share_format::ShareManifest {
                files: manifest_files,
            };
            let manifest_json =
                serde_json::to_vec(&manifest).map_err(|e| BridgeError::Internal {
                    msg: format!("Serialize error: {e}"),
                })?;

            // Upload to cloud home
            let share_id = uuid::Uuid::new_v4().to_string();
            cloud_home
                .write(&format!("shares/{share_id}/meta.enc"), meta_encrypted)
                .await
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Upload error: {e}"),
                })?;
            cloud_home
                .write(&format!("shares/{share_id}/manifest.json"), manifest_json)
                .await
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Upload error: {e}"),
                })?;

            // Build URL
            let key_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(per_share_key);
            Ok(format!("{base_url}/share/{share_id}#{key_b64}"))
        })
    }

    // =========================================================================
    // Storage transfer
    // =========================================================================

    /// Transfer an unmanaged release to managed local storage.
    /// Copies files into the library's managed storage directory.
    pub fn transfer_release_to_managed(&self, release_id: String) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            let encryption_service = self.library_manager.get().encryption_service().cloned();
            let library_dir = self.config.library_dir.clone();

            let transfer_service = bae_core::storage::transfer::TransferService::new(
                self.library_manager.clone(),
                encryption_service,
                library_dir,
            );

            let mut rx = transfer_service.transfer(
                release_id.clone(),
                bae_core::storage::transfer::TransferTarget::ManagedLocal,
            );

            while let Some(progress) = rx.recv().await {
                match progress {
                    bae_core::storage::transfer::TransferProgress::Complete { .. } => {
                        return Ok(());
                    }
                    bae_core::storage::transfer::TransferProgress::Failed { error, .. } => {
                        return Err(BridgeError::Internal { msg: error });
                    }
                    _ => {}
                }
            }

            Err(BridgeError::Internal {
                msg: "Transfer ended without completion or failure".to_string(),
            })
        })
    }

    /// Eject a release from managed storage to a local folder.
    /// The caller (Swift) is responsible for presenting a folder picker
    /// and passing the chosen path.
    pub fn eject_release_storage(
        &self,
        release_id: String,
        target_dir: String,
    ) -> Result<(), BridgeError> {
        let target_path = std::path::PathBuf::from(&target_dir);
        if !target_path.exists() {
            return Err(BridgeError::Config {
                msg: format!("Target directory does not exist: {target_dir}"),
            });
        }

        self.runtime.block_on(async {
            let encryption_service = self.library_manager.get().encryption_service().cloned();
            let library_dir = self.config.library_dir.clone();

            let transfer_service = bae_core::storage::transfer::TransferService::new(
                self.library_manager.clone(),
                encryption_service,
                library_dir.clone(),
            );

            let mut rx = transfer_service.transfer(
                release_id.clone(),
                bae_core::storage::transfer::TransferTarget::Eject(target_path),
            );

            while let Some(progress) = rx.recv().await {
                match progress {
                    bae_core::storage::transfer::TransferProgress::Complete { .. } => {
                        bae_core::storage::cleanup::schedule_cleanup(&library_dir);
                        return Ok(());
                    }
                    bae_core::storage::transfer::TransferProgress::Failed { error, .. } => {
                        return Err(BridgeError::Internal { msg: error });
                    }
                    _ => {}
                }
            }

            Err(BridgeError::Internal {
                msg: "Transfer ended without completion or failure".to_string(),
            })
        })
    }

    // =========================================================================
    // Sync / membership
    // =========================================================================

    /// Get the current sync configuration status.
    /// Re-reads from config.yaml to pick up changes made by sign-in/disconnect methods.
    pub fn get_sync_status(&self) -> BridgeSyncStatus {
        let config = Config::load();
        let configured = config.sync_enabled(&self.key_service);
        BridgeSyncStatus {
            configured,
            syncing: false,
            last_sync_time: None,
            error: None,
            device_count: if configured { 1 } else { 0 },
        }
    }

    /// Get the current sync bucket configuration.
    /// Re-reads from config.yaml to pick up changes made by select/disconnect/sign-in methods.
    pub fn get_sync_config(&self) -> BridgeSyncConfig {
        let config = Config::load();
        let cloud_provider = config.cloud_provider.as_ref().map(cloud_provider_to_string);
        let cloud_account_display = cloud_account_display_for(&config, &self.key_service);
        BridgeSyncConfig {
            cloud_provider,
            s3_bucket: config.cloud_home_s3_bucket.clone(),
            s3_region: config.cloud_home_s3_region.clone(),
            s3_endpoint: config.cloud_home_s3_endpoint.clone(),
            s3_key_prefix: config.cloud_home_s3_key_prefix.clone(),
            share_base_url: config.share_base_url.clone(),
            cloud_account_display,
            bae_cloud_url: config.cloud_home_bae_cloud_url.clone(),
        }
    }

    /// Save S3 sync configuration. Sets cloud_provider to S3 and persists
    /// bucket/region/endpoint/key_prefix to config.yaml and credentials to keyring.
    pub fn save_sync_config(&self, config_data: BridgeSaveSyncConfig) -> Result<(), BridgeError> {
        // Save credentials to keyring
        let creds = bae_core::keys::CloudHomeCredentials::S3 {
            access_key: config_data.access_key,
            secret_key: config_data.secret_key,
        };
        self.key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to save credentials: {e}"),
            })?;

        // Update config and persist
        let mut config = self.config.clone();
        config.cloud_provider = Some(bae_core::config::CloudProvider::S3);
        config.cloud_home_s3_bucket = Some(config_data.bucket);
        config.cloud_home_s3_region = Some(config_data.region);
        config.cloud_home_s3_endpoint = config_data.endpoint.filter(|s| !s.is_empty());
        config.cloud_home_s3_key_prefix = config_data.key_prefix.filter(|s| !s.is_empty());
        config.share_base_url = config_data.share_base_url.filter(|s| !s.is_empty());
        config.save().map_err(|e| BridgeError::Config {
            msg: format!("Failed to save config: {e}"),
        })?;

        info!("Saved S3 sync configuration");
        Ok(())
    }

    /// Disconnect the current cloud provider, clearing all sync credentials and config.
    pub fn disconnect_cloud_provider(&self) -> Result<(), BridgeError> {
        let mut config = Config::load();

        // Best-effort logout for bae cloud
        if matches!(
            config.cloud_provider,
            Some(bae_core::config::CloudProvider::BaeCloud)
        ) {
            if let Some(bae_core::keys::CloudHomeCredentials::BaeCloud { session_token }) =
                self.key_service.get_cloud_home_credentials()
            {
                let _ = self
                    .runtime
                    .block_on(async { bae_core::bae_cloud_api::logout(&session_token).await });
            }
        }

        // Clear all cloud home config fields
        config.cloud_provider = None;
        config.cloud_home_s3_bucket = None;
        config.cloud_home_s3_region = None;
        config.cloud_home_s3_endpoint = None;
        config.cloud_home_s3_key_prefix = None;
        config.cloud_home_google_drive_folder_id = None;
        config.cloud_home_dropbox_folder_path = None;
        config.cloud_home_onedrive_drive_id = None;
        config.cloud_home_onedrive_folder_id = None;
        config.cloud_home_icloud_container_path = None;
        config.cloud_home_bae_cloud_url = None;
        config.cloud_home_bae_cloud_username = None;

        config.save().map_err(|e| BridgeError::Config {
            msg: format!("Failed to save config: {e}"),
        })?;

        // Delete cloud home credentials from keyring (best-effort)
        if let Err(e) = self.key_service.delete_cloud_home_credentials() {
            warn!("Failed to delete cloud home credentials: {e}");
        }

        info!("Disconnected cloud provider");
        Ok(())
    }

    /// Sign up for a new bae cloud account, provision the library, and configure sync.
    /// Returns the library URL on success.
    pub fn sign_up_bae_cloud(
        &self,
        email: String,
        password: String,
    ) -> Result<String, BridgeError> {
        self.runtime.block_on(async {
            // 1. Call signup API (username = email prefix)
            let username = email.split('@').next().unwrap_or(&email).to_string();
            let resp = bae_core::bae_cloud_api::signup(&email, &username, &password)
                .await
                .map_err(|e| BridgeError::Config {
                    msg: format!("Signup failed: {e}"),
                })?;

            // 2. Get or create Ed25519 keypair for provisioning
            let keypair =
                self.key_service
                    .get_or_create_user_keypair()
                    .map_err(|e| BridgeError::Config {
                        msg: format!("Keypair error: {e}"),
                    })?;

            // 3. Sign provision message
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string();
            let message = format!("provision:{}:{timestamp}", resp.library_id);
            let signature = keypair.sign(message.as_bytes());

            let pubkey_hex = hex::encode(keypair.public_key);
            let sig_hex = hex::encode(signature);

            // 4. Call provision API
            bae_core::bae_cloud_api::provision(
                &resp.session_token,
                &pubkey_hex,
                &sig_hex,
                &timestamp,
            )
            .await
            .map_err(|e| BridgeError::Config {
                msg: format!("Provision failed: {e}"),
            })?;

            // 5. Save credentials to keyring
            self.key_service
                .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::BaeCloud {
                    session_token: resp.session_token,
                })
                .map_err(|e| BridgeError::Config {
                    msg: format!("Keyring error: {e}"),
                })?;

            // 6. Update config
            let mut config = Config::load();
            config.cloud_provider = Some(bae_core::config::CloudProvider::BaeCloud);
            config.cloud_home_bae_cloud_url = Some(resp.library_url.clone());
            config.cloud_home_bae_cloud_username = Some(username);
            config.save().map_err(|e| BridgeError::Config {
                msg: format!("Config save error: {e}"),
            })?;

            info!("Signed up for bae cloud, library URL: {}", resp.library_url);
            Ok(resp.library_url)
        })
    }

    /// Log in to an existing bae cloud account and configure sync.
    /// Returns the library URL on success.
    pub fn log_in_bae_cloud(&self, email: String, password: String) -> Result<String, BridgeError> {
        self.runtime.block_on(async {
            // 1. Call login API
            let resp = bae_core::bae_cloud_api::login(&email, &password)
                .await
                .map_err(|e| BridgeError::Config {
                    msg: format!("Login failed: {e}"),
                })?;

            // 2. If not yet provisioned, provision now
            if !resp.provisioned {
                let keypair = self.key_service.get_or_create_user_keypair().map_err(|e| {
                    BridgeError::Config {
                        msg: format!("Keypair error: {e}"),
                    }
                })?;

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string();
                let message = format!("provision:{}:{timestamp}", resp.library_id);
                let signature = keypair.sign(message.as_bytes());

                let pubkey_hex = hex::encode(keypair.public_key);
                let sig_hex = hex::encode(signature);

                bae_core::bae_cloud_api::provision(
                    &resp.session_token,
                    &pubkey_hex,
                    &sig_hex,
                    &timestamp,
                )
                .await
                .map_err(|e| BridgeError::Config {
                    msg: format!("Provision failed: {e}"),
                })?;
            }

            // 3. Save credentials to keyring
            self.key_service
                .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::BaeCloud {
                    session_token: resp.session_token,
                })
                .map_err(|e| BridgeError::Config {
                    msg: format!("Keyring error: {e}"),
                })?;

            // 4. Update config
            let mut config = Config::load();
            config.cloud_provider = Some(bae_core::config::CloudProvider::BaeCloud);
            config.cloud_home_bae_cloud_url = Some(resp.library_url.clone());
            let display_name = email.split('@').next().unwrap_or(&email).to_string();
            config.cloud_home_bae_cloud_username = Some(display_name);
            config.save().map_err(|e| BridgeError::Config {
                msg: format!("Config save error: {e}"),
            })?;

            info!("Logged in to bae cloud, library URL: {}", resp.library_url);
            Ok(resp.library_url)
        })
    }

    /// Start OAuth sign-in for a cloud provider (Google Drive, Dropbox, OneDrive).
    /// Opens the system browser for authorization, waits for the callback, stores tokens,
    /// creates provider folders, and saves config. Blocks until the flow completes or times out.
    pub fn sign_in_cloud_provider(&self, provider: String) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            match provider.as_str() {
                "google_drive" => sign_in_google_drive(&self.key_service, &self.config).await,
                "dropbox" => sign_in_dropbox(&self.key_service, &self.config).await,
                "onedrive" => sign_in_onedrive(&self.key_service, &self.config).await,
                _ => Err(BridgeError::Config {
                    msg: format!("Provider '{provider}' does not use OAuth sign-in"),
                }),
            }
        })
    }

    /// Detect and configure iCloud Drive as the cloud home.
    pub fn use_icloud(&self) -> Result<(), BridgeError> {
        let container = detect_icloud_container().ok_or_else(|| BridgeError::Config {
            msg: "iCloud Drive is not available. Sign in to iCloud in System Settings.".to_string(),
        })?;

        let mut config = Config::load();
        let cloud_home_path = container.join(&config.library_id);

        config.cloud_provider = Some(bae_core::config::CloudProvider::ICloud);
        config.cloud_home_icloud_container_path =
            Some(cloud_home_path.to_string_lossy().to_string());

        config.save().map_err(|e| BridgeError::Config {
            msg: format!("Failed to save iCloud config: {e}"),
        })?;

        info!("Configured iCloud Drive at {}", cloud_home_path.display());
        Ok(())
    }

    /// Get the user's Ed25519 public key (hex-encoded), or nil if no keypair exists.
    pub fn get_user_pubkey(&self) -> Option<String> {
        self.key_service.get_user_public_key().map(hex::encode)
    }

    /// Generate a follow code for this library.
    /// Requires a share_base_url and encryption key to be configured.
    pub fn generate_follow_code(&self) -> Result<String, BridgeError> {
        let proxy_url = self
            .config
            .share_base_url
            .as_ref()
            .ok_or_else(|| BridgeError::Config {
                msg: "No share base URL configured. Set it in sync settings.".to_string(),
            })?;

        let encryption_key_hex =
            self.key_service
                .get_encryption_key()
                .ok_or_else(|| BridgeError::Config {
                    msg: "No encryption key found".to_string(),
                })?;

        let encryption_key =
            hex::decode(&encryption_key_hex).map_err(|e| BridgeError::Internal {
                msg: format!("Invalid encryption key: {e}"),
            })?;

        let library_name = self.config.library_name.as_deref();
        Ok(bae_core::follow_code::encode(
            proxy_url,
            &encryption_key,
            library_name,
        ))
    }

    /// Follow a remote library using a follow code string.
    ///
    /// Decodes the follow code, generates a UUID for the followed library, saves
    /// the encryption key to the keyring, and adds the library to config.yaml.
    pub fn follow_library(
        &self,
        follow_code: String,
    ) -> Result<BridgeFollowedLibrary, BridgeError> {
        let (proxy_url, encryption_key, name) = bae_core::follow_code::decode(&follow_code)
            .map_err(|e| BridgeError::Config {
                msg: format!("Invalid follow code: {e}"),
            })?;

        let id = uuid::Uuid::new_v4().to_string();
        let display_name = name.unwrap_or_else(|| proxy_url.clone());

        // Save encryption key to keyring
        self.key_service
            .set_followed_encryption_key(&id, &encryption_key)
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to save encryption key: {e}"),
            })?;

        // Add to config and persist
        let mut config = self.config.clone();
        let followed = bae_core::config::FollowedLibrary {
            id: id.clone(),
            name: display_name.clone(),
            proxy_url: proxy_url.clone(),
        };
        config
            .add_followed_library(followed)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to save followed library: {e}"),
            })?;

        info!("Followed library '{}' at {}", display_name, proxy_url);

        Ok(BridgeFollowedLibrary {
            id,
            name: display_name,
            url: proxy_url,
        })
    }

    /// Unfollow a library by its ID.
    ///
    /// Removes the encryption key from the keyring, removes the library from
    /// config.yaml, and deletes any local data (snapshot DB, etc.).
    pub fn unfollow_library(&self, library_id: String) -> Result<(), BridgeError> {
        // Delete encryption key from keyring (non-fatal if missing)
        if let Err(e) = self.key_service.delete_followed_encryption_key(&library_id) {
            warn!("Failed to delete followed library encryption key: {e}");
        }

        // Remove from config and persist
        let mut config = self.config.clone();
        config
            .remove_followed_library(&library_id)
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to remove followed library: {e}"),
            })?;

        // Remove local data directory
        let dir = Config::followed_library_dir(&library_id);
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                warn!(
                    "Failed to remove followed library data at {}: {e}",
                    dir.display()
                );
            }
        }

        info!("Unfollowed library {library_id}");
        Ok(())
    }

    /// Get the list of followed libraries.
    ///
    /// Reloads config from disk to pick up any changes made during this session.
    pub fn get_followed_libraries(&self) -> Vec<BridgeFollowedLibrary> {
        let config = Config::load();
        config
            .followed_libraries
            .iter()
            .map(|fl| BridgeFollowedLibrary {
                id: fl.id.clone(),
                name: fl.name.clone(),
                url: fl.proxy_url.clone(),
            })
            .collect()
    }

    /// Read the membership chain from the sync bucket and return the current members.
    /// Returns an empty list if sync is not configured or no membership chain exists.
    pub fn get_members(&self) -> Result<Vec<BridgeMember>, BridgeError> {
        // Membership requires a sync bucket. Without one, there are no members to show.
        if !self.config.sync_enabled(&self.key_service) {
            return Ok(Vec::new());
        }

        self.runtime.block_on(async {
            let bucket =
                create_bucket_client(&self.config, &self.key_service, &self.encryption_service)
                    .await
                    .map_err(|e| BridgeError::Config {
                        msg: format!("Failed to create bucket client: {e}"),
                    })?;

            let entry_keys =
                bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to list membership entries: {e}"),
                    })?;

            if entry_keys.is_empty() {
                return Ok(Vec::new());
            }

            let mut raw_entries = Vec::new();
            for (author, seq) in &entry_keys {
                let data = bucket
                    .get_membership_entry(author, *seq)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to get membership entry {author}/{seq}: {e}"),
                    })?;

                let entry: bae_core::sync::membership::MembershipEntry =
                    serde_json::from_slice(&data).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to parse membership entry: {e}"),
                    })?;
                raw_entries.push(entry);
            }

            let chain = bae_core::sync::membership::MembershipChain::from_entries(raw_entries)
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Invalid membership chain: {e}"),
                })?;

            let user_pubkey = self.key_service.get_user_public_key().map(hex::encode);

            let current = chain.current_members();
            let members = current
                .into_iter()
                .map(|(pubkey, role)| {
                    let role_str = match role {
                        bae_core::sync::membership::MemberRole::Owner => "owner".to_string(),
                        bae_core::sync::membership::MemberRole::Member => "member".to_string(),
                    };
                    let name = if user_pubkey.as_deref() == Some(&pubkey) {
                        Some("You".to_string())
                    } else {
                        None
                    };
                    BridgeMember {
                        pubkey,
                        role: role_str,
                        added_by: None,
                        name,
                    }
                })
                .collect();

            Ok(members)
        })
    }

    /// Invite a member to the shared library.
    ///
    /// Downloads the membership chain (bootstrapping a founder entry if needed),
    /// creates a signed Add entry, wraps the encryption key to the invitee's
    /// public key, and uploads everything to the sync bucket.
    ///
    /// Returns the invite code string that should be shared with the invitee.
    pub fn invite_member(
        &self,
        public_key_hex: String,
        role: String,
    ) -> Result<String, BridgeError> {
        use bae_core::sync::membership::{
            sign_membership_entry, MemberRole as CoreMemberRole, MembershipAction, MembershipChain,
            MembershipEntry,
        };

        let sync_handle = self
            .sync_handle
            .as_ref()
            .ok_or_else(|| BridgeError::Config {
                msg: "Sync is not configured".to_string(),
            })?;

        let user_pubkey_hex = hex::encode(sync_handle.user_keypair.public_key);

        if public_key_hex == user_pubkey_hex {
            return Err(BridgeError::Config {
                msg: "Cannot invite yourself".to_string(),
            });
        }

        let encryption_key_hex =
            self.key_service
                .get_encryption_key()
                .ok_or_else(|| BridgeError::Config {
                    msg: "Encryption key not configured".to_string(),
                })?;

        let core_role = match role.as_str() {
            "owner" => CoreMemberRole::Owner,
            _ => CoreMemberRole::Member,
        };

        self.runtime.block_on(async {
            let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;
            let cloud_home = sync_handle.bucket_client.cloud_home();

            // Parse the encryption key from hex.
            let key_bytes: [u8; 32] = hex::decode(&encryption_key_hex)
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Invalid encryption key hex: {e}"),
                })?
                .try_into()
                .map_err(|_| BridgeError::Internal {
                    msg: "Encryption key wrong length".to_string(),
                })?;

            // Download existing membership entries.
            let entry_keys =
                bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to list membership entries: {e}"),
                    })?;

            let mut chain = if entry_keys.is_empty() {
                // No membership chain yet -- bootstrap with a founder entry.
                let mut founder = MembershipEntry {
                    action: MembershipAction::Add,
                    user_pubkey: user_pubkey_hex.clone(),
                    role: CoreMemberRole::Owner,
                    timestamp: sync_handle.hlc.now().to_string(),
                    author_pubkey: String::new(),
                    signature: String::new(),
                };

                sign_membership_entry(&mut founder, &sync_handle.user_keypair);

                let mut chain = MembershipChain::new();
                chain
                    .add_entry(founder.clone())
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to create founder entry: {e}"),
                    })?;

                // Upload the founder entry to the bucket.
                let founder_bytes =
                    serde_json::to_vec(&founder).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to serialize founder entry: {e}"),
                    })?;
                bucket
                    .put_membership_entry(&user_pubkey_hex, 1, founder_bytes)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to upload founder entry: {e}"),
                    })?;

                info!("Bootstrapped membership chain with founder entry");

                chain
            } else {
                // Build chain from existing entries.
                let mut raw_entries = Vec::new();
                for (author, seq) in &entry_keys {
                    let data = bucket
                        .get_membership_entry(author, *seq)
                        .await
                        .map_err(|e| BridgeError::Internal {
                            msg: format!("Failed to get membership entry {author}/{seq}: {e}"),
                        })?;
                    let entry: MembershipEntry =
                        serde_json::from_slice(&data).map_err(|e| BridgeError::Internal {
                            msg: format!("Failed to parse membership entry {author}/{seq}: {e}"),
                        })?;
                    raw_entries.push(entry);
                }

                MembershipChain::from_entries(raw_entries).map_err(|e| BridgeError::Internal {
                    msg: format!("Invalid membership chain: {e}"),
                })?
            };

            // Create the invitation.
            let invite_ts = sync_handle.hlc.now().to_string();
            let join_info = bae_core::sync::invite::create_invitation(
                bucket,
                cloud_home,
                &mut chain,
                &sync_handle.user_keypair,
                &public_key_hex,
                core_role,
                &key_bytes,
                &invite_ts,
            )
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to create invitation: {e}"),
            })?;

            info!(
                "Invited member {}...",
                &public_key_hex[..public_key_hex.len().min(16)]
            );

            // Encode the invite code.
            let invite_code = bae_core::join_code::InviteCode {
                library_id: self.config.library_id.clone(),
                library_name: self.config.library_name.clone().unwrap_or_default(),
                join_info,
                owner_pubkey: user_pubkey_hex,
            };

            Ok(bae_core::join_code::encode(&invite_code))
        })
    }

    /// Remove a member from the shared library.
    ///
    /// Downloads the membership chain, creates a signed Remove entry, rotates
    /// the encryption key, re-wraps it for remaining members, and persists the
    /// new key to the keyring and config.
    pub fn remove_member(&self, public_key_hex: String) -> Result<(), BridgeError> {
        use bae_core::sync::membership::{MembershipChain, MembershipEntry};

        let sync_handle = self
            .sync_handle
            .as_ref()
            .ok_or_else(|| BridgeError::Config {
                msg: "Sync is not configured".to_string(),
            })?;

        self.runtime.block_on(async {
            let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;
            let cloud_home = sync_handle.bucket_client.cloud_home();

            // Download existing membership entries and build the chain.
            let entry_keys =
                bucket
                    .list_membership_entries()
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to list membership entries: {e}"),
                    })?;

            if entry_keys.is_empty() {
                return Err(BridgeError::Internal {
                    msg: "No membership chain exists".to_string(),
                });
            }

            let mut raw_entries = Vec::new();
            for (author, seq) in &entry_keys {
                let data = bucket
                    .get_membership_entry(author, *seq)
                    .await
                    .map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to get membership entry {author}/{seq}: {e}"),
                    })?;
                let entry: MembershipEntry =
                    serde_json::from_slice(&data).map_err(|e| BridgeError::Internal {
                        msg: format!("Failed to parse membership entry {author}/{seq}: {e}"),
                    })?;
                raw_entries.push(entry);
            }

            let mut chain =
                MembershipChain::from_entries(raw_entries).map_err(|e| BridgeError::Internal {
                    msg: format!("Invalid membership chain: {e}"),
                })?;

            // Revoke the member.
            let revoke_ts = sync_handle.hlc.now().to_string();
            let new_key = bae_core::sync::invite::revoke_member(
                bucket,
                cloud_home,
                &mut chain,
                &sync_handle.user_keypair,
                &public_key_hex,
                &revoke_ts,
            )
            .await
            .map_err(|e| BridgeError::Internal {
                msg: format!("Failed to revoke member: {e}"),
            })?;

            // Persist the new encryption key to keyring.
            let new_key_hex = hex::encode(new_key);
            self.key_service
                .set_encryption_key(&new_key_hex)
                .map_err(|e| BridgeError::Internal {
                    msg: format!("Failed to persist new encryption key: {e}"),
                })?;

            // Update the shared encryption service.
            {
                let mut enc = sync_handle.encryption.write().unwrap();
                *enc = EncryptionService::from_key(new_key);
            }

            // Update config fingerprint and persist.
            let new_fingerprint = {
                let enc = sync_handle.encryption.read().unwrap();
                enc.fingerprint()
            };
            let mut updated_config = self.config.clone();
            updated_config.encryption_key_fingerprint = Some(new_fingerprint);
            if let Err(e) = updated_config.save_to_config_yaml() {
                warn!("Failed to save config after key rotation: {e}");
            }

            info!(
                "Revoked member {}... and rotated encryption key",
                &public_key_hex[..public_key_hex.len().min(16)]
            );

            Ok(())
        })
    }

    // =========================================================================
    // Sync trigger / loop
    // =========================================================================

    /// Run a single sync cycle: push local changes, pull remote changes.
    /// Returns the updated sync status. Notifies the event handler on completion
    /// (on_library_changed if remote changes were applied, on_sync_status_changed always).
    pub fn trigger_sync(&self) -> Result<BridgeSyncStatus, BridgeError> {
        let sync_handle = self
            .sync_handle
            .as_ref()
            .ok_or_else(|| BridgeError::Config {
                msg: "Sync not configured".to_string(),
            })?;

        self.runtime.block_on(async {
            let db = self.library_manager.get().database();
            let result = run_single_sync_cycle(sync_handle, db, &self.config.library_dir).await?;

            // Notify event handler
            if let Some(handler) = self.event_handler.lock().unwrap().as_ref() {
                handler.on_sync_status_changed(result.status.clone());
                if result.changesets_applied > 0 {
                    handler.on_library_changed();
                }
            }

            Ok(result.status)
        })
    }

    /// Start a background sync loop that runs every 30 seconds.
    /// No-op if sync is not configured or a loop is already running.
    ///
    /// Spawns a dedicated OS thread with its own tokio runtime because
    /// the sync session holds a raw sqlite3 pointer (not Send across
    /// tokio task boundaries).
    pub fn start_sync_loop(&self) {
        let Some(sync_handle) = self.sync_handle.clone() else {
            return;
        };

        // Don't start a second loop (if a handle exists, thread is running)
        {
            let guard = self.sync_loop_handle.lock().unwrap();
            if guard.is_some() {
                return;
            }
        }

        let library_manager = self.library_manager.clone();
        let library_dir = self.config.library_dir.clone();
        let event_handler = self.event_handler.lock().unwrap().clone();

        let handle = std::thread::Builder::new()
            .name("bae-sync-loop".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        warn!("Failed to create sync loop runtime: {e}");
                        return;
                    }
                };

                rt.block_on(async {
                    // Short delay to avoid racing with app startup
                    tokio::time::sleep(Duration::from_secs(3)).await;

                    loop {
                        if let Some(ref handler) = event_handler {
                            let db = library_manager.get().database();

                            match run_single_sync_cycle(&sync_handle, db, &library_dir).await {
                                Ok(result) => {
                                    handler.on_sync_status_changed(result.status);
                                    if result.changesets_applied > 0 {
                                        handler.on_library_changed();
                                    }
                                }
                                Err(e) => {
                                    let status = BridgeSyncStatus {
                                        configured: true,
                                        syncing: false,
                                        last_sync_time: None,
                                        error: Some(e.to_string()),
                                        device_count: 0,
                                    };
                                    handler.on_sync_status_changed(status);
                                }
                            }
                        }

                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                });
            })
            .expect("Failed to spawn sync loop thread");

        let mut guard = self.sync_loop_handle.lock().unwrap();
        *guard = Some(handle);
    }

    /// Stop the background sync loop if running.
    pub fn stop_sync_loop(&self) {
        // The sync loop thread will stop naturally when the process exits.
        // We just clear our reference to it.
        let mut guard = self.sync_loop_handle.lock().unwrap();
        guard.take();
    }

    /// Whether sync infrastructure is initialized and ready to use.
    pub fn is_sync_ready(&self) -> bool {
        self.sync_handle.is_some()
    }
}

/// Background task that reads PlaybackProgress events and dispatches them
/// to the Swift event handler callback.
async fn dispatch_playback_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    handler: Arc<dyn AppEventHandler>,
    library_manager: SharedLibraryManager,
) {
    while let Some(event) = rx.recv().await {
        match event {
            PlaybackProgress::StateChanged { state } => {
                let bridge_state = convert_playback_state(&state, &library_manager).await;
                handler.on_playback_state_changed(bridge_state);
            }
            PlaybackProgress::PositionUpdate {
                position, track_id, ..
            } => {
                handler.on_playback_progress(
                    position.as_millis() as u64,
                    0, // duration comes from state changes, not position updates
                    track_id,
                );
            }
            PlaybackProgress::QueueUpdated { tracks } => {
                handler.on_queue_updated(tracks);
            }
            PlaybackProgress::PlaybackError { message } => {
                handler.on_error(message);
            }
            // Other events (Seeked, SeekError, TrackCompleted, etc.) don't need
            // separate handling -- state changes cover the UI updates.
            _ => {}
        }
    }
}

/// Background task that reads folder scan events and forwards candidates to Swift.
async fn dispatch_scan_events(
    mut rx: tokio::sync::broadcast::Receiver<ScanEvent>,
    handler: Arc<dyn AppEventHandler>,
) {
    use bae_core::import::folder_scanner::AudioContent;

    while let Ok(event) = rx.recv().await {
        match event {
            ScanEvent::Candidate(candidate) => {
                let (track_count, format) = match &candidate.files.audio {
                    AudioContent::CueFlacPairs(pairs) => {
                        let count: usize = pairs.iter().map(|p| p.track_count).sum();
                        (count as u32, "CUE+FLAC".to_string())
                    }
                    AudioContent::TrackFiles(tracks) => (tracks.len() as u32, "FLAC".to_string()),
                };

                let total_size_bytes = match &candidate.files.audio {
                    AudioContent::CueFlacPairs(pairs) => pairs
                        .iter()
                        .map(|p| p.audio_file.size + p.cue_file.size)
                        .sum(),
                    AudioContent::TrackFiles(tracks) => tracks.iter().map(|t| t.size).sum(),
                };

                let folder_contents =
                    bae_core::import::detect_folder_contents(candidate.path.clone()).ok();
                let metadata = folder_contents.as_ref().map(|fc| &fc.metadata);

                handler.on_scan_result(BridgeImportCandidate {
                    folder_path: candidate.path.to_string_lossy().to_string(),
                    artist_name: metadata.and_then(|m| m.artist.clone()).unwrap_or_default(),
                    album_title: candidate.name,
                    track_count,
                    format,
                    total_size_bytes,
                    bad_audio_count: candidate.files.bad_audio_count as u32,
                    bad_image_count: candidate.files.bad_image_count as u32,
                    mb_discid: metadata.and_then(|m| m.mb_discid.clone()),
                });
            }
            ScanEvent::Error(msg) => {
                handler.on_error(msg);
            }
            ScanEvent::Finished => {
                // No specific callback for scan finished; UI can track this
                // via the absence of new candidates.
            }
        }
    }
}

/// Background task that reads import progress events and forwards them to Swift.
///
/// The import core uses random UUIDs as import_id, but the Swift UI keys status
/// by folder_path. The `id_map` translates between the two so events reach the UI.
async fn dispatch_import_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ImportProgress>,
    handler: Arc<dyn AppEventHandler>,
    id_map: Arc<Mutex<HashMap<String, String>>>,
) {
    while let Some(event) = rx.recv().await {
        match event {
            ImportProgress::Preparing {
                import_id,
                step,
                album_title,
                ..
            } => {
                let folder = id_map.lock().unwrap().get(&import_id).cloned();
                if let Some(folder) = folder {
                    handler.on_import_progress(
                        folder,
                        BridgeImportStatus::Importing {
                            progress_percent: 0,
                        },
                    );
                }

                info!(
                    "Import preparing: {} - {}",
                    album_title,
                    step.display_text()
                );
            }
            ImportProgress::Started {
                import_id: Some(iid),
                ..
            } => {
                let folder = id_map.lock().unwrap().get(&iid).cloned();
                if let Some(folder) = folder {
                    handler.on_import_progress(
                        folder,
                        BridgeImportStatus::Importing {
                            progress_percent: 0,
                        },
                    );
                }
            }
            ImportProgress::Progress {
                id,
                percent,
                import_id: Some(iid),
                ..
            } => {
                // Only forward release-level progress, not per-track
                if id != iid {
                    let folder = id_map.lock().unwrap().get(&iid).cloned();
                    if let Some(folder) = folder {
                        handler.on_import_progress(
                            folder,
                            BridgeImportStatus::Importing {
                                progress_percent: percent as u32,
                            },
                        );
                    }
                }
            }
            ImportProgress::Complete {
                release_id: None,
                import_id: Some(iid),
                ..
            } => {
                let folder = id_map.lock().unwrap().remove(&iid);
                if let Some(folder) = folder {
                    handler.on_import_progress(folder, BridgeImportStatus::Complete);
                }
                handler.on_library_changed();
            }
            ImportProgress::Failed {
                error,
                import_id: Some(iid),
                ..
            } => {
                let folder = id_map.lock().unwrap().remove(&iid);
                if let Some(folder) = folder {
                    handler
                        .on_import_progress(folder, BridgeImportStatus::Error { message: error });
                }
            }
            _ => {}
        }
    }
}

/// Convert a core PlaybackState into a BridgePlaybackState, looking up
/// track metadata (title, artist names, album_id) from the database.
async fn convert_playback_state(
    state: &PlaybackState,
    lm: &SharedLibraryManager,
) -> BridgePlaybackState {
    match state {
        PlaybackState::Stopped => BridgePlaybackState::Stopped,
        PlaybackState::Loading { track_id } => BridgePlaybackState::Loading {
            track_id: track_id.clone(),
        },
        PlaybackState::Playing {
            track,
            position,
            duration,
            decoded_duration,
            ..
        } => {
            let (artist_id, artist_names, album_id, album_title, cover_image_id) =
                resolve_track_info(lm, &track.id).await;
            let dur = duration.unwrap_or(*decoded_duration).as_millis() as u64;
            BridgePlaybackState::Playing {
                track_id: track.id.clone(),
                track_title: track.title.clone(),
                artist_names,
                artist_id,
                album_id,
                album_title,
                cover_image_id,
                position_ms: position.as_millis() as u64,
                duration_ms: dur,
            }
        }
        PlaybackState::Paused {
            track,
            position,
            duration,
            decoded_duration,
            ..
        } => {
            let (artist_id, artist_names, album_id, album_title, cover_image_id) =
                resolve_track_info(lm, &track.id).await;
            let dur = duration.unwrap_or(*decoded_duration).as_millis() as u64;
            BridgePlaybackState::Paused {
                track_id: track.id.clone(),
                track_title: track.title.clone(),
                artist_names,
                artist_id,
                album_id,
                album_title,
                cover_image_id,
                position_ms: position.as_millis() as u64,
                duration_ms: dur,
            }
        }
    }
}

/// Look up artist_id, artist names, album_id, album_title, and cover_image_id for a track.
async fn resolve_track_info(
    lm: &SharedLibraryManager,
    track_id: &str,
) -> (Option<String>, String, String, String, Option<String>) {
    let mgr = lm.get();

    let (album_id, album_title, cover_image_id) = match mgr.get_album_id_for_track(track_id).await {
        Ok(id) => {
            let (title, cover) = match mgr.get_album_by_id(&id).await {
                Ok(Some(album)) => (album.title, album.cover_release_id),
                _ => (String::new(), None),
            };
            (id, title, cover)
        }
        Err(e) => {
            warn!("Failed to get album_id for track {track_id}: {e}");
            (String::new(), String::new(), None)
        }
    };

    let (artist_id, artist_names) = match mgr.get_artists_for_track(track_id).await {
        Ok(artists) if !artists.is_empty() => {
            let id = artists.first().map(|a| a.id.clone());
            let names = artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (id, names)
        }
        _ => {
            // Fall back to album artists
            match mgr.get_artists_for_album(&album_id).await {
                Ok(artists) if !artists.is_empty() => {
                    let id = artists.first().map(|a| a.id.clone());
                    let names = artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    (id, names)
                }
                _ => (None, String::new()),
            }
        }
    };

    (
        artist_id,
        artist_names,
        album_id,
        album_title,
        cover_image_id,
    )
}

/// Write cover image bytes to disk and update the database.
#[allow(clippy::too_many_arguments)]
async fn write_cover_and_update_db(
    lm: &bae_core::library::LibraryManager,
    library_dir: &bae_core::library_dir::LibraryDir,
    album_id: &str,
    release_id: &str,
    bytes: &[u8],
    content_type: bae_core::content_type::ContentType,
    source: &str,
    source_url: Option<String>,
) -> Result<(), BridgeError> {
    let cover_path = library_dir.image_path(release_id);
    if let Some(parent) = cover_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BridgeError::Internal {
            msg: format!("Failed to create images dir: {e}"),
        })?;
    }
    std::fs::write(&cover_path, bytes).map_err(|e| BridgeError::Internal {
        msg: format!("Failed to write cover: {e}"),
    })?;

    let library_image = bae_core::db::DbLibraryImage {
        id: release_id.to_string(),
        image_type: bae_core::db::LibraryImageType::Cover,
        content_type,
        file_size: bytes.len() as i64,
        width: None,
        height: None,
        source: source.to_string(),
        source_url,
        updated_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
    };
    lm.upsert_library_image(&library_image)
        .await
        .map_err(|e| BridgeError::Database {
            msg: format!("{e}"),
        })?;

    lm.set_album_cover_release(album_id, release_id)
        .await
        .map_err(|e| BridgeError::Database {
            msg: format!("{e}"),
        })?;

    Ok(())
}

/// Parse a Discogs-style duration string ("3:45") to milliseconds.
fn parse_duration_to_ms(duration: &str) -> Option<u64> {
    let parts: Vec<&str> = duration.split(':').collect();
    match parts.len() {
        2 => {
            let mins: u64 = parts[0].parse().ok()?;
            let secs: u64 = parts[1].parse().ok()?;
            Some((mins * 60 + secs) * 1000)
        }
        3 => {
            let hours: u64 = parts[0].parse().ok()?;
            let mins: u64 = parts[1].parse().ok()?;
            let secs: u64 = parts[2].parse().ok()?;
            Some((hours * 3600 + mins * 60 + secs) * 1000)
        }
        _ => None,
    }
}

/// Find the S3 key for a release's cover image.
async fn find_cover_image_key(db: &Database, release_id: &str) -> Option<String> {
    let image = db
        .get_library_image(release_id, &bae_core::db::LibraryImageType::Cover)
        .await
        .ok()??;
    let hex = image.id.replace('-', "");
    Some(format!("images/{}/{}/{}", &hex[..2], &hex[2..4], &image.id))
}

/// Initialize the app with a specific library.
/// Call discover_libraries() first to find available libraries.
#[uniffi::export]
pub fn init_app(library_id: String) -> Result<Arc<AppHandle>, BridgeError> {
    // Find the library among discovered libraries
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| BridgeError::NotFound {
            msg: format!("Library '{library_id}' not found"),
        })?;

    // Load the full config for this library.
    // Write the active-library pointer so Config::load() finds it.
    let home_dir = dirs::home_dir().ok_or_else(|| BridgeError::Config {
        msg: "Failed to get home directory".to_string(),
    })?;
    let bae_dir = home_dir.join(".bae");
    std::fs::create_dir_all(&bae_dir).map_err(|e| BridgeError::Config {
        msg: format!("Failed to create .bae directory: {e}"),
    })?;
    std::fs::write(bae_dir.join("active-library"), &lib_info.id).map_err(|e| {
        BridgeError::Config {
            msg: format!("Failed to write active-library pointer: {e}"),
        }
    })?;

    let config = Config::load();

    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| BridgeError::Internal {
        msg: format!("Failed to create runtime: {e}"),
    })?;

    // Initialize FFmpeg
    bae_core::audio_codec::init();

    // Initialize keyring
    bae_core::config::init_keyring();

    // Create database
    let db_path = config.library_dir.db_path();
    let database = runtime
        .block_on(Database::new(db_path.to_str().unwrap()))
        .map_err(|e| BridgeError::Database {
            msg: format!("Failed to open database: {e}"),
        })?;

    // Create key service
    let dev_mode = Config::is_dev_mode();
    let key_service = KeyService::new(dev_mode, config.library_id.clone());

    // Create encryption service if configured
    let encryption_service = if config.encryption_key_stored {
        key_service
            .get_encryption_key()
            .and_then(|key| bae_core::encryption::EncryptionService::new(&key).ok())
    } else {
        None
    };

    // Create library manager
    let library_manager =
        bae_core::library::LibraryManager::new(database.clone(), encryption_service.clone());
    let shared_library = SharedLibraryManager::new(library_manager);

    // Start import service
    let import_handle = ImportService::start(
        runtime.handle().clone(),
        shared_library.clone(),
        encryption_service.clone(),
        Arc::new(database.clone()),
        key_service.clone(),
        config.library_dir.clone(),
    );

    // Start playback service (needs an owned LibraryManager clone)
    let playback_handle = PlaybackService::start(
        shared_library.get().clone(),
        encryption_service.clone(),
        config.library_dir.clone(),
        runtime.handle().clone(),
    );

    // Start image server
    let image_server = runtime.block_on(image_server::start_image_server(
        shared_library.clone(),
        config.library_dir.clone(),
        encryption_service.clone(),
        "127.0.0.1",
    ));

    // Try to create cloud home (non-fatal if not configured)
    let cloud_home: Option<Arc<dyn CloudHome>> = match runtime.block_on(
        bae_core::cloud_home::create_cloud_home(&config, &key_service),
    ) {
        Ok(ch) => Some(Arc::from(ch)),
        Err(e) => {
            info!("Cloud home not available: {e}");
            None
        }
    };

    // Initialize sync infrastructure if sync is configured and encryption is enabled
    let sync_handle = if config.sync_enabled(&key_service) {
        if let Some(ref enc) = encryption_service {
            runtime.block_on(create_sync_handle(&config, &key_service, &database, enc))
        } else {
            info!("Sync is configured but encryption is not enabled, skipping sync initialization");
            None
        }
    } else {
        None
    };

    info!("AppHandle initialized for library '{library_id}'");

    Ok(Arc::new(AppHandle {
        runtime,
        config,
        library_manager: shared_library,
        key_service,
        encryption_service,
        cloud_home,
        image_server,
        playback_handle,
        import_handle,
        event_handler: Mutex::new(None),
        subsonic_server_handle: Mutex::new(None),
        sync_handle,
        sync_loop_handle: Mutex::new(None),
        import_id_to_folder: Arc::new(Mutex::new(HashMap::new())),
    }))
}

/// Create a temporary bucket client for reading membership data.
///
/// This constructs a CloudHomeSyncBucket from the current config and keyring
/// credentials. Used by get_members() which only needs read access.
async fn create_bucket_client(
    config: &Config,
    key_service: &KeyService,
    encryption_service: &Option<EncryptionService>,
) -> Result<impl SyncBucketClient, String> {
    use bae_core::cloud_home::s3::S3CloudHome;
    use bae_core::sync::cloud_home_bucket::CloudHomeSyncBucket;

    let bucket = config
        .cloud_home_s3_bucket
        .as_ref()
        .ok_or("No S3 bucket configured")?;
    let region = config
        .cloud_home_s3_region
        .as_ref()
        .ok_or("No S3 region configured")?;
    let endpoint = config.cloud_home_s3_endpoint.clone();

    let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
        Some(bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        }) => (access_key, secret_key),
        _ => return Err("No S3 credentials found in keyring".to_string()),
    };

    let encryption = match encryption_service {
        Some(enc) => enc.clone(),
        None => {
            let key = key_service
                .get_encryption_key()
                .ok_or("No encryption key found")?;
            EncryptionService::new(&key)
                .map_err(|e| format!("Failed to create encryption service: {e}"))?
        }
    };

    let key_prefix = config.cloud_home_s3_key_prefix.clone();
    let cloud_home = S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint,
        access_key,
        secret_key,
        key_prefix,
    )
    .await
    .map_err(|e| format!("Failed to create S3 cloud home: {e}"))?;

    Ok(CloudHomeSyncBucket::new(Box::new(cloud_home), encryption))
}

/// Create the sync handle if sync bucket credentials and configuration are available.
///
/// Extracts the raw sqlite3 write handle, creates the S3 bucket client, HLC,
/// user keypair, and starts the initial sync session.
async fn create_sync_handle(
    config: &Config,
    key_service: &KeyService,
    database: &Database,
    encryption: &EncryptionService,
) -> Option<Arc<BridgeSyncHandle>> {
    use bae_core::cloud_home::s3::S3CloudHome;

    let bucket = config.cloud_home_s3_bucket.as_ref()?;
    let region = config.cloud_home_s3_region.as_ref()?;
    let endpoint = config.cloud_home_s3_endpoint.clone();

    let (access_key, secret_key) = match key_service.get_cloud_home_credentials() {
        Some(bae_core::keys::CloudHomeCredentials::S3 {
            access_key,
            secret_key,
        }) => (access_key, secret_key),
        _ => {
            warn!("Sync configured but no S3 credentials found");
            return None;
        }
    };

    let key_prefix = config.cloud_home_s3_key_prefix.clone();
    let cloud_home = match S3CloudHome::new(
        bucket.clone(),
        region.clone(),
        endpoint,
        access_key,
        secret_key,
        key_prefix,
    )
    .await
    {
        Ok(ch) => ch,
        Err(e) => {
            warn!("Failed to create cloud home for sync: {e}");
            return None;
        }
    };

    let bucket_client = CloudHomeSyncBucket::new(Box::new(cloud_home), encryption.clone());
    let encryption_lock = bucket_client.shared_encryption();

    let raw_db = match database.raw_write_handle().await {
        Ok(ptr) => ptr,
        Err(e) => {
            warn!("Failed to extract raw write handle for sync: {e}");
            return None;
        }
    };

    let session = match unsafe { SyncSession::start(raw_db) } {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to start initial sync session: {e}");
            return None;
        }
    };

    let user_keypair = match key_service.get_or_create_user_keypair() {
        Ok(kp) => kp,
        Err(e) => {
            warn!("Failed to get/create user keypair for sync: {e}");
            return None;
        }
    };

    let hlc = Hlc::new(config.device_id.clone());

    info!(
        "Sync initialized (bucket: {}, device: {})",
        bucket, config.device_id
    );

    Some(Arc::new(BridgeSyncHandle {
        bucket_client: Arc::new(bucket_client),
        hlc: Arc::new(hlc),
        device_id: config.device_id.clone(),
        encryption: encryption_lock,
        raw_db,
        session: tokio::sync::Mutex::new(Some(session)),
        user_keypair,
    }))
}

/// Path for staging outgoing changeset bytes that survived a push failure.
fn staging_path(library_dir: &LibraryDir) -> PathBuf {
    library_dir.join("sync_staging.bin")
}

/// Stage outgoing changeset bytes to disk before pushing.
fn stage_changeset(library_dir: &LibraryDir, packed: &[u8]) {
    if let Err(e) = std::fs::write(staging_path(library_dir), packed) {
        warn!("Failed to stage outgoing changeset: {e}");
    }
}

/// Clear the staged changeset after a successful push.
fn clear_staged_changeset(library_dir: &LibraryDir) {
    let _ = std::fs::remove_file(staging_path(library_dir));
}

/// Read a previously staged changeset (if any) for retry.
fn read_staged_changeset(library_dir: &LibraryDir) -> Option<Vec<u8>> {
    let path = staging_path(library_dir);
    if path.exists() {
        match std::fs::read(&path) {
            Ok(data) if !data.is_empty() => Some(data),
            Ok(_) => {
                clear_staged_changeset(library_dir);
                None
            }
            Err(e) => {
                warn!("Failed to read staged changeset: {e}");
                clear_staged_changeset(library_dir);
                None
            }
        }
    } else {
        None
    }
}

/// Push a changeset to the sync bucket and update the device head.
async fn push_changeset(
    bucket: &dyn SyncBucketClient,
    device_id: &str,
    seq: u64,
    packed: Vec<u8>,
    snapshot_seq: Option<u64>,
    timestamp: &str,
) -> Result<(), bae_core::sync::bucket::BucketError> {
    bucket.put_changeset(device_id, seq, packed).await?;
    bucket
        .put_head(device_id, seq, snapshot_seq, timestamp)
        .await?;
    Ok(())
}

/// Result of a single sync cycle, for internal use.
struct SyncCycleResult {
    status: BridgeSyncStatus,
    changesets_applied: u64,
}

/// Run a single sync cycle: grab changeset, push, pull, restart session.
///
/// This is the bridge equivalent of bae-desktop's `run_sync_cycle`. It manages
/// all the state (local_seq, cursors, staging, snapshots) by loading/persisting
/// from the database each cycle, rather than keeping mutable state across calls.
async fn run_single_sync_cycle(
    sync_handle: &BridgeSyncHandle,
    db: &Database,
    library_dir: &LibraryDir,
) -> Result<SyncCycleResult, BridgeError> {
    let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;
    let device_id = &sync_handle.device_id;
    let hlc = &sync_handle.hlc;
    let sync_service = SyncService::new(device_id.clone());

    // Load persisted sync state
    let mut local_seq = db
        .get_sync_state("local_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let snapshot_seq: Option<u64> = db
        .get_sync_state("snapshot_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    let last_snapshot_time: Option<chrono::DateTime<chrono::Utc>> = db
        .get_sync_state("last_snapshot_time")
        .await
        .ok()
        .flatten()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let staged_seq: Option<u64> = db
        .get_sync_state("staged_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    // Retry any staged changeset from a previous failed push
    if let Some(seq) = staged_seq {
        if let Some(staged_data) = read_staged_changeset(library_dir) {
            let timestamp = hlc.now().to_string();

            info!(seq, "Retrying staged changeset push");

            match push_changeset(
                bucket,
                device_id,
                seq,
                staged_data,
                snapshot_seq,
                &timestamp,
            )
            .await
            {
                Ok(()) => {
                    info!(seq, "Staged changeset push succeeded");
                    clear_staged_changeset(library_dir);
                    local_seq = seq;
                    let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                    let _ = db.set_sync_state("staged_seq", "").await;
                }
                Err(e) => {
                    return Err(BridgeError::Internal {
                        msg: format!("Staged changeset push failed: {e}"),
                    });
                }
            }
        } else {
            let _ = db.set_sync_state("staged_seq", "").await;
        }
    }

    // Load current cursors from DB
    let cursors = db
        .get_all_sync_cursors()
        .await
        .map_err(|e| BridgeError::Database {
            msg: format!("Failed to load sync cursors: {e}"),
        })?;

    // Take the current session
    let session = match sync_handle.session.lock().await.take() {
        Some(s) => s,
        None => {
            warn!("Sync session was None, creating a new one");
            unsafe { SyncSession::start(sync_handle.raw_db) }.map_err(|e| {
                BridgeError::Internal {
                    msg: format!("Failed to create replacement sync session: {e}"),
                }
            })?
        }
    };

    let timestamp = hlc.now().to_string();

    // Run the core sync cycle
    let sync_result = unsafe {
        sync_service
            .sync(
                sync_handle.raw_db,
                session,
                local_seq,
                &cursors,
                bucket,
                &timestamp,
                "background sync",
                &sync_handle.user_keypair,
                None,
                library_dir,
            )
            .await
    };

    let sync_result = match sync_result {
        Ok(r) => r,
        Err(e) => {
            // Try to restart the session even if the cycle failed
            match unsafe { SyncSession::start(sync_handle.raw_db) } {
                Ok(new_session) => {
                    *sync_handle.session.lock().await = Some(new_session);
                }
                Err(session_err) => {
                    warn!("Failed to restart sync session after error: {session_err}");
                }
            }
            return Err(BridgeError::Internal {
                msg: format!("Sync cycle error: {e}"),
            });
        }
    };

    // Handle outgoing changeset (push)
    if let Some(outgoing) = &sync_result.outgoing {
        let seq = outgoing.seq;

        // Stage before pushing so bytes survive a push failure
        stage_changeset(library_dir, &outgoing.packed);
        let _ = db.set_sync_state("staged_seq", &seq.to_string()).await;

        match push_changeset(
            bucket,
            device_id,
            seq,
            outgoing.packed.clone(),
            snapshot_seq,
            &timestamp,
        )
        .await
        {
            Ok(()) => {
                clear_staged_changeset(library_dir);
                local_seq = seq;
                let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                let _ = db.set_sync_state("staged_seq", "").await;

                info!(seq, "Pushed changeset");
            }
            Err(e) => {
                warn!(seq, "Push failed, changeset staged for retry: {e}");
            }
        }
    }

    // Persist updated cursors
    for (cursor_device_id, cursor_seq) in &sync_result.updated_cursors {
        if let Err(e) = db.set_sync_cursor(cursor_device_id, *cursor_seq).await {
            warn!(
                device_id = cursor_device_id,
                seq = cursor_seq,
                "Failed to persist sync cursor: {e}"
            );
        }
    }

    // Update HLC with max remote timestamp
    let max_remote_ts = sync_result
        .pull
        .remote_heads
        .iter()
        .filter(|h| h.device_id != *device_id)
        .filter_map(|h| h.last_sync.as_deref())
        .filter_map(|ts_str| {
            chrono::DateTime::parse_from_rfc3339(ts_str)
                .ok()
                .map(|dt| dt.timestamp_millis().max(0) as u64)
        })
        .max();

    if let Some(remote_millis) = max_remote_ts {
        let remote_ts = bae_core::sync::hlc::Timestamp::new(remote_millis, 0, "remote".to_string());
        hlc.update(&remote_ts);
    }

    // Start a new sync session
    match unsafe { SyncSession::start(sync_handle.raw_db) } {
        Ok(new_session) => {
            *sync_handle.session.lock().await = Some(new_session);
        }
        Err(e) => {
            warn!("Failed to start new sync session: {e}");
            return Err(BridgeError::Internal {
                msg: format!("Failed to restart sync session: {e}"),
            });
        }
    }

    // Check snapshot policy
    let hours_since = last_snapshot_time.map(|t| {
        let elapsed = chrono::Utc::now().signed_duration_since(t);
        elapsed.num_hours().max(0) as u64
    });

    if bae_core::sync::snapshot::should_create_snapshot(local_seq, snapshot_seq, hours_since) {
        info!("Snapshot policy triggered, creating snapshot");

        let temp_dir = std::env::temp_dir();
        let snapshot_result = {
            let enc = sync_handle.encryption.read().unwrap();
            unsafe {
                bae_core::sync::snapshot::create_snapshot(sync_handle.raw_db, &temp_dir, &enc)
            }
        };

        match snapshot_result {
            Ok(encrypted) => {
                match bae_core::sync::snapshot::push_snapshot(
                    bucket, encrypted, device_id, local_seq,
                )
                .await
                {
                    Ok(()) => {
                        let _ = db
                            .set_sync_state("snapshot_seq", &local_seq.to_string())
                            .await;
                        let _ = db
                            .set_sync_state("last_snapshot_time", &chrono::Utc::now().to_rfc3339())
                            .await;

                        info!(local_seq, "Snapshot created and pushed");
                    }
                    Err(e) => {
                        warn!("Failed to push snapshot: {e}");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to create snapshot: {e}");
            }
        }
    }

    // Build status from remote heads
    let now = chrono::Utc::now().to_rfc3339();
    let core_status = bae_core::sync::status::build_sync_status(
        &sync_result.pull.remote_heads,
        device_id,
        Some(&now),
    );

    let device_count = (core_status.other_devices.len() + 1) as u32;

    Ok(SyncCycleResult {
        status: BridgeSyncStatus {
            configured: true,
            syncing: false,
            last_sync_time: Some(now),
            error: None,
            device_count,
        },
        changesets_applied: sync_result.pull.changesets_applied,
    })
}

// =========================================================================
// Cloud provider helpers
// =========================================================================

fn cloud_provider_to_string(p: &bae_core::config::CloudProvider) -> String {
    match p {
        bae_core::config::CloudProvider::S3 => "s3".to_string(),
        bae_core::config::CloudProvider::ICloud => "icloud".to_string(),
        bae_core::config::CloudProvider::GoogleDrive => "google_drive".to_string(),
        bae_core::config::CloudProvider::Dropbox => "dropbox".to_string(),
        bae_core::config::CloudProvider::OneDrive => "onedrive".to_string(),
        bae_core::config::CloudProvider::BaeCloud => "bae_cloud".to_string(),
    }
}

/// Derive a display string for the connected cloud account.
fn cloud_account_display_for(config: &Config, key_service: &KeyService) -> Option<String> {
    match config.cloud_provider.as_ref()? {
        bae_core::config::CloudProvider::BaeCloud => config.cloud_home_bae_cloud_username.clone(),
        bae_core::config::CloudProvider::ICloud => Some("iCloud Drive".to_string()),
        bae_core::config::CloudProvider::S3 => config
            .cloud_home_s3_bucket
            .as_ref()
            .map(|b| format!("s3://{b}")),
        bae_core::config::CloudProvider::GoogleDrive
        | bae_core::config::CloudProvider::Dropbox
        | bae_core::config::CloudProvider::OneDrive => {
            // For OAuth providers, we could extract the account from the token,
            // but that requires network calls. Return the provider name for now.
            match key_service.get_cloud_home_credentials() {
                Some(bae_core::keys::CloudHomeCredentials::OAuth { .. }) => {
                    Some("Connected".to_string())
                }
                _ => None,
            }
        }
    }
}

/// Google Drive OAuth sign-in: authorize, create/find folder, save tokens + config.
async fn sign_in_google_drive(
    key_service: &KeyService,
    app_config: &Config,
) -> Result<(), BridgeError> {
    let oauth_config = bae_core::cloud_home::google_drive::GoogleDriveCloudHome::oauth_config();
    let tokens = bae_core::oauth::authorize(&oauth_config)
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("Google Drive authorization failed: {e}"),
        })?;

    let client = reqwest::Client::new();

    // Create or find the folder
    let lib_name = app_config
        .library_name
        .clone()
        .unwrap_or_else(|| app_config.library_id.clone());
    let folder_name = format!("bae - {}", lib_name);

    let search_query = format!(
        "name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
        folder_name.replace('\'', "\\'")
    );
    let existing_folder_id = async {
        let resp = client
            .get("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(&tokens.access_token)
            .query(&[("q", &search_query), ("fields", &"files(id)".to_string())])
            .send()
            .await
            .ok()?;
        let json: serde_json::Value = resp.json().await.ok()?;
        json["files"][0]["id"].as_str().map(|s| s.to_string())
    }
    .await;

    let folder_id = if let Some(id) = existing_folder_id {
        id
    } else {
        let create_body = serde_json::json!({
            "name": folder_name,
            "mimeType": "application/vnd.google-apps.folder",
        });
        let resp = client
            .post("https://www.googleapis.com/drive/v3/files")
            .bearer_auth(&tokens.access_token)
            .json(&create_body)
            .send()
            .await
            .map_err(|e| BridgeError::Config {
                msg: format!("Failed to create Google Drive folder: {e}"),
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BridgeError::Config {
                msg: format!("Failed to create Google Drive folder: {body}"),
            });
        }

        let folder_resp: serde_json::Value =
            resp.json().await.map_err(|e| BridgeError::Config {
                msg: format!("Failed to parse folder response: {e}"),
            })?;
        folder_resp["id"]
            .as_str()
            .ok_or_else(|| BridgeError::Config {
                msg: "Google Drive folder response missing 'id'".to_string(),
            })?
            .to_string()
    };

    // Save tokens to keyring
    let token_json = serde_json::to_string(&tokens).map_err(|e| BridgeError::Config {
        msg: format!("Failed to serialize tokens: {e}"),
    })?;
    key_service
        .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::OAuth { token_json })
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to save OAuth token: {e}"),
        })?;

    // Save config
    let mut config = Config::load();
    config.cloud_provider = Some(bae_core::config::CloudProvider::GoogleDrive);
    config.cloud_home_google_drive_folder_id = Some(folder_id);
    config.save().map_err(|e| BridgeError::Config {
        msg: format!("Failed to save config: {e}"),
    })?;

    info!("Configured Google Drive cloud provider");
    Ok(())
}

/// Dropbox OAuth sign-in: authorize, create folder, save tokens + config.
async fn sign_in_dropbox(key_service: &KeyService, app_config: &Config) -> Result<(), BridgeError> {
    let oauth_config = bae_core::cloud_home::dropbox::DropboxCloudHome::oauth_config();
    let tokens = bae_core::oauth::authorize(&oauth_config)
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("Dropbox authorization failed: {e}"),
        })?;

    let client = reqwest::Client::new();

    let lib_name = app_config
        .library_name
        .clone()
        .unwrap_or_else(|| app_config.library_id.clone());
    let folder_path = format!("/Apps/bae/{}", lib_name);

    // Create the folder (ignore error if it already exists)
    let create_body = serde_json::json!({
        "path": folder_path,
        "autorename": false,
    });
    let resp = client
        .post("https://api.dropboxapi.com/2/files/create_folder_v2")
        .bearer_auth(&tokens.access_token)
        .json(&create_body)
        .send()
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to create Dropbox folder: {e}"),
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        // 409 with "path/conflict" means the folder already exists -- fine
        if !(status == reqwest::StatusCode::CONFLICT && body.contains("conflict")) {
            return Err(BridgeError::Config {
                msg: format!("Failed to create Dropbox folder (HTTP {status}): {body}"),
            });
        }
    }

    // Save tokens to keyring
    let token_json = serde_json::to_string(&tokens).map_err(|e| BridgeError::Config {
        msg: format!("Failed to serialize tokens: {e}"),
    })?;
    key_service
        .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::OAuth { token_json })
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to save OAuth token: {e}"),
        })?;

    // Save config
    let mut config = Config::load();
    config.cloud_provider = Some(bae_core::config::CloudProvider::Dropbox);
    config.cloud_home_dropbox_folder_path = Some(folder_path);
    config.save().map_err(|e| BridgeError::Config {
        msg: format!("Failed to save config: {e}"),
    })?;

    info!("Configured Dropbox cloud provider");
    Ok(())
}

/// OneDrive OAuth sign-in: authorize, get drive, create folder, save tokens + config.
async fn sign_in_onedrive(
    key_service: &KeyService,
    _app_config: &Config,
) -> Result<(), BridgeError> {
    let oauth_config = bae_core::cloud_home::onedrive::OneDriveCloudHome::oauth_config();
    let tokens = bae_core::oauth::authorize(&oauth_config)
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("OneDrive authorization failed: {e}"),
        })?;

    let client = reqwest::Client::new();

    // Get the user's default drive
    let drive_resp = client
        .get("https://graph.microsoft.com/v1.0/me/drive")
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to get drive info: {e}"),
        })?;

    if !drive_resp.status().is_success() {
        let body = drive_resp.text().await.unwrap_or_default();
        return Err(BridgeError::Config {
            msg: format!("Failed to get OneDrive info: {body}"),
        });
    }

    let drive_json: serde_json::Value =
        drive_resp.json().await.map_err(|e| BridgeError::Config {
            msg: format!("Failed to parse drive response: {e}"),
        })?;

    let drive_id = drive_json["id"]
        .as_str()
        .ok_or_else(|| BridgeError::Config {
            msg: "Drive response missing 'id' field".to_string(),
        })?
        .to_string();

    // Create the app folder
    let create_resp = client
        .post(format!(
            "https://graph.microsoft.com/v1.0/drives/{}/root/children",
            drive_id
        ))
        .bearer_auth(&tokens.access_token)
        .json(&serde_json::json!({
            "name": "bae",
            "folder": {},
            "@microsoft.graph.conflictBehavior": "useExisting",
        }))
        .send()
        .await
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to create OneDrive folder: {e}"),
        })?;

    if !create_resp.status().is_success() {
        let body = create_resp.text().await.unwrap_or_default();
        return Err(BridgeError::Config {
            msg: format!("Failed to create OneDrive folder: {body}"),
        });
    }

    let folder_json: serde_json::Value =
        create_resp.json().await.map_err(|e| BridgeError::Config {
            msg: format!("Failed to parse folder response: {e}"),
        })?;

    let folder_id = folder_json["id"]
        .as_str()
        .ok_or_else(|| BridgeError::Config {
            msg: "Folder response missing 'id' field".to_string(),
        })?
        .to_string();

    // Save tokens to keyring
    let token_json = serde_json::to_string(&tokens).map_err(|e| BridgeError::Config {
        msg: format!("Failed to serialize tokens: {e}"),
    })?;
    key_service
        .set_cloud_home_credentials(&bae_core::keys::CloudHomeCredentials::OAuth { token_json })
        .map_err(|e| BridgeError::Config {
            msg: format!("Failed to save OAuth token: {e}"),
        })?;

    // Save config
    let mut config = Config::load();
    config.cloud_provider = Some(bae_core::config::CloudProvider::OneDrive);
    config.cloud_home_onedrive_drive_id = Some(drive_id);
    config.cloud_home_onedrive_folder_id = Some(folder_id);
    config.save().map_err(|e| BridgeError::Config {
        msg: format!("Failed to save config: {e}"),
    })?;

    info!("Configured OneDrive cloud provider");
    Ok(())
}

/// Detect the iCloud Drive ubiquity container path using NSFileManager.
fn detect_icloud_container() -> Option<std::path::PathBuf> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let nsfilemanager_class = Class::get("NSFileManager")?;
        let file_manager: *mut Object = msg_send![nsfilemanager_class, defaultManager];
        if file_manager.is_null() {
            return None;
        }

        let nsstring_class = Class::get("NSString")?;
        let container_cstr = std::ffi::CString::new("iCloud.fm.bae.desktop").ok()?;
        let container_nsstring: *mut Object = msg_send![
            nsstring_class,
            stringWithUTF8String: container_cstr.as_ptr()
        ];

        let url: *mut Object =
            msg_send![file_manager, URLForUbiquityContainerIdentifier: container_nsstring];
        if url.is_null() {
            info!("iCloud Drive ubiquity container not available");
            return None;
        }

        let path_nsstring: *mut Object = msg_send![url, path];
        if path_nsstring.is_null() {
            return None;
        }

        let path_cstr: *const std::ffi::c_char = msg_send![path_nsstring, UTF8String];
        if path_cstr.is_null() {
            return None;
        }

        let path_str = std::ffi::CStr::from_ptr(path_cstr).to_str().ok()?;
        Some(std::path::PathBuf::from(path_str))
    }
}
