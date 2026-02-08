use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring_core::Error),
    #[error("Cannot modify keys in dev mode (use environment variables)")]
    DevMode,
}

/// Manages secret keys (Discogs API key, encryption key) with lazy reads.
///
/// In dev mode, reads from environment variables.
/// In prod mode, reads from the OS keyring. Each library_id gets its own
/// namespaced keyring entries so multiple libraries can have independent keys.
///
/// `new()` does no I/O — keyring reads happen lazily in `get_*` methods,
/// because the macOS protected keyring triggers a system password prompt.
#[derive(Clone)]
pub struct KeyService {
    dev_mode: bool,
    library_id: String,
}

impl KeyService {
    pub fn new(dev_mode: bool, library_id: String) -> Self {
        Self {
            dev_mode,
            library_id,
        }
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// Build a namespaced account name for keyring entries.
    fn account(&self, base: &str) -> String {
        format!("{}:{}", base, self.library_id)
    }

    /// Read the Discogs API key. Returns None if not configured.
    ///
    /// Dev mode: reads `BAE_DISCOGS_API_KEY` env var.
    /// Prod mode: reads from OS keyring (may trigger a system prompt on first access).
    pub fn get_discogs_key(&self) -> Option<String> {
        if self.dev_mode {
            std::env::var("BAE_DISCOGS_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        } else {
            keyring_core::Entry::new("bae", &self.account("discogs_api_key"))
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Save the Discogs API key to the OS keyring.
    /// Errors in dev mode (use environment variables instead).
    pub fn set_discogs_key(&self, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        keyring_core::Entry::new("bae", &self.account("discogs_api_key"))?.set_password(value)?;
        info!("Discogs API key saved to keyring");
        Ok(())
    }

    /// Delete the Discogs API key from the OS keyring.
    /// Errors in dev mode.
    pub fn delete_discogs_key(&self) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        match keyring_core::Entry::new("bae", &self.account("discogs_api_key"))?.delete_credential()
        {
            Ok(()) => {
                info!("Discogs API key deleted from keyring");
                Ok(())
            }
            Err(keyring_core::Error::NoEntry) => {
                warn!("Tried to delete Discogs key but none was stored");
                Ok(())
            }
            Err(e) => Err(KeyError::Keyring(e)),
        }
    }

    /// Read the encryption master key. Returns None if not configured.
    ///
    /// Dev mode: reads `BAE_ENCRYPTION_KEY` env var.
    /// Prod mode: reads from OS keyring (may trigger a system prompt on first access).
    pub fn get_encryption_key(&self) -> Option<String> {
        if self.dev_mode {
            std::env::var("BAE_ENCRYPTION_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        } else {
            keyring_core::Entry::new("bae", &self.account("encryption_master_key"))
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Get the encryption key, creating a new one if none exists.
    /// Errors in dev mode (use environment variables instead).
    pub fn get_or_create_encryption_key(&self) -> Result<String, KeyError> {
        if self.dev_mode {
            return self.get_encryption_key().ok_or(KeyError::DevMode);
        }

        if let Some(key) = self.get_encryption_key() {
            return Ok(key);
        }

        let key_hex = hex::encode(crate::encryption::generate_random_key());
        keyring_core::Entry::new("bae", &self.account("encryption_master_key"))?
            .set_password(&key_hex)?;
        info!("Generated and saved new encryption key to keyring");
        Ok(key_hex)
    }

    /// Save the encryption master key to the OS keyring.
    /// Errors in dev mode (use environment variables instead).
    pub fn set_encryption_key(&self, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        keyring_core::Entry::new("bae", &self.account("encryption_master_key"))?
            .set_password(value)?;
        info!("Encryption key saved to keyring");
        Ok(())
    }

    /// Read the cloud sync S3 access key. Returns None if not configured.
    pub fn get_cloud_sync_access_key(&self) -> Option<String> {
        if self.dev_mode {
            std::env::var("BAE_CLOUD_SYNC_ACCESS_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        } else {
            keyring_core::Entry::new("bae", &self.account("cloud_sync_access_key"))
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Save the cloud sync S3 access key to the OS keyring.
    pub fn set_cloud_sync_access_key(&self, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        keyring_core::Entry::new("bae", &self.account("cloud_sync_access_key"))?
            .set_password(value)?;
        info!("Cloud sync access key saved to keyring");
        Ok(())
    }

    /// Delete the cloud sync S3 access key from the OS keyring.
    pub fn delete_cloud_sync_access_key(&self) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        match keyring_core::Entry::new("bae", &self.account("cloud_sync_access_key"))?
            .delete_credential()
        {
            Ok(()) => {
                info!("Cloud sync access key deleted from keyring");
                Ok(())
            }
            Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError::Keyring(e)),
        }
    }

    /// Read the cloud sync S3 secret key. Returns None if not configured.
    pub fn get_cloud_sync_secret_key(&self) -> Option<String> {
        if self.dev_mode {
            std::env::var("BAE_CLOUD_SYNC_SECRET_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        } else {
            keyring_core::Entry::new("bae", &self.account("cloud_sync_secret_key"))
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Save the cloud sync S3 secret key to the OS keyring.
    pub fn set_cloud_sync_secret_key(&self, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        keyring_core::Entry::new("bae", &self.account("cloud_sync_secret_key"))?
            .set_password(value)?;
        info!("Cloud sync secret key saved to keyring");
        Ok(())
    }

    /// Delete the cloud sync S3 secret key from the OS keyring.
    pub fn delete_cloud_sync_secret_key(&self) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        match keyring_core::Entry::new("bae", &self.account("cloud_sync_secret_key"))?
            .delete_credential()
        {
            Ok(()) => {
                info!("Cloud sync secret key deleted from keyring");
                Ok(())
            }
            Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError::Keyring(e)),
        }
    }

    /// Migrate keys from the old global keyring entries to per-library namespaced entries.
    /// Reads from old names, writes to new names, deletes old entries.
    /// No-op in dev mode.
    pub fn migrate_global_keys(&self) {
        if self.dev_mode {
            return;
        }

        let keys_to_migrate = [
            "encryption_master_key",
            "discogs_api_key",
            "cloud_sync_access_key",
            "cloud_sync_secret_key",
        ];

        for base_name in keys_to_migrate {
            let old_entry = match keyring_core::Entry::new("bae", base_name) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let value = match old_entry.get_password() {
                Ok(v) if !v.is_empty() => v,
                _ => continue,
            };

            let new_account = self.account(base_name);
            match keyring_core::Entry::new("bae", &new_account) {
                Ok(new_entry) => {
                    if let Err(e) = new_entry.set_password(&value) {
                        warn!("Failed to migrate {base_name} to {new_account}: {e}");
                        continue;
                    }
                }
                Err(e) => {
                    warn!("Failed to create entry for {new_account}: {e}");
                    continue;
                }
            }

            if let Err(e) = old_entry.delete_credential() {
                warn!("Failed to delete old entry {base_name}: {e}");
            } else {
                info!("Migrated keyring entry {base_name} -> {new_account}");
            }
        }
    }
}
