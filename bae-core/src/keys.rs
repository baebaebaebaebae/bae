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

    // -------------------------------------------------------------------------
    // Per-storage-profile S3 credentials
    // -------------------------------------------------------------------------

    /// Read the S3 access key for a storage profile. Returns None if not set.
    ///
    /// Dev mode: reads `BAE_S3_ACCESS_KEY_{profile_id}`, then falls back to `BAE_S3_ACCESS_KEY`.
    /// Prod mode: reads from OS keyring.
    pub fn get_profile_access_key(&self, profile_id: &str) -> Option<String> {
        if self.dev_mode {
            std::env::var(format!("BAE_S3_ACCESS_KEY_{}", profile_id))
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    std::env::var("BAE_S3_ACCESS_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
        } else {
            let account = self.account(&format!("s3_access_key:{}", profile_id));
            keyring_core::Entry::new("bae", &account)
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Save the S3 access key for a storage profile.
    ///
    /// Dev mode: sets the env var (so tests can round-trip without a real keyring).
    /// Prod mode: writes to OS keyring.
    pub fn set_profile_access_key(&self, profile_id: &str, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            std::env::set_var(format!("BAE_S3_ACCESS_KEY_{}", profile_id), value);
            return Ok(());
        }

        let account = self.account(&format!("s3_access_key:{}", profile_id));
        keyring_core::Entry::new("bae", &account)?.set_password(value)?;
        info!("S3 access key saved for profile {}", profile_id);
        Ok(())
    }

    /// Read the S3 secret key for a storage profile. Returns None if not set.
    ///
    /// Dev mode: reads `BAE_S3_SECRET_KEY_{profile_id}`, then falls back to `BAE_S3_SECRET_KEY`.
    /// Prod mode: reads from OS keyring.
    pub fn get_profile_secret_key(&self, profile_id: &str) -> Option<String> {
        if self.dev_mode {
            std::env::var(format!("BAE_S3_SECRET_KEY_{}", profile_id))
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    std::env::var("BAE_S3_SECRET_KEY")
                        .ok()
                        .filter(|k| !k.is_empty())
                })
        } else {
            let account = self.account(&format!("s3_secret_key:{}", profile_id));
            keyring_core::Entry::new("bae", &account)
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        }
    }

    /// Save the S3 secret key for a storage profile.
    ///
    /// Dev mode: sets the env var (so tests can round-trip without a real keyring).
    /// Prod mode: writes to OS keyring.
    pub fn set_profile_secret_key(&self, profile_id: &str, value: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            std::env::set_var(format!("BAE_S3_SECRET_KEY_{}", profile_id), value);
            return Ok(());
        }

        let account = self.account(&format!("s3_secret_key:{}", profile_id));
        keyring_core::Entry::new("bae", &account)?.set_password(value)?;
        info!("S3 secret key saved for profile {}", profile_id);
        Ok(())
    }

    /// Delete S3 credentials for a storage profile from the keyring.
    ///
    /// Dev mode: removes env vars.
    /// Prod mode: deletes from OS keyring. Silently ignores missing entries.
    pub fn delete_profile_credentials(&self, profile_id: &str) -> Result<(), KeyError> {
        if self.dev_mode {
            std::env::remove_var(format!("BAE_S3_ACCESS_KEY_{}", profile_id));
            std::env::remove_var(format!("BAE_S3_SECRET_KEY_{}", profile_id));
            return Ok(());
        }

        for key_type in ["s3_access_key", "s3_secret_key"] {
            let account = self.account(&format!("{}:{}", key_type, profile_id));
            match keyring_core::Entry::new("bae", &account)?.delete_credential() {
                Ok(()) => info!("Deleted {} for profile {}", key_type, profile_id),
                Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(KeyError::Keyring(e)),
            }
        }

        Ok(())
    }

    /// Migrate keys from the old global keyring entries to per-library namespaced entries.
    /// Reads from old names, writes to new names, deletes old entries.
    /// No-op in dev mode.
    pub fn migrate_global_keys(&self) {
        if self.dev_mode {
            return;
        }

        let keys_to_migrate = ["encryption_master_key", "discogs_api_key"];

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
