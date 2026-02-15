use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Payload encoded inside a follow code.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FollowPayload {
    /// Server URL (e.g. "http://192.168.1.100:4533")
    url: String,
    /// Username for authentication
    user: String,
    /// Password for authentication
    pass: String,
    /// Optional display name for the library
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Encode follow credentials into an opaque follow code string.
pub fn encode(server_url: &str, username: &str, password: &str, name: Option<&str>) -> String {
    let payload = FollowPayload {
        url: server_url.to_string(),
        user: username.to_string(),
        pass: password.to_string(),
        name: name.map(|s| s.to_string()),
    };
    let json = serde_json::to_string(&payload).expect("FollowPayload serialization cannot fail");
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

/// Decode a follow code into its components.
/// Returns (server_url, username, password, name).
pub fn decode(code: &str) -> Result<(String, String, String, Option<String>), FollowCodeError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(code.trim())
        .map_err(|_| FollowCodeError::InvalidBase64)?;
    let payload: FollowPayload =
        serde_json::from_slice(&bytes).map_err(|e| FollowCodeError::InvalidJson(e.to_string()))?;
    Ok((payload.url, payload.user, payload.pass, payload.name))
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
        let code = encode(
            "http://192.168.1.100:4533",
            "listener",
            "secret123",
            Some("Friend's Library"),
        );
        let (url, user, pass, name) = decode(&code).unwrap();
        assert_eq!(url, "http://192.168.1.100:4533");
        assert_eq!(user, "listener");
        assert_eq!(pass, "secret123");
        assert_eq!(name, Some("Friend's Library".to_string()));
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
        let code = encode("http://localhost:4533", "admin", "pass", None);
        let (url, user, pass, name) = decode(&code).unwrap();
        assert_eq!(url, "http://localhost:4533");
        assert_eq!(user, "admin");
        assert_eq!(pass, "pass");
        assert_eq!(name, None);
    }

    #[test]
    fn decode_trims_whitespace() {
        let code = encode("http://example.com:4533", "user", "pw", None);
        let padded = format!("  {} \n", code);
        let (url, _, _, _) = decode(&padded).unwrap();
        assert_eq!(url, "http://example.com:4533");
    }
}
