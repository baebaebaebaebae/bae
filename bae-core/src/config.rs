use crate::library_dir::LibraryDir;
use crate::sync::participation::{default_participation, ParticipationMode};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

/// Generate a fun default library name like "groovin-coltrane" or "boppin-beethoven".
fn generate_library_name() -> String {
    const VERBS: &[&str] = &[
        "boppin",
        "groovin",
        "swingin",
        "rockin",
        "jigging",
        "vibin",
        "jammin",
        "funkin",
        "chillin",
        "cruisin",
        "bumpin",
        "rollin",
        "flowin",
        "blazin",
        "rippin",
        "shreddin",
        "stompin",
        "thumpin",
        "bouncin",
        "struttin",
        "slidin",
        "tappin",
        "hummin",
        "wailin",
        "mixin",
        "looping",
        "droppin",
        "spinnin",
        "scratchin",
        "ticklin",
        "strummin",
        "pluckin",
        "beltin",
        "snappin",
        "poppin",
        "buskin",
        "noodlin",
        "howlin",
        "swooning",
        "crooning",
        "twangin",
        "riffin",
        "sampling",
        "beatboxin",
        "freestylin",
        "headbangin",
    ];
    const MUSICIANS: &[&str] = &[
        // classical
        "bach",
        "beethoven",
        "brahms",
        "chopin",
        "debussy",
        "gershwin",
        "grieg",
        "holst",
        "liszt",
        "mahler",
        "mozart",
        "paganini",
        "ravel",
        "satie",
        "schubert",
        "stravinsky",
        "tchaikovsky",
        "vivaldi",
        // jazz
        "coltrane",
        "davis",
        "dizzy",
        "ella",
        "ellington",
        "mingus",
        "monk",
        // rock / pop / funk / electronic
        "aretha",
        "billie",
        "bjork",
        "bowie",
        "dolly",
        "elvis",
        "etta",
        "hendrix",
        "marley",
        "nina",
        "otis",
        "prince",
        "sinatra",
        "stevie",
        "sting",
        "waits",
        "zappa",
        // hip-hop / rap
        "dilla",
        "kendrick",
        "lauryn",
        "missy",
        "nas",
        "outkast",
        "questlove",
        "tupac",
        // r&b / soul / gospel
        "erykah",
        "luther",
        "sade",
        "sam-cooke",
        "whitney",
        // country / folk
        "cash",
        "joni",
        "woody",
        // latin / brazilian
        "celia",
        "gilberto",
        "jobim",
        "piazzolla",
        "santana",
        "selena",
        "shakira",
        "tito",
        // south asian
        "bismillah",
        "lata",
        "nusrat",
        "shankar",
        "zakir",
        // east asian
        "kitaro",
        "ryuichi",
        "yo-yo",
        // african
        "fela",
        "miriam",
        "youssou",
    ];
    let mut rng = rand::rng();
    let verb = VERBS.choose(&mut rng).unwrap();
    let musician = MUSICIANS.choose(&mut rng).unwrap();
    format!("{}-{}", verb, musician)
}

/// Initialize the keyring credential store.
///
/// On macOS, uses the protected data store with iCloud cloud-sync enabled,
/// so the encryption key is backed up via iCloud Keychain (if the user has it on).
///
/// Must be called once at startup before any keyring operations.
pub fn init_keyring() {
    #[cfg(target_os = "macos")]
    {
        use std::collections::HashMap;
        let config = HashMap::from([("cloud-sync", "true")]);
        match apple_native_keyring_store::protected::Store::new_with_configuration(&config) {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (protected store, iCloud sync enabled)");
            }
            Err(e) => {
                warn!("Failed to create protected keyring store: {e}, falling back to local");
                if let Ok(store) = apple_native_keyring_store::protected::Store::new() {
                    keyring_core::set_default_store(store);
                    info!("Keyring initialized (protected store, local only)");
                }
            }
        }
    }
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn default_true() -> bool {
    true
}

fn default_share_signing_version() -> u32 {
    1
}

/// YAML config file structure for non-secret settings (per-library)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigYaml {
    pub library_id: String,
    /// Human-readable name for this library
    #[serde(default)]
    pub library_name: Option<String>,
    /// Unique identifier for this device, used as the namespace key for sync changesets.
    /// Auto-generated on first startup if missing.
    #[serde(default)]
    pub device_id: Option<String>,
    /// Whether global keyring entries have been migrated to per-library entries
    #[serde(default)]
    pub keys_migrated: bool,
    /// Whether a Discogs API key is stored in the keyring (hint flag, avoids keyring read)
    #[serde(default)]
    pub discogs_key_stored: bool,
    /// Whether an encryption key is stored in the keyring (hint flag, avoids keyring read)
    #[serde(default)]
    pub encryption_key_stored: bool,
    /// SHA-256 fingerprint of the encryption key (first 8 bytes, hex).
    /// Used to detect wrong key without attempting decryption.
    #[serde(default)]
    pub encryption_key_fingerprint: Option<String>,
    pub torrent_bind_interface: Option<String>,
    /// Listening port for incoming torrent connections. None = random port.
    pub torrent_listen_port: Option<u16>,
    /// Enable UPnP port forwarding
    #[serde(default = "default_true")]
    pub torrent_enable_upnp: bool,
    /// Enable NAT-PMP port forwarding
    #[serde(default = "default_true")]
    pub torrent_enable_natpmp: bool,
    /// Enable the Mainline DHT (BEP 5) for peer discovery
    #[serde(default)]
    pub torrent_enable_dht: bool,
    /// Global max connections. None = disabled/unlimited.
    pub torrent_max_connections: Option<i32>,
    /// Max connections per torrent. None = disabled/unlimited.
    pub torrent_max_connections_per_torrent: Option<i32>,
    /// Global max upload slots. None = disabled/unlimited.
    pub torrent_max_uploads: Option<i32>,
    /// Max upload slots per torrent. None = disabled/unlimited.
    pub torrent_max_uploads_per_torrent: Option<i32>,
    /// Discovery network participation mode (off, attestations_only, full).
    /// Controls whether this library announces releases on the DHT and shares attestations.
    #[serde(default = "default_participation")]
    pub network_participation: ParticipationMode,
    /// Enable the Subsonic API server
    #[serde(default = "default_true")]
    pub subsonic_enabled: bool,
    /// Subsonic server port
    pub subsonic_port: Option<u16>,
    /// Subsonic server bind address (default: 127.0.0.1, set to 0.0.0.0 for LAN/external access)
    #[serde(default)]
    pub subsonic_bind_address: Option<String>,
    /// Whether Subsonic authentication is required
    #[serde(default)]
    pub subsonic_auth_enabled: bool,
    /// Subsonic username (password stored in keyring)
    #[serde(default)]
    pub subsonic_username: Option<String>,

    // Cloud home S3 configuration (credentials stored in keyring)
    /// S3 bucket name for cloud home
    #[serde(default)]
    pub cloud_home_s3_bucket: Option<String>,
    /// S3 region for cloud home
    #[serde(default)]
    pub cloud_home_s3_region: Option<String>,
    /// S3 endpoint for cloud home (for S3-compatible services)
    #[serde(default)]
    pub cloud_home_s3_endpoint: Option<String>,

    /// Base URL for share links (e.g. "https://listen.example.com")
    #[serde(default)]
    pub share_base_url: Option<String>,
    /// Default expiry for share links in days (None = never expires)
    #[serde(default)]
    pub share_default_expiry_days: Option<u32>,
    /// Signing key version for share tokens. Incrementing invalidates all outstanding links.
    #[serde(default = "default_share_signing_version")]
    pub share_signing_key_version: u32,

    /// Remote Subsonic servers the user is following (read-only browsing + streaming)
    #[serde(default)]
    pub followed_libraries: Vec<FollowedLibrary>,
}

/// A remote Subsonic server the user is "following" (read-only browsing + streaming).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FollowedLibrary {
    /// Unique ID (UUID)
    pub id: String,
    /// User-chosen display name
    pub name: String,
    /// Subsonic server URL (e.g. "http://192.168.1.100:4533")
    pub server_url: String,
    /// Subsonic username (password stored in keyring)
    pub username: String,
}

/// Metadata about a discovered library (for the library switcher UI)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryInfo {
    pub id: String,
    pub name: Option<String>,
    pub path: PathBuf,
    pub is_active: bool,
}

/// Application configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub library_id: String,
    /// Unique device identifier for sync changeset namespacing.
    /// Always present after startup (auto-generated if missing from config).
    pub device_id: String,
    pub library_dir: LibraryDir,
    pub library_name: Option<String>,
    pub keys_migrated: bool,
    /// Whether a Discogs API key is stored (hint flag, avoids keyring read on settings render)
    pub discogs_key_stored: bool,
    /// Whether an encryption key is stored (hint flag, avoids keyring read on settings render)
    pub encryption_key_stored: bool,
    /// SHA-256 fingerprint of the encryption key (detects wrong key without decryption)
    pub encryption_key_fingerprint: Option<String>,
    pub torrent_bind_interface: Option<String>,
    pub torrent_listen_port: Option<u16>,
    pub torrent_enable_upnp: bool,
    pub torrent_enable_natpmp: bool,
    pub torrent_enable_dht: bool,
    pub torrent_max_connections: Option<i32>,
    pub torrent_max_connections_per_torrent: Option<i32>,
    pub torrent_max_uploads: Option<i32>,
    pub torrent_max_uploads_per_torrent: Option<i32>,
    pub network_participation: ParticipationMode,
    pub subsonic_enabled: bool,
    pub subsonic_port: u16,
    /// Subsonic server bind address (default: 127.0.0.1)
    pub subsonic_bind_address: String,
    /// Whether Subsonic authentication is required
    pub subsonic_auth_enabled: bool,
    /// Subsonic username (password stored in keyring)
    pub subsonic_username: Option<String>,
    /// S3 bucket name for cloud home
    pub cloud_home_s3_bucket: Option<String>,
    /// S3 region for cloud home
    pub cloud_home_s3_region: Option<String>,
    /// S3 endpoint for cloud home (for S3-compatible services)
    pub cloud_home_s3_endpoint: Option<String>,
    /// Base URL for share links (e.g. "https://listen.example.com")
    pub share_base_url: Option<String>,
    /// Default expiry for share links in days (None = never expires)
    pub share_default_expiry_days: Option<u32>,
    /// Signing key version for share tokens. Incrementing invalidates all outstanding links.
    pub share_signing_key_version: u32,
    /// Remote Subsonic servers the user is following
    pub followed_libraries: Vec<FollowedLibrary>,
}

impl Config {
    pub fn load() -> Self {
        let dev_mode = std::env::var("BAE_DEV_MODE").is_ok() || dotenvy::dotenv().is_ok();
        if dev_mode {
            info!("Dev mode activated — loading config.yaml with .env overrides");
            Self::from_env()
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file()
        }
    }

    fn from_env() -> Self {
        // Use the same active-library pointer file as production mode
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        let mut config = Self::load_from_bae_dir(&bae_dir);

        // Overlay dev-specific env vars on top of the config.yaml values
        if let Some(path) = std::env::var("BAE_LIBRARY_PATH")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.library_dir = LibraryDir::new(PathBuf::from(path));
        }

        let encryption_key_hex = std::env::var("BAE_ENCRYPTION_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        if let Some(ref key) = encryption_key_hex {
            config.encryption_key_stored = true;
            config.encryption_key_fingerprint = crate::encryption::compute_key_fingerprint(key);
        }

        if std::env::var("BAE_DISCOGS_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some()
        {
            config.discogs_key_stored = true;
        }

        if let Some(v) = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.torrent_bind_interface = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_BUCKET")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home_s3_bucket = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_REGION")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home_s3_region = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home_s3_endpoint = Some(v);
        }

        if let Some(v) = std::env::var("BAE_SHARE_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.share_base_url = Some(v);
        }

        config
    }

    fn from_config_file() -> Self {
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        Self::load_from_bae_dir(&bae_dir)
    }

    fn load_from_bae_dir(bae_dir: &std::path::Path) -> Self {
        // Read active library UUID from pointer file
        let pointer_file = bae_dir.join("active-library");
        let active_id = read_active_library_id(bae_dir);

        let library_id = match active_id {
            Some(id) => id,
            None => {
                // No pointer file — auto-select the first known library
                let libraries = discover_all_library_paths(bae_dir);
                match libraries.into_iter().next() {
                    Some((_path, yaml)) => yaml.library_id,
                    None => panic!(
                        "No active-library pointer at {} and no libraries found. \
                         Run bae to set up a library.",
                        pointer_file.display()
                    ),
                }
            }
        };

        let library_dir = find_library_by_id(bae_dir, &library_id).unwrap_or_else(|| {
            panic!(
                "Library '{}' not found. The library may have been removed or its drive unmounted.",
                library_id
            )
        });

        // Read library-specific config — must exist with library_id (first-run flow creates it)
        let config_path = library_dir.config_path();
        let yaml_config: ConfigYaml =
            serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
                panic!(
                    "No config.yaml at {}. Library may be corrupted. ({})",
                    config_path.display(),
                    e
                )
            }))
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", config_path.display(), e));

        // Auto-generate device_id if missing (first startup after upgrade)
        let device_id = match yaml_config.device_id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();

                info!("No device_id in config.yaml, generated: {}", id);
                let mut yaml_to_save = yaml_config.clone();
                yaml_to_save.device_id = Some(id.clone());
                if let Err(e) =
                    std::fs::write(&config_path, serde_yaml::to_string(&yaml_to_save).unwrap())
                {
                    warn!("Failed to save device_id to config.yaml: {e}");
                }
                id
            }
        };

        Self {
            library_id: yaml_config.library_id,
            device_id,
            library_dir,
            library_name: yaml_config.library_name,
            keys_migrated: yaml_config.keys_migrated,
            discogs_key_stored: yaml_config.discogs_key_stored,
            encryption_key_stored: yaml_config.encryption_key_stored,
            encryption_key_fingerprint: yaml_config.encryption_key_fingerprint,
            torrent_bind_interface: yaml_config.torrent_bind_interface,
            torrent_listen_port: yaml_config.torrent_listen_port,
            torrent_enable_upnp: yaml_config.torrent_enable_upnp,
            torrent_enable_natpmp: yaml_config.torrent_enable_natpmp,
            torrent_enable_dht: yaml_config.torrent_enable_dht,
            torrent_max_connections: yaml_config.torrent_max_connections,
            torrent_max_connections_per_torrent: yaml_config.torrent_max_connections_per_torrent,
            torrent_max_uploads: yaml_config.torrent_max_uploads,
            torrent_max_uploads_per_torrent: yaml_config.torrent_max_uploads_per_torrent,
            network_participation: yaml_config.network_participation,
            subsonic_enabled: yaml_config.subsonic_enabled,
            subsonic_port: yaml_config.subsonic_port.unwrap_or(4533),
            subsonic_bind_address: yaml_config
                .subsonic_bind_address
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            subsonic_auth_enabled: yaml_config.subsonic_auth_enabled,
            subsonic_username: yaml_config.subsonic_username,
            cloud_home_s3_bucket: yaml_config.cloud_home_s3_bucket,
            cloud_home_s3_region: yaml_config.cloud_home_s3_region,
            cloud_home_s3_endpoint: yaml_config.cloud_home_s3_endpoint,
            share_base_url: yaml_config.share_base_url,
            share_default_expiry_days: yaml_config.share_default_expiry_days,
            share_signing_key_version: yaml_config.share_signing_key_version,
            followed_libraries: yaml_config.followed_libraries,
        }
    }

    pub fn is_dev_mode() -> bool {
        std::env::var("BAE_DEV_MODE").is_ok() || std::path::Path::new(".env").exists()
    }

    /// Whether sync is configured: bucket, region, and keyring credentials all present.
    pub fn sync_enabled(&self, key_service: &crate::keys::KeyService) -> bool {
        self.cloud_home_s3_bucket.is_some()
            && self.cloud_home_s3_region.is_some()
            && key_service.get_cloud_home_access_key().is_some()
            && key_service.get_cloud_home_secret_key().is_some()
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to_config_yaml()
    }

    /// Save the active library UUID to the global pointer file (~/.bae/active-library).
    pub fn save_active_library(&self) -> Result<(), ConfigError> {
        let bae_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".bae");
        std::fs::create_dir_all(&bae_dir)?;
        std::fs::write(bae_dir.join("active-library"), &self.library_id)?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        std::fs::create_dir_all(&*self.library_dir)?;
        let yaml = ConfigYaml {
            library_id: self.library_id.clone(),
            library_name: self.library_name.clone(),
            device_id: Some(self.device_id.clone()),
            keys_migrated: self.keys_migrated,
            discogs_key_stored: self.discogs_key_stored,
            encryption_key_stored: self.encryption_key_stored,
            encryption_key_fingerprint: self.encryption_key_fingerprint.clone(),
            torrent_bind_interface: self.torrent_bind_interface.clone(),
            torrent_listen_port: self.torrent_listen_port,
            torrent_enable_upnp: self.torrent_enable_upnp,
            torrent_enable_natpmp: self.torrent_enable_natpmp,
            torrent_enable_dht: self.torrent_enable_dht,
            torrent_max_connections: self.torrent_max_connections,
            torrent_max_connections_per_torrent: self.torrent_max_connections_per_torrent,
            torrent_max_uploads: self.torrent_max_uploads,
            torrent_max_uploads_per_torrent: self.torrent_max_uploads_per_torrent,
            network_participation: self.network_participation,
            subsonic_enabled: self.subsonic_enabled,
            subsonic_port: Some(self.subsonic_port),
            subsonic_bind_address: Some(self.subsonic_bind_address.clone()),
            subsonic_auth_enabled: self.subsonic_auth_enabled,
            subsonic_username: self.subsonic_username.clone(),
            cloud_home_s3_bucket: self.cloud_home_s3_bucket.clone(),
            cloud_home_s3_region: self.cloud_home_s3_region.clone(),
            cloud_home_s3_endpoint: self.cloud_home_s3_endpoint.clone(),
            share_base_url: self.share_base_url.clone(),
            share_default_expiry_days: self.share_default_expiry_days,
            share_signing_key_version: self.share_signing_key_version,
            followed_libraries: self.followed_libraries.clone(),
        };
        std::fs::write(
            self.library_dir.config_path(),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }

    /// Create a brand-new library: generate ID, create directory, encryption key, and config.yaml.
    ///
    /// Returns the Config. Caller should call `save_active_library()` and relaunch separately.
    pub fn create_new_library(dev_mode: bool) -> Result<Config, ConfigError> {
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        let id = uuid::Uuid::new_v4().to_string();
        let library_dir = LibraryDir::new(bae_dir.join("libraries").join(&id));
        std::fs::create_dir_all(&*library_dir)?;

        let key_service = crate::keys::KeyService::new(dev_mode, id.clone());

        let device_id = uuid::Uuid::new_v4().to_string();

        let mut config = Config {
            library_id: id,
            device_id,
            library_dir: library_dir.clone(),
            library_name: Some(generate_library_name()),
            keys_migrated: true,
            discogs_key_stored: false,
            encryption_key_stored: true,
            encryption_key_fingerprint: None,
            torrent_bind_interface: None,
            torrent_listen_port: None,
            torrent_enable_upnp: true,
            torrent_enable_natpmp: true,
            torrent_enable_dht: false,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            network_participation: ParticipationMode::Off,
            subsonic_enabled: true,
            subsonic_port: 4533,
            subsonic_bind_address: "127.0.0.1".to_string(),
            subsonic_auth_enabled: false,
            subsonic_username: None,
            cloud_home_s3_bucket: None,
            cloud_home_s3_region: None,
            cloud_home_s3_endpoint: None,
            share_base_url: None,
            share_default_expiry_days: None,
            share_signing_key_version: 1,
            followed_libraries: vec![],
        };

        match key_service.get_or_create_encryption_key() {
            Ok(key_hex) => {
                config.encryption_key_fingerprint =
                    crate::encryption::compute_key_fingerprint(&key_hex);
            }
            Err(e) => {
                tracing::error!("Failed to create encryption key: {e}");
                config.encryption_key_stored = false;
            }
        }

        config.save_to_config_yaml()?;

        info!("Created new library at {}", library_dir.display());
        Ok(config)
    }

    /// Discover all libraries under ~/.bae/libraries/.
    pub fn discover_libraries() -> Vec<LibraryInfo> {
        let home_dir = match dirs::home_dir() {
            Some(d) => d,
            None => return vec![],
        };
        let bae_dir = home_dir.join(".bae");
        let active_id = read_active_library_id(&bae_dir);

        let mut libraries: Vec<LibraryInfo> = discover_all_library_paths(&bae_dir)
            .into_iter()
            .map(|(path, yaml)| {
                let is_active = active_id.as_deref() == Some(&yaml.library_id);
                LibraryInfo {
                    id: yaml.library_id,
                    name: yaml.library_name,
                    path,
                    is_active,
                }
            })
            .collect();

        // Sort: active first, then by name/id
        libraries.sort_by(|a, b| {
            b.is_active.cmp(&a.is_active).then_with(|| {
                let a_name = a.name.as_deref().unwrap_or(&a.id);
                let b_name = b.name.as_deref().unwrap_or(&b.id);
                a_name.cmp(b_name)
            })
        });

        libraries
    }

    /// Read the library_id from a library directory's config.yaml.
    /// Handles YAML parsing so callers don't need serde_yaml.
    pub fn read_library_id(library_path: &std::path::Path) -> Result<String, ConfigError> {
        let config_path = library_path.join("config.yaml");
        let content = std::fs::read_to_string(&config_path)?;
        let yaml: ConfigYaml = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::Serialization(e.to_string()))?;
        Ok(yaml.library_id)
    }

    /// Add a followed library to the config and persist.
    pub fn add_followed_library(&mut self, lib: FollowedLibrary) -> Result<(), ConfigError> {
        self.followed_libraries.push(lib);
        self.save_to_config_yaml()
    }

    /// Remove a followed library by ID and persist.
    pub fn remove_followed_library(&mut self, id: &str) -> Result<(), ConfigError> {
        self.followed_libraries.retain(|l| l.id != id);
        self.save_to_config_yaml()
    }

    /// Rename a library by updating its config.yaml library_name field.
    /// Handles YAML read/modify/write so callers don't need serde_yaml.
    pub fn rename_library(
        library_path: &std::path::Path,
        new_name: &str,
    ) -> Result<(), ConfigError> {
        let config_path = library_path.join("config.yaml");
        let content = std::fs::read_to_string(&config_path)?;
        let mut yaml: ConfigYaml = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::Serialization(e.to_string()))?;

        yaml.library_name = if new_name.is_empty() {
            None
        } else {
            Some(new_name.to_string())
        };

        std::fs::write(&config_path, serde_yaml::to_string(&yaml).unwrap())?;

        Ok(())
    }
}

/// Read the active library UUID from `~/.bae/active-library`, if it exists.
fn read_active_library_id(bae_dir: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(bae_dir.join("active-library"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Find a library's directory by its UUID, scanning `~/.bae/libraries/` subdirectories.
fn find_library_by_id(bae_dir: &std::path::Path, uuid: &str) -> Option<LibraryDir> {
    for (path, yaml) in discover_all_library_paths(bae_dir) {
        if yaml.library_id == uuid {
            return Some(LibraryDir::new(path));
        }
    }
    None
}

/// Collect all (path, ConfigYaml) pairs from ~/.bae/libraries/.
fn discover_all_library_paths(bae_dir: &std::path::Path) -> Vec<(PathBuf, ConfigYaml)> {
    let mut results = Vec::new();
    let libraries_dir = bae_dir.join("libraries");

    if libraries_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&libraries_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(yaml) = read_config_yaml(&path) {
                        results.push((path, yaml));
                    }
                }
            }
        }
    }

    results
}

/// Read and parse config.yaml from a library directory, if it exists.
fn read_config_yaml(path: &std::path::Path) -> Option<ConfigYaml> {
    let config_path = path.join("config.yaml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    serde_yaml::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_config(library_id: &str, library_path: PathBuf) -> Config {
        Config {
            library_id: library_id.to_string(),
            device_id: "test-device-id".to_string(),
            library_dir: LibraryDir::new(library_path),
            library_name: None,
            keys_migrated: true,
            discogs_key_stored: false,
            encryption_key_stored: false,
            encryption_key_fingerprint: None,
            torrent_bind_interface: None,
            torrent_listen_port: None,
            torrent_enable_upnp: true,
            torrent_enable_natpmp: true,
            torrent_enable_dht: false,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            network_participation: ParticipationMode::Off,
            subsonic_enabled: true,
            subsonic_port: 4533,
            subsonic_bind_address: "127.0.0.1".to_string(),
            subsonic_auth_enabled: false,
            subsonic_username: None,
            cloud_home_s3_bucket: None,
            cloud_home_s3_region: None,
            cloud_home_s3_endpoint: None,
            share_base_url: None,
            share_default_expiry_days: None,
            share_signing_key_version: 1,
            followed_libraries: vec![],
        }
    }

    #[test]
    fn config_yaml_requires_library_id() {
        let yaml = "discogs_key_stored: false\n";
        let result: Result<ConfigYaml, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "ConfigYaml should fail without library_id");
    }

    #[test]
    fn config_yaml_parses_with_library_id() {
        let yaml = "library_id: abc-123\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.library_id, "abc-123");
        // network_participation defaults to Off when absent
        assert_eq!(config.network_participation, ParticipationMode::Off);
    }

    #[test]
    fn config_yaml_parses_network_participation() {
        let yaml = "library_id: abc-123\nnetwork_participation: full\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.network_participation, ParticipationMode::Full);

        let yaml = "library_id: abc-123\nnetwork_participation: attestations_only\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.network_participation,
            ParticipationMode::AttestationsOnly
        );
    }

    #[test]
    fn save_and_load_config_yaml_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let config = make_test_config("my-library-id", library_path.clone());

        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_id, "my-library-id");
    }

    #[test]
    fn load_from_bae_dir_reads_pointer_and_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_id = "test-lib-id";
        let library_path = bae_dir.join("libraries").join(library_id);

        // Set up active-library pointer (UUID) + config.yaml
        let config = make_test_config(library_id, library_path.clone());
        config.save_to_config_yaml().unwrap();
        std::fs::write(bae_dir.join("active-library"), library_id).unwrap();

        let loaded = Config::load_from_bae_dir(bae_dir);
        assert_eq!(loaded.library_id, library_id);
        assert_eq!(&*loaded.library_dir, library_path.as_path());
    }

    #[test]
    fn load_from_bae_dir_auto_selects_first_library_without_pointer() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("auto-lib");

        // Create a library in libraries/ but no active-library pointer
        make_test_config("auto-lib", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let loaded = Config::load_from_bae_dir(bae_dir);
        assert_eq!(loaded.library_id, "auto-lib");
    }

    #[test]
    #[should_panic(expected = "no libraries found")]
    fn load_from_bae_dir_panics_without_pointer_or_libraries() {
        let tmp = TempDir::new().unwrap();
        Config::load_from_bae_dir(tmp.path());
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_when_library_id_not_found() {
        let tmp = TempDir::new().unwrap();
        // Pointer to a UUID that doesn't exist anywhere
        std::fs::write(tmp.path().join("active-library"), "nonexistent-uuid").unwrap();
        Config::load_from_bae_dir(tmp.path());
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_when_dir_exists_but_no_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        // Dir exists but no config.yaml — library is invisible to find_library_by_id
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        Config::load_from_bae_dir(bae_dir);
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_on_unparseable_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        // config.yaml exists but missing library_id — invisible to find_library_by_id
        std::fs::write(
            library_path.join("config.yaml"),
            "discogs_key_stored: false\n",
        )
        .unwrap();
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        Config::load_from_bae_dir(bae_dir);
    }

    #[test]
    fn library_name_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let mut config = make_test_config("lib-1", library_path.clone());
        config.library_name = Some("My Music".to_string());
        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_name, Some("My Music".to_string()));
    }

    #[test]
    fn discover_libraries_finds_dirs_with_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        // Create two libraries
        let lib1_path = libraries_dir.join("lib-1");
        make_test_config("lib-1", lib1_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let lib2_path = libraries_dir.join("lib-2");
        let mut lib2 = make_test_config("lib-2", lib2_path.clone());
        lib2.library_name = Some("Second Library".to_string());
        lib2.save_to_config_yaml().unwrap();

        // Create an invalid dir (no config.yaml)
        std::fs::create_dir_all(libraries_dir.join("invalid")).unwrap();

        let discovered = discover_all_library_paths(bae_dir);
        assert_eq!(discovered.len(), 2);

        let ids: Vec<&str> = discovered
            .iter()
            .map(|(_, y)| y.library_id.as_str())
            .collect();
        assert!(ids.contains(&"lib-1"));
        assert!(ids.contains(&"lib-2"));

        let lib2_entry = discovered
            .iter()
            .find(|(_, y)| y.library_id == "lib-2")
            .unwrap();
        assert_eq!(
            lib2_entry.1.library_name,
            Some("Second Library".to_string())
        );
    }

    #[test]
    fn find_library_by_id_scans_libraries_dir() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        let lib1_path = libraries_dir.join("lib-1");
        make_test_config("lib-1", lib1_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let lib2_path = libraries_dir.join("lib-2");
        make_test_config("lib-2", lib2_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let found = find_library_by_id(bae_dir, "lib-1");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), lib1_path.as_path());

        let found = find_library_by_id(bae_dir, "lib-2");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), lib2_path.as_path());

        assert!(find_library_by_id(bae_dir, "nonexistent").is_none());
    }

    #[test]
    fn followed_libraries_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let mut config = make_test_config("lib-follow", library_path.clone());
        config.followed_libraries = vec![FollowedLibrary {
            id: "follow-1".to_string(),
            name: "Friend's Library".to_string(),
            server_url: "http://192.168.1.50:4533".to_string(),
            username: "listener".to_string(),
        }];
        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.followed_libraries.len(), 1);
        assert_eq!(yaml.followed_libraries[0].id, "follow-1");
        assert_eq!(yaml.followed_libraries[0].name, "Friend's Library");
        assert_eq!(
            yaml.followed_libraries[0].server_url,
            "http://192.168.1.50:4533"
        );
        assert_eq!(yaml.followed_libraries[0].username, "listener");
    }

    #[test]
    fn followed_libraries_default_empty() {
        let yaml = "library_id: abc-123\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert!(config.followed_libraries.is_empty());
    }

    #[test]
    fn add_and_remove_followed_library() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let mut config = make_test_config("lib-follow-2", library_path.clone());
        config.save_to_config_yaml().unwrap();

        config
            .add_followed_library(FollowedLibrary {
                id: "f1".to_string(),
                name: "Server A".to_string(),
                server_url: "http://a:4533".to_string(),
                username: "user".to_string(),
            })
            .unwrap();
        assert_eq!(config.followed_libraries.len(), 1);

        config
            .add_followed_library(FollowedLibrary {
                id: "f2".to_string(),
                name: "Server B".to_string(),
                server_url: "http://b:4533".to_string(),
                username: "user2".to_string(),
            })
            .unwrap();
        assert_eq!(config.followed_libraries.len(), 2);

        config.remove_followed_library("f1").unwrap();
        assert_eq!(config.followed_libraries.len(), 1);
        assert_eq!(config.followed_libraries[0].id, "f2");

        // Verify persistence
        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.followed_libraries.len(), 1);
        assert_eq!(yaml.followed_libraries[0].id, "f2");
    }

    #[test]
    fn rename_library_updates_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        make_test_config("lib-1", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        Config::rename_library(&library_path, "New Name").unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_name, Some("New Name".to_string()));
        assert_eq!(yaml.library_id, "lib-1"); // unchanged
    }
}
