use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring_core::Error),
    #[error("Cannot modify keys in dev mode (use environment variables)")]
    DevMode,
}

/// Manages secret keys (Discogs API key, etc.) with lazy reads.
///
/// In dev mode, reads from environment variables.
/// In prod mode, reads from the OS keyring.
///
/// `new()` does no I/O — keyring reads happen lazily in `get_*` methods,
/// because the macOS protected keyring triggers a system password prompt.
#[derive(Clone)]
pub struct KeyService {
    dev_mode: bool,
}

impl KeyService {
    pub fn new(dev_mode: bool) -> Self {
        Self { dev_mode }
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
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
            keyring_core::Entry::new("bae", "discogs_api_key")
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

        keyring_core::Entry::new("bae", "discogs_api_key")?.set_password(value)?;
        info!("Discogs API key saved to keyring");
        Ok(())
    }

    /// Delete the Discogs API key from the OS keyring.
    /// Errors in dev mode.
    pub fn delete_discogs_key(&self) -> Result<(), KeyError> {
        if self.dev_mode {
            return Err(KeyError::DevMode);
        }

        match keyring_core::Entry::new("bae", "discogs_api_key")?.delete_credential() {
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
}
