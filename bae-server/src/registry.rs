use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Registry {
    pub libraries: Vec<LibraryConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LibraryConfig {
    pub hostname: String,
    pub library_id: String,
    /// Base path for cached DB + images.
    pub library_path: String,
    /// Hex-encoded encryption key.
    pub recovery_key: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_key_prefix: Option<String>,
    /// None = pinned (never evict). Some(N) = evict after N seconds idle.
    pub cache_timeout_secs: Option<u64>,
}

impl Registry {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read registry: {e}"))?;
        serde_yaml::from_str(&content).map_err(|e| format!("parse registry: {e}"))
    }

    /// Build a hostname -> config lookup map.
    /// Returns an error if two libraries share the same hostname.
    pub fn by_hostname(&self) -> Result<HashMap<String, LibraryConfig>, String> {
        let mut map = HashMap::new();
        for lib in &self.libraries {
            if map.contains_key(&lib.hostname) {
                return Err(format!("duplicate hostname in registry: {}", lib.hostname));
            }
            map.insert(lib.hostname.clone(), lib.clone());
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_registry_yaml() {
        let yaml = r#"
libraries:
  - hostname: alice.example.com
    library_id: lib-alice
    library_path: /tmp/bae/alice
    recovery_key: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    s3_bucket: my-bucket
    s3_region: us-east-1
    s3_access_key: AKIA123
    s3_secret_key: secret123
    s3_key_prefix: alice/
    cache_timeout_secs: 600
  - hostname: bob.example.com
    library_id: lib-bob
    library_path: /tmp/bae/bob
    recovery_key: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
    s3_bucket: my-bucket
    s3_region: us-east-1
    s3_endpoint: https://minio.example.com
    s3_access_key: AKIA456
    s3_secret_key: secret456
"#;

        let registry: Registry = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(registry.libraries.len(), 2);

        let by_host = registry.by_hostname().unwrap();
        assert!(by_host.contains_key("alice.example.com"));
        assert!(by_host.contains_key("bob.example.com"));

        let alice = &by_host["alice.example.com"];
        assert_eq!(alice.library_id, "lib-alice");
        assert_eq!(alice.s3_key_prefix, Some("alice/".to_string()));
        assert_eq!(alice.cache_timeout_secs, Some(600));

        let bob = &by_host["bob.example.com"];
        assert_eq!(bob.library_id, "lib-bob");
        assert_eq!(
            bob.s3_endpoint,
            Some("https://minio.example.com".to_string())
        );
        assert_eq!(bob.s3_key_prefix, None);
        assert_eq!(bob.cache_timeout_secs, None); // pinned
    }
}
