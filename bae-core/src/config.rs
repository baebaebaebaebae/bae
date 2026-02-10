use crate::library_dir::LibraryDir;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

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
    /// Global max connections. None = disabled/unlimited.
    pub torrent_max_connections: Option<i32>,
    /// Max connections per torrent. None = disabled/unlimited.
    pub torrent_max_connections_per_torrent: Option<i32>,
    /// Global max upload slots. None = disabled/unlimited.
    pub torrent_max_uploads: Option<i32>,
    /// Max upload slots per torrent. None = disabled/unlimited.
    pub torrent_max_uploads_per_torrent: Option<i32>,
    /// Enable the Subsonic API server
    #[serde(default = "default_true")]
    pub subsonic_enabled: bool,
    /// Subsonic server port
    pub subsonic_port: Option<u16>,
}

/// Metadata about a discovered library (for the library switcher UI)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryInfo {
    pub id: String,
    pub name: Option<String>,
    pub path: PathBuf,
    pub is_active: bool,
}

/// All library paths tracked in ~/.bae/known_libraries.yaml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KnownLibraries {
    paths: Vec<PathBuf>,
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
    pub torrent_max_connections: Option<i32>,
    pub torrent_max_connections_per_torrent: Option<i32>,
    pub torrent_max_uploads: Option<i32>,
    pub torrent_max_uploads_per_torrent: Option<i32>,
    pub subsonic_enabled: bool,
    pub subsonic_port: u16,
}

impl Config {
    pub fn load() -> Self {
        let dev_mode = std::env::var("BAE_DEV_MODE").is_ok() || dotenvy::dotenv().is_ok();
        if dev_mode {
            info!("Dev mode activated - loading from .env");
            Self::from_env()
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file()
        }
    }

    fn from_env() -> Self {
        let env_path = std::path::Path::new(".env");
        let library_id = match std::env::var("BAE_LIBRARY_ID")
            .ok()
            .filter(|s| !s.is_empty())
        {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();

                info!("No BAE_LIBRARY_ID in .env, generated: {}", id);
                append_to_env_file(env_path, "BAE_LIBRARY_ID", &id);
                id
            }
        };
        let device_id = match std::env::var("BAE_DEVICE_ID")
            .ok()
            .filter(|s| !s.is_empty())
        {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();

                info!("No BAE_DEVICE_ID in .env, generated: {}", id);
                append_to_env_file(env_path, "BAE_DEVICE_ID", &id);
                id
            }
        };

        let discogs_key_stored = std::env::var("BAE_DISCOGS_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();
        let encryption_key_hex = std::env::var("BAE_ENCRYPTION_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let encryption_key_stored = encryption_key_hex.is_some();
        let encryption_key_fingerprint =
            encryption_key_hex.and_then(|k| crate::encryption::compute_key_fingerprint(&k));
        let library_dir = LibraryDir::new(match std::env::var("BAE_LIBRARY_PATH").ok() {
            Some(p) => PathBuf::from(p),
            None => {
                let home = dirs::home_dir().expect("Failed to get home directory");
                home.join(".bae").join("libraries").join(&library_id)
            }
        });
        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            library_id,
            device_id,
            library_dir,
            library_name: None,
            keys_migrated: true,
            discogs_key_stored,
            encryption_key_stored,
            encryption_key_fingerprint,
            torrent_bind_interface,
            torrent_listen_port: None,
            torrent_enable_upnp: true,
            torrent_enable_natpmp: true,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            subsonic_enabled: true,
            subsonic_port: 4533,
        }
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
            torrent_max_connections: yaml_config.torrent_max_connections,
            torrent_max_connections_per_torrent: yaml_config.torrent_max_connections_per_torrent,
            torrent_max_uploads: yaml_config.torrent_max_uploads,
            torrent_max_uploads_per_torrent: yaml_config.torrent_max_uploads_per_torrent,
            subsonic_enabled: yaml_config.subsonic_enabled,
            subsonic_port: yaml_config.subsonic_port.unwrap_or(4533),
        }
    }

    pub fn is_dev_mode() -> bool {
        std::env::var("BAE_DEV_MODE").is_ok() || std::path::Path::new(".env").exists()
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        if Self::is_dev_mode() {
            self.save_to_env()
        } else {
            self.save_to_config_yaml()
        }
    }

    pub fn save_to_env(&self) -> Result<(), ConfigError> {
        let env_path = std::path::Path::new(".env");
        let mut lines: Vec<String> = if env_path.exists() {
            std::io::BufReader::new(std::fs::File::open(env_path)?)
                .lines()
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        let mut new_values = std::collections::HashMap::new();
        new_values.insert("BAE_LIBRARY_ID", self.library_id.clone());
        new_values.insert("BAE_DEVICE_ID", self.device_id.clone());
        if let Some(iface) = &self.torrent_bind_interface {
            new_values.insert("BAE_TORRENT_BIND_INTERFACE", iface.clone());
        }

        let mut found = std::collections::HashSet::new();
        for line in &mut lines {
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim().to_string();
                if let Some(val) = new_values.get(key.as_str()) {
                    *line = format!("{}={}", key, val);
                    found.insert(key);
                }
            }
        }
        for (key, val) in &new_values {
            if !found.contains(*key) {
                lines.push(format!("{}={}", key, val));
            }
        }
        let mut file = std::fs::File::create(env_path)?;
        for line in lines {
            writeln!(file, "{}", line)?;
        }
        Ok(())
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
            torrent_max_connections: self.torrent_max_connections,
            torrent_max_connections_per_torrent: self.torrent_max_connections_per_torrent,
            torrent_max_uploads: self.torrent_max_uploads,
            torrent_max_uploads_per_torrent: self.torrent_max_uploads_per_torrent,
            subsonic_enabled: self.subsonic_enabled,
            subsonic_port: Some(self.subsonic_port),
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
            library_name: None,
            keys_migrated: true,
            discogs_key_stored: false,
            encryption_key_stored: true,
            encryption_key_fingerprint: None,
            torrent_bind_interface: None,
            torrent_listen_port: None,
            torrent_enable_upnp: true,
            torrent_enable_natpmp: true,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            subsonic_enabled: true,
            subsonic_port: 4533,
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
        Self::add_known_library(&library_dir)?;

        info!("Created new library at {}", library_dir.display());
        Ok(config)
    }

    /// Discover all libraries registered in ~/.bae/known_libraries.yaml.
    pub fn discover_libraries() -> Vec<LibraryInfo> {
        let home_dir = match dirs::home_dir() {
            Some(d) => d,
            None => return vec![],
        };
        let bae_dir = home_dir.join(".bae");

        let active_id = read_active_library_id(&bae_dir);

        let mut libraries = Vec::new();

        // Read known_libraries.yaml — single source of truth for all libraries
        let known_path = bae_dir.join("known_libraries.yaml");
        if known_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&known_path) {
                if let Ok(known) = serde_yaml::from_str::<KnownLibraries>(&content) {
                    for path in known.paths {
                        if let Some(info) = read_library_info(&path, &active_id) {
                            libraries.push(info);
                        }
                    }
                }
            }
        }

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

    /// Register a library path in ~/.bae/known_libraries.yaml.
    pub fn add_known_library(path: &std::path::Path) -> Result<(), ConfigError> {
        let bae_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".bae");
        let known_path = bae_dir.join("known_libraries.yaml");

        let mut known = if known_path.is_file() {
            let content = std::fs::read_to_string(&known_path)?;
            serde_yaml::from_str::<KnownLibraries>(&content).unwrap_or_default()
        } else {
            KnownLibraries::default()
        };

        let path = path.to_path_buf();
        if !known.paths.contains(&path) {
            known.paths.push(path);
            std::fs::create_dir_all(&bae_dir)?;
            std::fs::write(&known_path, serde_yaml::to_string(&known).unwrap())?;
        }

        Ok(())
    }

    /// Remove a library path from ~/.bae/known_libraries.yaml.
    pub fn remove_known_library(path: &std::path::Path) -> Result<(), ConfigError> {
        let bae_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".bae");
        let known_path = bae_dir.join("known_libraries.yaml");

        if !known_path.is_file() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&known_path)?;
        let mut known = serde_yaml::from_str::<KnownLibraries>(&content).unwrap_or_default();

        known.paths.retain(|p| p != path);
        std::fs::write(&known_path, serde_yaml::to_string(&known).unwrap())?;

        Ok(())
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

/// Find a library's directory by its UUID. Scans known_libraries.yaml paths and
/// `~/.bae/libraries/` subdirectories, reading each config.yaml to match.
fn find_library_by_id(bae_dir: &std::path::Path, uuid: &str) -> Option<LibraryDir> {
    for (path, yaml) in discover_all_library_paths(bae_dir) {
        if yaml.library_id == uuid {
            return Some(LibraryDir::new(path));
        }
    }
    None
}

/// Collect all (path, ConfigYaml) pairs from known_libraries.yaml and ~/.bae/libraries/.
fn discover_all_library_paths(bae_dir: &std::path::Path) -> Vec<(PathBuf, ConfigYaml)> {
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    // Check known_libraries.yaml
    let known_path = bae_dir.join("known_libraries.yaml");
    if known_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&known_path) {
            if let Ok(known) = serde_yaml::from_str::<KnownLibraries>(&content) {
                for path in known.paths {
                    if seen.insert(path.clone()) {
                        if let Some(yaml) = read_config_yaml(&path) {
                            results.push((path, yaml));
                        }
                    }
                }
            }
        }
    }

    // Check ~/.bae/libraries/ subdirectories
    let libraries_dir = bae_dir.join("libraries");
    if libraries_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&libraries_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && seen.insert(path.clone()) {
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

/// Read library info from a directory if it contains a valid config.yaml.
fn read_library_info(path: &std::path::Path, active_id: &Option<String>) -> Option<LibraryInfo> {
    let yaml = read_config_yaml(path)?;
    let is_active = active_id.as_deref() == Some(&yaml.library_id);

    Some(LibraryInfo {
        id: yaml.library_id,
        name: yaml.library_name,
        path: path.to_path_buf(),
        is_active,
    })
}

/// Append a key=value line to a .env file.
fn append_to_env_file(path: &std::path::Path, key: &str, value: &str) {
    use std::fs::OpenOptions;

    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}={}", key, value);
        }
        Err(e) => {
            warn!("Failed to persist {} to {}: {}", key, path.display(), e);
        }
    }
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
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            subsonic_enabled: true,
            subsonic_port: 4533,
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
    fn append_to_env_file_creates_and_appends() {
        let tmp = TempDir::new().unwrap();
        let env_path = tmp.path().join(".env");

        append_to_env_file(&env_path, "FOO", "bar");
        append_to_env_file(&env_path, "BAZ", "qux");

        let content = std::fs::read_to_string(&env_path).unwrap();
        assert!(content.contains("FOO=bar"));
        assert!(content.contains("BAZ=qux"));
    }

    #[test]
    fn append_to_env_file_preserves_existing_content() {
        let tmp = TempDir::new().unwrap();
        let env_path = tmp.path().join(".env");
        std::fs::write(&env_path, "EXISTING=value\n").unwrap();

        append_to_env_file(&env_path, "NEW_KEY", "new_value");

        let content = std::fs::read_to_string(&env_path).unwrap();
        assert!(content.starts_with("EXISTING=value\n"));
        assert!(content.contains("NEW_KEY=new_value"));
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

        // Test read_library_info with UUID-based active comparison
        let active_id = Some("lib-1".to_string());
        let info1 = read_library_info(&lib1_path, &active_id).unwrap();
        assert_eq!(info1.id, "lib-1");
        assert!(info1.is_active);

        let info2 = read_library_info(&lib2_path, &active_id).unwrap();
        assert_eq!(info2.id, "lib-2");
        assert_eq!(info2.name, Some("Second Library".to_string()));
        assert!(!info2.is_active);

        // Invalid dir returns None
        assert!(read_library_info(&libraries_dir.join("invalid"), &active_id).is_none());
    }

    #[test]
    fn find_library_by_id_scans_known_and_libraries_dir() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        // Library in libraries/ dir (found by directory scan)
        let lib1_path = libraries_dir.join("lib-1");
        make_test_config("lib-1", lib1_path.clone())
            .save_to_config_yaml()
            .unwrap();

        // Library at external path (found via known_libraries.yaml)
        let external_path = tmp.path().join("external-lib");
        make_test_config("ext-lib", external_path.clone())
            .save_to_config_yaml()
            .unwrap();
        let known = KnownLibraries {
            paths: vec![external_path.clone()],
        };
        std::fs::write(
            bae_dir.join("known_libraries.yaml"),
            serde_yaml::to_string(&known).unwrap(),
        )
        .unwrap();

        // Find by directory scan
        let found = find_library_by_id(bae_dir, "lib-1");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), lib1_path.as_path());

        // Find by known_libraries.yaml
        let found = find_library_by_id(bae_dir, "ext-lib");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), external_path.as_path());

        // Not found
        assert!(find_library_by_id(bae_dir, "nonexistent").is_none());
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
