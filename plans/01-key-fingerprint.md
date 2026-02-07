# Plan: Key Fingerprint

Branch: `encryption-ux` (chained off `main` after PR #97)

## Problem

There's no way to detect a wrong encryption key. If the key in keyring doesn't match what was used to encrypt files, decryption silently produces garbage. The user sees corrupted playback with no explanation.

## Solution

Compute a SHA-256 fingerprint of the encryption key. Store it in `config.yaml`. On startup, compare the loaded key's fingerprint against the stored one. Mismatch = clear error, refuse to create EncryptionService.

## Changes

### 1. `bae-core/src/encryption.rs` — add fingerprint methods

```rust
use sha2::{Sha256, Digest};

impl EncryptionService {
    /// SHA-256 fingerprint of the key, first 8 bytes hex-encoded (16 hex chars).
    pub fn fingerprint(&self) -> String {
        let hash = Sha256::digest(self.key);
        hex::encode(&hash[..8])
    }
}

/// Compute fingerprint from a hex-encoded key string without creating an EncryptionService.
pub fn compute_key_fingerprint(key_hex: &str) -> Option<String> {
    let key_bytes = hex::decode(key_hex).ok()?;
    if key_bytes.len() != 32 { return None; }
    let hash = Sha256::digest(&key_bytes);
    Some(hex::encode(&hash[..8]))
}
```

16 hex chars is enough to detect wrong keys (64-bit collision resistance). Short enough to display in settings UI.

Add tests:
- Same key → same fingerprint (deterministic)
- Different keys → different fingerprints
- `compute_key_fingerprint` matches `EncryptionService::fingerprint()` for same key
- Invalid hex / wrong length → None

### 2. `bae-core/src/config.rs` — persist fingerprint

Add to `ConfigYaml`:
```rust
/// SHA-256 fingerprint of the encryption key (first 8 bytes, hex).
/// Used to detect wrong key without attempting decryption.
#[serde(default)]
pub encryption_key_fingerprint: Option<String>,
```

Add to `Config`:
```rust
pub encryption_key_fingerprint: Option<String>,
```

Wire through `from_env()`, `from_config_file()`, `save_to_config_yaml()` — same pattern as `encryption_key_stored`.

For `from_env()`: compute fingerprint from `BAE_ENCRYPTION_KEY` env var if set.

### 3. `bae-desktop/src/main.rs` — validate on startup

Fingerprint validation only runs inside the existing `if config.encryption_key_stored` guard — no keyring read if no key is stored.

Current code (lines 105-112):
```rust
let encryption_service = if config.encryption_key_stored {
    key_service
        .get_encryption_key()
        .and_then(|key| encryption::EncryptionService::new(&key).ok())
} else {
    None
};
```

New code:
```rust
let encryption_service = if config.encryption_key_stored {
    key_service.get_encryption_key().and_then(|key| {
        let service = encryption::EncryptionService::new(&key).ok()?;
        let fingerprint = service.fingerprint();

        match &config.encryption_key_fingerprint {
            Some(stored) if stored != &fingerprint => {
                error!(
                    "Encryption key fingerprint mismatch! Expected {}, got {}. \
                     Wrong key in keyring — encryption disabled.",
                    stored, fingerprint
                );
                None
            }
            None => {
                // First run after upgrade — save fingerprint for future validation
                info!("Saving encryption key fingerprint: {}", fingerprint);
                config.encryption_key_fingerprint = Some(fingerprint);
                config.save().ok();
                Some(service)
            }
            Some(_) => Some(service), // Match — proceed normally
        }
    })
} else {
    None
};
```

Note: `config` needs to be `mut` for the migration case.

### 4. `bae-ui/src/stores/config.rs` — add to store

```rust
pub encryption_key_fingerprint: Option<String>,
```

### 5. `bae-desktop/src/ui/app_service.rs` — sync store

In `load_config()` and `save_config()`, add:
```rust
self.state
    .config()
    .encryption_key_fingerprint()
    .set(config.encryption_key_fingerprint.clone());
```

### 6. `bae-desktop/src/ui/components/settings/storage_profiles.rs` — display fingerprint

Replace the `●●●●●●●●` preview with the fingerprint when available:
```rust
let encryption_configured = *app.state.config().encryption_key_stored().read();
let fingerprint = app.state.config().encryption_key_fingerprint().read().clone();
let encryption_key_preview = if let Some(ref fp) = fingerprint {
    fp.clone()
} else if encryption_configured {
    "●●●●●●●●".to_string()
} else {
    "Not configured".to_string()
};
```

### 7. `bae-desktop/src/ui/components/settings/storage_profiles.rs` — save fingerprint on import

In the `on_import_key` handler, after saving the key, compute and save the fingerprint:
```rust
move |key: String| {
    if let Err(e) = app.key_service.set_encryption_key(&key) {
        tracing::error!("Failed to save encryption key: {e}");
        return;
    }
    let fingerprint = encryption::compute_key_fingerprint(&key);
    app.save_config(|config| {
        config.encryption_key_stored = true;
        config.encryption_key_fingerprint = fingerprint;
    });
}
```

## Files changed

| File | Change |
|------|--------|
| `bae-core/src/encryption.rs` | Add `fingerprint()`, `compute_key_fingerprint()`, tests |
| `bae-core/src/config.rs` | Add `encryption_key_fingerprint` to ConfigYaml + Config |
| `bae-core/src/keys.rs` | No changes needed |
| `bae-desktop/src/main.rs` | Validate fingerprint on startup, migrate existing installs |
| `bae-ui/src/stores/config.rs` | Add `encryption_key_fingerprint` field |
| `bae-desktop/src/ui/app_service.rs` | Sync fingerprint in load/save |
| `bae-desktop/src/ui/components/settings/storage_profiles.rs` | Show fingerprint, save on import |

## Verification

- `cargo clippy -p bae-desktop && cargo clippy -p bae-mocks` clean
- `cargo test -p bae-core` — fingerprint unit tests pass
- Existing install (no fingerprint in config): first startup computes and saves it, encryption works
- Correct key: fingerprint matches, encryption works normally
- Wrong key: fingerprint mismatch logged, encryption service not created
- Import key via settings: fingerprint saved alongside `encryption_key_stored`
- Settings UI shows fingerprint string instead of dots
