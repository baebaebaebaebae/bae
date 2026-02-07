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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigYaml {
    pub library_id: Option<String>,
    /// Whether a Discogs API key is stored in the keyring (hint flag, avoids keyring read)
    #[serde(default)]
    pub discogs_key_stored: bool,
    /// Whether an encryption key is stored in the keyring (hint flag, avoids keyring read)
    #[serde(default)]
    pub encryption_key_stored: bool,
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

/// Application configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub library_id: String,
    pub library_path: Option<PathBuf>,
    /// Whether a Discogs API key is stored (hint flag, avoids keyring read on settings render)
    pub discogs_key_stored: bool,
    /// Whether an encryption key is stored (hint flag, avoids keyring read on settings render)
    pub encryption_key_stored: bool,
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
        let library_id = std::env::var("BAE_LIBRARY_ID").unwrap_or_else(|_| {
            let id = uuid::Uuid::new_v4().to_string();
            warn!("No BAE_LIBRARY_ID in .env, generated new ID: {}", id);
            id
        });
        let discogs_key_stored = std::env::var("BAE_DISCOGS_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();
        let encryption_key_stored = std::env::var("BAE_ENCRYPTION_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();
        let library_path = std::env::var("BAE_LIBRARY_PATH").ok().map(PathBuf::from);
        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            library_id,
            library_path,
            discogs_key_stored,
            encryption_key_stored,
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

        // Read library path from global pointer file
        let library_path_file = home_dir.join(".bae").join("library");
        let library_path = if library_path_file.exists() {
            let content = std::fs::read_to_string(&library_path_file)
                .expect("Failed to read library path file");
            let trimmed = content.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        } else {
            None
        };

        // Read library-specific config from the library directory
        let library_dir = library_path
            .clone()
            .unwrap_or_else(|| home_dir.join(".bae"));
        let config_path = library_dir.join("config.yaml");
        let yaml_config: ConfigYaml = if config_path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap())
                .unwrap_or_default()
        } else {
            ConfigYaml::default()
        };

        let library_id = yaml_config
            .library_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        Self {
            library_id,
            library_path,
            discogs_key_stored: yaml_config.discogs_key_stored,
            encryption_key_stored: yaml_config.encryption_key_stored,
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

    pub fn get_library_path(&self) -> PathBuf {
        if let Some(path) = &self.library_path {
            return path.clone();
        }

        dirs::home_dir().unwrap().join(".bae")
    }

    pub fn set_library_path(&mut self, path: PathBuf) {
        self.library_path = Some(path);
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

    /// Save the library path to the global pointer file (~/.bae/library).
    pub fn save_library_path(&self) -> Result<(), ConfigError> {
        let bae_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".bae");
        std::fs::create_dir_all(&bae_dir)?;
        let path_str = self
            .library_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::write(bae_dir.join("library"), path_str)?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        let config_dir = self.get_library_path();
        std::fs::create_dir_all(&config_dir)?;
        let yaml = ConfigYaml {
            library_id: Some(self.library_id.clone()),
            discogs_key_stored: self.discogs_key_stored,
            encryption_key_stored: self.encryption_key_stored,
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
            config_dir.join("config.yaml"),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }
}
