use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Registry {
    pub libraries: Vec<LibraryEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LibraryEntry {
    pub hostname: String,
    pub library_id: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_key_prefix: String,
    pub ed25519_pubkey: Option<String>,
}

impl Registry {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read registry: {e}"))?;
        serde_yaml::from_str(&content).map_err(|e| format!("parse registry: {e}"))
    }

    pub fn find_by_hostname(&self, hostname: &str) -> Option<&LibraryEntry> {
        self.libraries.iter().find(|l| l.hostname == hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_yaml() -> &'static str {
        r#"
libraries:
  - hostname: alice.bae.fm
    library_id: lib-alice
    s3_bucket: bae-production
    s3_region: falkenstein
    s3_endpoint: https://fsn1.your-objectstorage.com
    s3_access_key: AKIA123
    s3_secret_key: secret123
    s3_key_prefix: libraries/lib-alice/
    ed25519_pubkey: aabbccdd0011223344556677889900aabbccdd0011223344556677889900aabb
  - hostname: bob.bae.fm
    library_id: lib-bob
    s3_bucket: bae-production
    s3_region: falkenstein
    s3_endpoint: https://fsn1.your-objectstorage.com
    s3_access_key: AKIA456
    s3_secret_key: secret456
    s3_key_prefix: libraries/lib-bob/
    ed25519_pubkey: null
"#
    }

    #[test]
    fn parse_registry_yaml() {
        let registry: Registry = serde_yaml::from_str(sample_yaml()).unwrap();
        assert_eq!(registry.libraries.len(), 2);

        let alice = &registry.libraries[0];
        assert_eq!(alice.hostname, "alice.bae.fm");
        assert_eq!(alice.library_id, "lib-alice");
        assert_eq!(alice.s3_bucket, "bae-production");
        assert_eq!(alice.s3_region, "falkenstein");
        assert_eq!(alice.s3_endpoint, "https://fsn1.your-objectstorage.com");
        assert_eq!(alice.s3_key_prefix, "libraries/lib-alice/");
        assert!(alice.ed25519_pubkey.is_some());

        let bob = &registry.libraries[1];
        assert_eq!(bob.hostname, "bob.bae.fm");
        assert_eq!(bob.s3_key_prefix, "libraries/lib-bob/");
    }

    #[test]
    fn find_by_hostname() {
        let registry: Registry = serde_yaml::from_str(sample_yaml()).unwrap();

        let found = registry.find_by_hostname("alice.bae.fm");
        assert!(found.is_some());
        assert_eq!(found.unwrap().library_id, "lib-alice");

        let missing = registry.find_by_hostname("unknown.bae.fm");
        assert!(missing.is_none());
    }

    #[test]
    fn missing_pubkey() {
        let registry: Registry = serde_yaml::from_str(sample_yaml()).unwrap();

        let bob = registry.find_by_hostname("bob.bae.fm").unwrap();
        assert!(bob.ed25519_pubkey.is_none());

        let alice = registry.find_by_hostname("alice.bae.fm").unwrap();
        assert!(alice.ed25519_pubkey.is_some());
    }
}
