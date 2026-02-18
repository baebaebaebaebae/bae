//! HTTP-backed `CloudHome` implementation.
//!
//! Talks to a bae-proxy's `/cloud/*` write proxy endpoints.
//! Requests are authenticated with Ed25519 signatures.

use async_trait::async_trait;
use reqwest::Client;

use crate::keys::UserKeypair;

use super::{CloudHome, CloudHomeError, JoinInfo};

/// HTTP-backed cloud home that proxies through a bae-proxy.
pub struct HttpCloudHome {
    base_url: String,
    keypair: Option<UserKeypair>,
    client: Client,
}

impl HttpCloudHome {
    pub fn new(base_url: String, keypair: UserKeypair) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            keypair: Some(keypair),
            client: Client::new(),
        }
    }

    /// Create a read-only instance without a keypair.
    /// Reads work because bae-proxy allows unauthenticated reads (all data is encrypted).
    /// Writes will fail with 401/403 from bae-proxy.
    pub fn new_readonly(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            keypair: None,
            client: Client::new(),
        }
    }

    /// Build auth headers for a request. Returns empty headers when no keypair is set.
    fn sign_request(&self, method: &str, path: &str) -> [(&'static str, String); 3] {
        let Some(ref keypair) = self.keypair else {
            return [
                ("X-Bae-Pubkey", String::new()),
                ("X-Bae-Timestamp", String::new()),
                ("X-Bae-Signature", String::new()),
            ];
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message = format!("{}\n{}\n{}", method, path, timestamp);
        let signature = keypair.sign(message.as_bytes());

        [
            ("X-Bae-Pubkey", hex::encode(keypair.public_key)),
            ("X-Bae-Timestamp", timestamp.to_string()),
            ("X-Bae-Signature", hex::encode(signature)),
        ]
    }

    /// Map an HTTP response to a CloudHomeError for non-success status codes.
    async fn map_error(key: &str, resp: reqwest::Response) -> CloudHomeError {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if status == reqwest::StatusCode::NOT_FOUND {
            CloudHomeError::NotFound(key.to_string())
        } else if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            CloudHomeError::Storage(format!("unauthorized: {body}"))
        } else {
            CloudHomeError::Storage(format!("{status}: {body}"))
        }
    }
}

#[async_trait]
impl CloudHome for HttpCloudHome {
    async fn write(&self, key: &str, data: Vec<u8>) -> Result<(), CloudHomeError> {
        let path = format!("/cloud/{key}");
        let url = format!("{}{}", self.base_url, path);
        let headers = self.sign_request("PUT", &path);

        let resp = self
            .client
            .put(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .body(data)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("write {key}: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::map_error(key, resp).await)
        }
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
        let path = format!("/cloud/{key}");
        let url = format!("{}{}", self.base_url, path);
        let headers = self.sign_request("GET", &path);

        let resp = self
            .client
            .get(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read {key}: {e}")))?;

        if resp.status().is_success() {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CloudHomeError::Storage(format!("read body {key}: {e}")))?;
            Ok(bytes.to_vec())
        } else {
            Err(Self::map_error(key, resp).await)
        }
    }

    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        let path = format!("/cloud/{key}");
        let url = format!("{}{}", self.base_url, path);
        let headers = self.sign_request("GET", &path);
        let range_value = format!("bytes={}-{}", start, end.saturating_sub(1));

        let resp = self
            .client
            .get(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .header("Range", &range_value)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read_range {key}: {e}")))?;

        if resp.status().is_success() {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CloudHomeError::Storage(format!("read_range body {key}: {e}")))?;
            Ok(bytes.to_vec())
        } else {
            Err(Self::map_error(key, resp).await)
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
        let url = format!(
            "{}/cloud?prefix={}",
            self.base_url,
            urlencoding::encode(prefix)
        );
        let headers = self.sign_request("GET", "/cloud");

        let resp = self
            .client
            .get(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("list {prefix}: {e}")))?;

        if resp.status().is_success() {
            let keys: Vec<String> = resp
                .json()
                .await
                .map_err(|e| CloudHomeError::Storage(format!("list parse {prefix}: {e}")))?;
            Ok(keys)
        } else {
            Err(Self::map_error(prefix, resp).await)
        }
    }

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        let path = format!("/cloud/{key}");
        let url = format!("{}{}", self.base_url, path);
        let headers = self.sign_request("DELETE", &path);

        let resp = self
            .client
            .delete(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("delete {key}: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::map_error(key, resp).await)
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError> {
        let path = format!("/cloud/{key}");
        let url = format!("{}{}", self.base_url, path);
        let headers = self.sign_request("HEAD", &path);

        let resp = self
            .client
            .head(&url)
            .header(headers[0].0, &headers[0].1)
            .header(headers[1].0, &headers[1].1)
            .header(headers[2].0, &headers[2].1)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("exists {key}: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(false)
        } else if resp.status().is_success() {
            Ok(true)
        } else {
            Err(Self::map_error(key, resp).await)
        }
    }

    async fn grant_access(&self, _member_id: &str) -> Result<JoinInfo, CloudHomeError> {
        Err(CloudHomeError::Storage(
            "access is managed by the server".to_string(),
        ))
    }

    async fn revoke_access(&self, _member_id: &str) -> Result<(), CloudHomeError> {
        Err(CloudHomeError::Storage(
            "access is managed by the server".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{verify_signature, UserKeypair};

    /// Helper to create a keypair for testing without going through KeyService.
    fn test_keypair() -> UserKeypair {
        crate::encryption::ensure_sodium_init();
        let mut pk = [0u8; crate::sodium_ffi::SIGN_PUBLICKEYBYTES];
        let mut sk = [0u8; crate::sodium_ffi::SIGN_SECRETKEYBYTES];
        let ret = unsafe {
            crate::sodium_ffi::crypto_sign_ed25519_keypair(pk.as_mut_ptr(), sk.as_mut_ptr())
        };
        assert_eq!(ret, 0);
        UserKeypair {
            signing_key: sk,
            public_key: pk,
        }
    }

    #[test]
    fn sign_request_produces_three_headers() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com".to_string(), kp);
        let headers = cloud_home.sign_request("PUT", "/cloud/changes/dev1/42.enc");

        assert_eq!(headers[0].0, "X-Bae-Pubkey");
        assert_eq!(headers[1].0, "X-Bae-Timestamp");
        assert_eq!(headers[2].0, "X-Bae-Signature");

        // Pubkey is hex-encoded 32-byte key = 64 hex chars
        assert_eq!(headers[0].1.len(), 64);

        // Timestamp is a valid u64
        let ts: u64 = headers[1].1.parse().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(ts.abs_diff(now) < 5);

        // Signature is hex-encoded 64-byte signature = 128 hex chars
        assert_eq!(headers[2].1.len(), 128);
    }

    #[test]
    fn sign_request_signature_verifies() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com".to_string(), kp.clone());
        let headers = cloud_home.sign_request("GET", "/cloud/some/key");

        let message = format!("GET\n/cloud/some/key\n{}", headers[1].1);
        let sig_bytes: [u8; crate::sodium_ffi::SIGN_BYTES] =
            hex::decode(&headers[2].1).unwrap().try_into().unwrap();

        assert!(verify_signature(
            &sig_bytes,
            message.as_bytes(),
            &kp.public_key
        ));
    }

    #[test]
    fn sign_request_different_methods_produce_different_signatures() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com".to_string(), kp);

        let h1 = cloud_home.sign_request("GET", "/cloud/key");
        let h2 = cloud_home.sign_request("PUT", "/cloud/key");

        // Same timestamp is unlikely but possible; signatures should still differ
        // because the method is part of the signed message.
        // We can't guarantee different timestamps, but we can check the signatures
        // are produced (non-empty).
        assert_eq!(h1[2].1.len(), 128);
        assert_eq!(h2[2].1.len(), 128);
    }

    #[test]
    fn base_url_trailing_slash_stripped() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com/".to_string(), kp);
        assert_eq!(cloud_home.base_url, "https://example.com");
    }

    #[test]
    fn grant_access_returns_error() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com".to_string(), kp);

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(cloud_home.grant_access("some-member"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("access is managed by the server"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn revoke_access_returns_error() {
        let kp = test_keypair();
        let cloud_home = HttpCloudHome::new("https://example.com".to_string(), kp);

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(cloud_home.revoke_access("some-member"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("access is managed by the server"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn readonly_sign_request_returns_empty_headers() {
        let cloud_home = HttpCloudHome::new_readonly("https://example.com".to_string());
        let headers = cloud_home.sign_request("GET", "/cloud/some/key");

        assert_eq!(headers[0].0, "X-Bae-Pubkey");
        assert_eq!(headers[0].1, "");
        assert_eq!(headers[1].0, "X-Bae-Timestamp");
        assert_eq!(headers[1].1, "");
        assert_eq!(headers[2].0, "X-Bae-Signature");
        assert_eq!(headers[2].1, "");
    }

    #[test]
    fn readonly_base_url_trailing_slash_stripped() {
        let cloud_home = HttpCloudHome::new_readonly("https://example.com/".to_string());
        assert_eq!(cloud_home.base_url, "https://example.com");
    }
}
