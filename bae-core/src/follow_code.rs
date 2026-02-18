use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Payload encoded inside a follow code.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FollowPayload {
    /// Server base URL (e.g. "https://alice.bae.fm")
    url: String,
    /// Base64-encoded library encryption key
    key: String,
    /// Optional display name for the library
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Encode follow credentials into an opaque follow code string.
pub fn encode(proxy_url: &str, encryption_key: &[u8], name: Option<&str>) -> String {
    let payload = FollowPayload {
        url: proxy_url.to_string(),
        key: URL_SAFE_NO_PAD.encode(encryption_key),
        name: name.map(|s| s.to_string()),
    };
    let json = serde_json::to_string(&payload).expect("FollowPayload serialization cannot fail");
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

/// Decode a follow code into its components.
/// Returns (proxy_url, encryption_key_bytes, name).
pub fn decode(code: &str) -> Result<(String, Vec<u8>, Option<String>), FollowCodeError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(code.trim())
        .map_err(|_| FollowCodeError::InvalidBase64)?;
    let payload: FollowPayload =
        serde_json::from_slice(&bytes).map_err(|e| FollowCodeError::InvalidJson(e.to_string()))?;
    let key_bytes = URL_SAFE_NO_PAD
        .decode(&payload.key)
        .map_err(|_| FollowCodeError::InvalidBase64)?;
    Ok((payload.url, key_bytes, payload.name))
}

#[derive(Debug, thiserror::Error)]
pub enum FollowCodeError {
    #[error("invalid base64url encoding")]
    InvalidBase64,
    #[error("invalid follow code payload: {0}")]
    InvalidJson(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let key = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
        let code = encode("https://alice.bae.fm", &key, Some("Test Library"));
        let (url, decoded_key, name) = decode(&code).unwrap();
        assert_eq!(url, "https://alice.bae.fm");
        assert_eq!(decoded_key, key);
        assert_eq!(name, Some("Test Library".to_string()));
    }

    #[test]
    fn decode_invalid_base64() {
        assert!(matches!(
            decode("not-valid!!!"),
            Err(FollowCodeError::InvalidBase64)
        ));
    }

    #[test]
    fn decode_invalid_json() {
        let encoded = URL_SAFE_NO_PAD.encode(b"not json");
        assert!(matches!(
            decode(&encoded),
            Err(FollowCodeError::InvalidJson(_))
        ));
    }

    #[test]
    fn name_is_optional() {
        let key = vec![0x01, 0x02, 0x03];
        let code = encode("https://example.com", &key, None);
        let (url, decoded_key, name) = decode(&code).unwrap();
        assert_eq!(url, "https://example.com");
        assert_eq!(decoded_key, key);
        assert_eq!(name, None);
    }

    #[test]
    fn decode_trims_whitespace() {
        let key = vec![0x01, 0x02, 0x03];
        let code = encode("https://example.com", &key, None);
        let padded = format!("  {} \n", code);
        let (url, _, _) = decode(&padded).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn roundtrip_32_byte_key() {
        let key = vec![0xAB; 32];
        let code = encode("https://proxy.example.com", &key, Some("Full Key"));
        let (_, decoded_key, _) = decode(&code).unwrap();
        assert_eq!(decoded_key.len(), 32);
        assert_eq!(decoded_key, key);
    }
}
