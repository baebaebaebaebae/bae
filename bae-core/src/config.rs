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
    /// Whether cloud sync is enabled
    #[serde(default)]
    pub cloud_sync_enabled: bool,
    /// S3 bucket for cloud sync
    pub cloud_sync_bucket: Option<String>,
    /// S3 region for cloud sync
    pub cloud_sync_region: Option<String>,
    /// S3 endpoint for cloud sync (custom endpoint for MinIO etc.)
    pub cloud_sync_endpoint: Option<String>,
    /// Last successful cloud sync upload (ISO 8601)
    pub cloud_sync_last_upload: Option<String>,
}

/// Application configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub library_id: String,
    pub library_path: PathBuf,
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
    pub cloud_sync_enabled: bool,
    pub cloud_sync_bucket: Option<String>,
    pub cloud_sync_region: Option<String>,
    pub cloud_sync_endpoint: Option<String>,
    pub cloud_sync_last_upload: Option<String>,
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
        let library_path = match std::env::var("BAE_LIBRARY_PATH").ok() {
            Some(p) => PathBuf::from(p),
            None => {
                let home = dirs::home_dir().expect("Failed to get home directory");
                home.join(".bae").join("libraries").join(&library_id)
            }
        };
        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            library_id,
            library_path,
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
            cloud_sync_enabled: false,
            cloud_sync_bucket: std::env::var("BAE_CLOUD_SYNC_BUCKET")
                .ok()
                .filter(|s| !s.is_empty()),
            cloud_sync_region: std::env::var("BAE_CLOUD_SYNC_REGION")
                .ok()
                .filter(|s| !s.is_empty()),
            cloud_sync_endpoint: std::env::var("BAE_CLOUD_SYNC_ENDPOINT")
                .ok()
                .filter(|s| !s.is_empty()),
            cloud_sync_last_upload: None,
        }
    }

    fn from_config_file() -> Self {
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        Self::load_from_bae_dir(&bae_dir)
    }

    fn load_from_bae_dir(bae_dir: &std::path::Path) -> Self {
        // Read library path from pointer file — must exist (first-run flow creates it)
        let library_path_file = bae_dir.join("library");
        let library_path = {
            let content = std::fs::read_to_string(&library_path_file).unwrap_or_else(|e| {
                panic!(
                    "No library pointer at {}. Run bae to set up a library. ({})",
                    library_path_file.display(),
                    e
                )
            });
            let trimmed = content.trim();
            assert!(
                !trimmed.is_empty(),
                "Library pointer at {} is empty",
                library_path_file.display()
            );
            PathBuf::from(trimmed)
        };

        // Read library-specific config — must exist with library_id (first-run flow creates it)
        let config_path = library_path.join("config.yaml");
        let yaml_config: ConfigYaml =
            serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
                panic!(
                    "No config.yaml at {}. Library may be corrupted. ({})",
                    config_path.display(),
                    e
                )
            }))
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", config_path.display(), e));

        let library_id = yaml_config.library_id;

        Self {
            library_id,
            library_path,
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
            cloud_sync_enabled: yaml_config.cloud_sync_enabled,
            cloud_sync_bucket: yaml_config.cloud_sync_bucket,
            cloud_sync_region: yaml_config.cloud_sync_region,
            cloud_sync_endpoint: yaml_config.cloud_sync_endpoint,
            cloud_sync_last_upload: yaml_config.cloud_sync_last_upload,
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
        std::fs::write(
            bae_dir.join("library"),
            self.library_path.to_string_lossy().as_ref(),
        )?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        std::fs::create_dir_all(&self.library_path)?;
        let yaml = ConfigYaml {
            library_id: self.library_id.clone(),
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
            cloud_sync_enabled: self.cloud_sync_enabled,
            cloud_sync_bucket: self.cloud_sync_bucket.clone(),
            cloud_sync_region: self.cloud_sync_region.clone(),
            cloud_sync_endpoint: self.cloud_sync_endpoint.clone(),
            cloud_sync_last_upload: self.cloud_sync_last_upload.clone(),
        };
        std::fs::write(
            self.library_path.join("config.yaml"),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }
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
            library_path,
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
            cloud_sync_enabled: false,
            cloud_sync_bucket: None,
            cloud_sync_region: None,
            cloud_sync_endpoint: None,
            cloud_sync_last_upload: None,
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

        // Set up pointer file + config.yaml
        let config = make_test_config(library_id, library_path.clone());
        config.save_to_config_yaml().unwrap();
        std::fs::write(
            bae_dir.join("library"),
            library_path.to_string_lossy().as_ref(),
        )
        .unwrap();

        let loaded = Config::load_from_bae_dir(bae_dir);
        assert_eq!(loaded.library_id, library_id);
        assert_eq!(loaded.library_path, library_path);
    }

    #[test]
    #[should_panic(expected = "No library pointer")]
    fn load_from_bae_dir_panics_without_pointer_file() {
        let tmp = TempDir::new().unwrap();
        Config::load_from_bae_dir(tmp.path());
    }

    #[test]
    #[should_panic(expected = "is empty")]
    fn load_from_bae_dir_panics_on_empty_pointer_file() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("library"), "").unwrap();
        Config::load_from_bae_dir(tmp.path());
    }

    #[test]
    #[should_panic(expected = "No config.yaml")]
    fn load_from_bae_dir_panics_without_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        std::fs::write(
            tmp.path().join("library"),
            library_path.to_string_lossy().as_ref(),
        )
        .unwrap();

        Config::load_from_bae_dir(tmp.path());
    }

    #[test]
    #[should_panic(expected = "Failed to parse")]
    fn load_from_bae_dir_panics_on_config_missing_library_id() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        std::fs::write(
            tmp.path().join("library"),
            library_path.to_string_lossy().as_ref(),
        )
        .unwrap();
        // config.yaml exists but has no library_id
        std::fs::write(
            library_path.join("config.yaml"),
            "discogs_key_stored: false\n",
        )
        .unwrap();

        Config::load_from_bae_dir(tmp.path());
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
}
