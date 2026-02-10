use crate::sodium_ffi;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring_core::Error),
    #[error("Cannot modify keys in dev mode (use environment variables)")]
    DevMode,
    #[error("Crypto error: {0}")]
    Crypto(String),
}

/// Ed25519 keypair for signing changesets and membership changes.
/// The same seed can derive an X25519 keypair for key wrapping.
///
/// This is a global identity (not per-library) so attestations accumulate
/// under one pubkey across all libraries.
pub struct UserKeypair {
    pub signing_key: [u8; sodium_ffi::SIGN_SECRETKEYBYTES], // Ed25519 secret key (64 bytes)
    pub public_key: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES],  // Ed25519 public key (32 bytes)
}

impl UserKeypair {
    /// Generate a new random Ed25519 keypair.
    fn generate() -> Self {
        crate::encryption::ensure_sodium_init();
        let mut pk = [0u8; sodium_ffi::SIGN_PUBLICKEYBYTES];
        let mut sk = [0u8; sodium_ffi::SIGN_SECRETKEYBYTES];
        let ret =
            unsafe { sodium_ffi::crypto_sign_ed25519_keypair(pk.as_mut_ptr(), sk.as_mut_ptr()) };
        assert_eq!(ret, 0, "crypto_sign_ed25519_keypair failed");
        Self {
            signing_key: sk,
            public_key: pk,
        }
    }

    /// Sign a message, returning a 64-byte detached signature.
    pub fn sign(&self, message: &[u8]) -> [u8; sodium_ffi::SIGN_BYTES] {
        crate::encryption::ensure_sodium_init();
        let mut sig = [0u8; sodium_ffi::SIGN_BYTES];
        let mut sig_len: u64 = 0;
        let ret = unsafe {
            sodium_ffi::crypto_sign_ed25519_detached(
                sig.as_mut_ptr(),
                &mut sig_len,
                message.as_ptr(),
                message.len() as u64,
                self.signing_key.as_ptr(),
            )
        };
        assert_eq!(ret, 0, "crypto_sign_ed25519_detached failed");
        sig
    }

    /// Derive the X25519 secret key from this Ed25519 signing key.
    pub fn to_x25519_secret_key(&self) -> [u8; sodium_ffi::CURVE25519_SECRETKEYBYTES] {
        crate::encryption::ensure_sodium_init();
        let mut curve_sk = [0u8; sodium_ffi::CURVE25519_SECRETKEYBYTES];
        let ret = unsafe {
            sodium_ffi::crypto_sign_ed25519_sk_to_curve25519(
                curve_sk.as_mut_ptr(),
                self.signing_key.as_ptr(),
            )
        };
        assert_eq!(ret, 0, "crypto_sign_ed25519_sk_to_curve25519 failed");
        curve_sk
    }

    /// Derive the X25519 public key from this Ed25519 public key.
    pub fn to_x25519_public_key(&self) -> [u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES] {
        crate::encryption::ensure_sodium_init();
        let mut curve_pk = [0u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES];
        let ret = unsafe {
            sodium_ffi::crypto_sign_ed25519_pk_to_curve25519(
                curve_pk.as_mut_ptr(),
                self.public_key.as_ptr(),
            )
        };
        assert_eq!(ret, 0, "crypto_sign_ed25519_pk_to_curve25519 failed");
        curve_pk
    }
}

/// Verify a detached Ed25519 signature against a public key.
pub fn verify_signature(
    signature: &[u8; sodium_ffi::SIGN_BYTES],
    message: &[u8],
    public_key: &[u8; sodium_ffi::SIGN_PUBLICKEYBYTES],
) -> bool {
    crate::encryption::ensure_sodium_init();
    let ret = unsafe {
        sodium_ffi::crypto_sign_ed25519_verify_detached(
            signature.as_ptr(),
            message.as_ptr(),
            message.len() as u64,
            public_key.as_ptr(),
        )
    };
    ret == 0
}

/// Encrypt a message to a recipient's X25519 public key using a sealed box.
/// The sender is anonymous -- only the recipient can decrypt.
pub fn seal_box_encrypt(
    message: &[u8],
    recipient_x25519_pk: &[u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES],
) -> Vec<u8> {
    crate::encryption::ensure_sodium_init();
    let mut ciphertext = vec![0u8; message.len() + sodium_ffi::SEALBYTES];
    let ret = unsafe {
        sodium_ffi::crypto_box_seal(
            ciphertext.as_mut_ptr(),
            message.as_ptr(),
            message.len() as u64,
            recipient_x25519_pk.as_ptr(),
        )
    };
    assert_eq!(ret, 0, "crypto_box_seal failed");
    ciphertext
}

/// Decrypt a sealed box using the recipient's X25519 keypair.
pub fn seal_box_decrypt(
    ciphertext: &[u8],
    recipient_x25519_pk: &[u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES],
    recipient_x25519_sk: &[u8; sodium_ffi::CURVE25519_SECRETKEYBYTES],
) -> Result<Vec<u8>, KeyError> {
    crate::encryption::ensure_sodium_init();
    if ciphertext.len() < sodium_ffi::SEALBYTES {
        return Err(KeyError::Crypto("Ciphertext too short".to_string()));
    }
    let plaintext_len = ciphertext.len() - sodium_ffi::SEALBYTES;
    let mut plaintext = vec![0u8; plaintext_len];
    let ret = unsafe {
        sodium_ffi::crypto_box_seal_open(
            plaintext.as_mut_ptr(),
            ciphertext.as_ptr(),
            ciphertext.len() as u64,
            recipient_x25519_pk.as_ptr(),
            recipient_x25519_sk.as_ptr(),
        )
    };
    if ret != 0 {
        return Err(KeyError::Crypto(
            "Sealed box decryption failed (wrong key or tampered)".to_string(),
        ));
    }
    Ok(plaintext)
}

/// Convert an Ed25519 public key to an X25519 public key.
///
/// This is used when we only have a remote user's Ed25519 public key (hex string)
/// and need to encrypt something to them via sealed box. The `UserKeypair` methods
/// handle the local case; this handles the remote case.
pub fn ed25519_to_x25519_public_key(
    ed25519_pk: &[u8; sodium_ffi::SIGN_PUBLICKEYBYTES],
) -> [u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES] {
    crate::encryption::ensure_sodium_init();
    let mut curve_pk = [0u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES];
    let ret = unsafe {
        sodium_ffi::crypto_sign_ed25519_pk_to_curve25519(curve_pk.as_mut_ptr(), ed25519_pk.as_ptr())
    };
    assert_eq!(ret, 0, "crypto_sign_ed25519_pk_to_curve25519 failed");
    curve_pk
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

    // -------------------------------------------------------------------------
    // Global user keypair (Ed25519 identity, NOT library-scoped)
    // -------------------------------------------------------------------------

    /// Load the user's Ed25519 keypair from the keyring, creating a new one if
    /// none exists. This is a global identity shared across all libraries.
    ///
    /// Dev mode: reads `BAE_USER_SIGNING_KEY` and `BAE_USER_PUBLIC_KEY` env vars (hex).
    /// Falls back to generating and storing in env vars so tests can round-trip.
    pub fn get_or_create_user_keypair(&self) -> Result<UserKeypair, KeyError> {
        if let Some(kp) = self.get_user_keypair_inner()? {
            return Ok(kp);
        }

        let kp = UserKeypair::generate();
        let sk_hex = hex::encode(kp.signing_key);
        let pk_hex = hex::encode(kp.public_key);

        if self.dev_mode {
            std::env::set_var("BAE_USER_SIGNING_KEY", &sk_hex);
            std::env::set_var("BAE_USER_PUBLIC_KEY", &pk_hex);
        } else {
            keyring_core::Entry::new("bae", "bae_user_signing_key")?.set_password(&sk_hex)?;
            keyring_core::Entry::new("bae", "bae_user_public_key")?.set_password(&pk_hex)?;
        }

        info!("Generated and saved new user Ed25519 keypair");
        Ok(kp)
    }

    /// Return just the user's Ed25519 public key, or None if no keypair exists.
    pub fn get_user_public_key(&self) -> Option<[u8; sodium_ffi::SIGN_PUBLICKEYBYTES]> {
        let pk_hex = if self.dev_mode {
            std::env::var("BAE_USER_PUBLIC_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        } else {
            keyring_core::Entry::new("bae", "bae_user_public_key")
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty())
        };

        let pk_hex = pk_hex?;
        let pk_bytes = hex::decode(&pk_hex).ok()?;
        pk_bytes.try_into().ok()
    }

    /// Internal: try to load an existing keypair from the keyring.
    fn get_user_keypair_inner(&self) -> Result<Option<UserKeypair>, KeyError> {
        let (sk_hex, pk_hex) = if self.dev_mode {
            let sk = std::env::var("BAE_USER_SIGNING_KEY")
                .ok()
                .filter(|k| !k.is_empty());
            let pk = std::env::var("BAE_USER_PUBLIC_KEY")
                .ok()
                .filter(|k| !k.is_empty());
            match (sk, pk) {
                (Some(s), Some(p)) => (s, p),
                _ => return Ok(None),
            }
        } else {
            let sk = keyring_core::Entry::new("bae", "bae_user_signing_key")
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty());
            let pk = keyring_core::Entry::new("bae", "bae_user_public_key")
                .ok()
                .and_then(|e| e.get_password().ok())
                .filter(|k| !k.is_empty());
            match (sk, pk) {
                (Some(s), Some(p)) => (s, p),
                _ => return Ok(None),
            }
        };

        let sk_bytes: [u8; sodium_ffi::SIGN_SECRETKEYBYTES] = hex::decode(&sk_hex)
            .map_err(|e| KeyError::Crypto(format!("Invalid signing key hex: {e}")))?
            .try_into()
            .map_err(|_| KeyError::Crypto("Signing key wrong length".to_string()))?;

        let pk_bytes: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] = hex::decode(&pk_hex)
            .map_err(|e| KeyError::Crypto(format!("Invalid public key hex: {e}")))?
            .try_into()
            .map_err(|_| KeyError::Crypto("Public key wrong length".to_string()))?;

        Ok(Some(UserKeypair {
            signing_key: sk_bytes,
            public_key: pk_bytes,
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generation_produces_valid_keys() {
        let kp = UserKeypair::generate();

        // Ed25519 secret key is 64 bytes, public key is 32 bytes
        assert_eq!(kp.signing_key.len(), 64);
        assert_eq!(kp.public_key.len(), 32);

        // Keys should not be all zeros (astronomically unlikely)
        assert!(kp.signing_key.iter().any(|&b| b != 0));
        assert!(kp.public_key.iter().any(|&b| b != 0));
    }

    #[test]
    fn two_keypairs_are_distinct() {
        let kp1 = UserKeypair::generate();
        let kp2 = UserKeypair::generate();
        assert_ne!(kp1.public_key, kp2.public_key);
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let kp = UserKeypair::generate();
        let message = b"changeset payload";

        let sig = kp.sign(message);
        assert!(verify_signature(&sig, message, &kp.public_key));
    }

    #[test]
    fn verify_rejects_wrong_message() {
        let kp = UserKeypair::generate();
        let sig = kp.sign(b"original");
        assert!(!verify_signature(&sig, b"tampered", &kp.public_key));
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let kp1 = UserKeypair::generate();
        let kp2 = UserKeypair::generate();
        let sig = kp1.sign(b"message");
        assert!(!verify_signature(&sig, b"message", &kp2.public_key));
    }

    #[test]
    fn sign_empty_message() {
        let kp = UserKeypair::generate();
        let sig = kp.sign(b"");
        assert!(verify_signature(&sig, b"", &kp.public_key));
    }

    #[test]
    fn ed25519_to_x25519_conversion() {
        let kp = UserKeypair::generate();
        let x_sk = kp.to_x25519_secret_key();
        let x_pk = kp.to_x25519_public_key();

        // Should produce non-zero 32-byte keys
        assert_eq!(x_sk.len(), 32);
        assert_eq!(x_pk.len(), 32);
        assert!(x_sk.iter().any(|&b| b != 0));
        assert!(x_pk.iter().any(|&b| b != 0));
    }

    #[test]
    fn ed25519_to_x25519_is_deterministic() {
        let kp = UserKeypair::generate();
        let x_sk1 = kp.to_x25519_secret_key();
        let x_sk2 = kp.to_x25519_secret_key();
        assert_eq!(x_sk1, x_sk2);
    }

    #[test]
    fn sealed_box_roundtrip() {
        let kp = UserKeypair::generate();
        let x_pk = kp.to_x25519_public_key();
        let x_sk = kp.to_x25519_secret_key();

        let plaintext = b"library encryption key material";
        let ciphertext = seal_box_encrypt(plaintext, &x_pk);

        assert_eq!(ciphertext.len(), plaintext.len() + sodium_ffi::SEALBYTES);

        let decrypted = seal_box_decrypt(&ciphertext, &x_pk, &x_sk).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn sealed_box_wrong_key_fails() {
        let kp1 = UserKeypair::generate();
        let kp2 = UserKeypair::generate();

        let ciphertext = seal_box_encrypt(b"secret", &kp1.to_x25519_public_key());

        let result = seal_box_decrypt(
            &ciphertext,
            &kp2.to_x25519_public_key(),
            &kp2.to_x25519_secret_key(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn sealed_box_empty_message() {
        let kp = UserKeypair::generate();
        let x_pk = kp.to_x25519_public_key();
        let x_sk = kp.to_x25519_secret_key();

        let ciphertext = seal_box_encrypt(b"", &x_pk);
        let decrypted = seal_box_decrypt(&ciphertext, &x_pk, &x_sk).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn sealed_box_too_short_ciphertext() {
        let kp = UserKeypair::generate();
        let result = seal_box_decrypt(
            &[0u8; 10], // shorter than SEALBYTES
            &kp.to_x25519_public_key(),
            &kp.to_x25519_secret_key(),
        );
        assert!(result.is_err());
    }

    /// Combined test for KeyService user keypair methods.
    /// These share process-global env vars so they must run in one test.
    #[test]
    fn key_service_user_keypair() {
        std::env::remove_var("BAE_USER_SIGNING_KEY");
        std::env::remove_var("BAE_USER_PUBLIC_KEY");

        let ks = KeyService::new(true, "test-keypair".to_string());

        // No keypair yet
        assert!(ks.get_user_public_key().is_none());

        // Generate and store
        let kp = ks.get_or_create_user_keypair().unwrap();

        // Should be retrievable now
        let pk = ks.get_user_public_key().unwrap();
        assert_eq!(pk, kp.public_key);

        // Calling again returns the same keypair (idempotent)
        let kp2 = ks.get_or_create_user_keypair().unwrap();
        assert_eq!(kp2.public_key, kp.public_key);
        assert_eq!(kp2.signing_key, kp.signing_key);

        // Different library_id sees the same global keypair
        let ks2 = KeyService::new(true, "other-library".to_string());
        let pk2 = ks2.get_user_public_key().unwrap();
        assert_eq!(pk2, kp.public_key);

        // Stored keypair can sign and verify
        let message = b"test message for signing";
        let sig = kp.sign(message);
        assert!(verify_signature(&sig, message, &kp.public_key));

        // Reloaded keypair produces consistent verification
        let kp3 = ks.get_or_create_user_keypair().unwrap();
        assert!(verify_signature(&sig, message, &kp3.public_key));

        // Clean up
        std::env::remove_var("BAE_USER_SIGNING_KEY");
        std::env::remove_var("BAE_USER_PUBLIC_KEY");
    }
}
