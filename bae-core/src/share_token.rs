use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use thiserror::Error;
use uuid::Uuid;

use crate::encryption::EncryptionService;
use crate::hmac_utils::{hmac_sign, hmac_verify};

const TOKEN_LEN: usize = 56; // 16 (UUID) + 8 (expiry) + 32 (HMAC)
const PAYLOAD_LEN: usize = 24; // 16 (UUID) + 8 (expiry)
const SIGNING_INFO: &str = "bae-share-link-v1";

#[derive(Debug)]
pub struct ShareTokenPayload {
    pub track_id: String,
    pub expiry: Option<u64>,
}

#[derive(Error, Debug)]
pub enum ShareTokenError {
    #[error("Invalid track ID: {0}")]
    InvalidTrackId(String),
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token has expired")]
    Expired,
    #[error("Invalid signature")]
    InvalidSignature,
}

/// Generate a share token for a track.
///
/// The token is a base64url-encoded binary blob:
/// `[track_id: 16 bytes UUID] [expiry: 8 bytes u64 BE, 0 = no expiry] [signature: 32 bytes HMAC-SHA256]`
pub fn generate_share_token(
    encryption: &EncryptionService,
    track_id: &str,
    expiry: Option<u64>,
) -> Result<String, ShareTokenError> {
    let uuid = Uuid::parse_str(track_id)
        .map_err(|_| ShareTokenError::InvalidTrackId(track_id.to_string()))?;

    let mut payload = [0u8; PAYLOAD_LEN];
    payload[..16].copy_from_slice(uuid.as_bytes());
    payload[16..24].copy_from_slice(&expiry.unwrap_or(0).to_be_bytes());

    let signing_key = encryption.derive_key(SIGNING_INFO);
    let signature = hmac_sign(&signing_key, &payload);

    let mut token_bytes = Vec::with_capacity(TOKEN_LEN);
    token_bytes.extend_from_slice(&payload);
    token_bytes.extend_from_slice(&signature);

    Ok(URL_SAFE_NO_PAD.encode(&token_bytes))
}

/// Validate a share token and extract its payload.
pub fn validate_share_token(
    encryption: &EncryptionService,
    token: &str,
) -> Result<ShareTokenPayload, ShareTokenError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| ShareTokenError::InvalidToken)?;

    if bytes.len() != TOKEN_LEN {
        return Err(ShareTokenError::InvalidToken);
    }

    let payload = &bytes[..PAYLOAD_LEN];
    let signature = &bytes[PAYLOAD_LEN..];

    let signing_key = encryption.derive_key(SIGNING_INFO);
    if !hmac_verify(&signing_key, payload, signature) {
        return Err(ShareTokenError::InvalidSignature);
    }

    let uuid = Uuid::from_bytes(payload[..16].try_into().expect("16 bytes"));
    let expiry_raw = u64::from_be_bytes(payload[16..24].try_into().expect("8 bytes"));

    if expiry_raw > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_secs();

        if now > expiry_raw {
            return Err(ShareTokenError::Expired);
        }
    }

    let expiry = if expiry_raw == 0 {
        None
    } else {
        Some(expiry_raw)
    };

    Ok(ShareTokenPayload {
        track_id: uuid.to_string(),
        expiry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_encryption() -> EncryptionService {
        EncryptionService::new_with_key(&[0xAA; 32])
    }

    #[test]
    fn test_roundtrip() {
        let enc = test_encryption();
        let track_id = Uuid::new_v4().to_string();

        let token = generate_share_token(&enc, &track_id, None).unwrap();
        let payload = validate_share_token(&enc, &token).unwrap();

        assert_eq!(payload.track_id, track_id);
    }

    #[test]
    fn test_roundtrip_with_expiry() {
        let enc = test_encryption();
        let track_id = Uuid::new_v4().to_string();
        let future_expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600; // 1 hour from now

        let token = generate_share_token(&enc, &track_id, Some(future_expiry)).unwrap();
        let payload = validate_share_token(&enc, &token).unwrap();

        assert_eq!(payload.track_id, track_id);
        assert_eq!(payload.expiry, Some(future_expiry));
    }

    #[test]
    fn test_expired_token() {
        let enc = test_encryption();
        let track_id = Uuid::new_v4().to_string();
        let past_expiry = 1; // 1 second after epoch -- definitely in the past

        let token = generate_share_token(&enc, &track_id, Some(past_expiry)).unwrap();
        let err = validate_share_token(&enc, &token).unwrap_err();

        assert!(matches!(err, ShareTokenError::Expired));
    }

    #[test]
    fn test_no_expiry() {
        let enc = test_encryption();
        let track_id = Uuid::new_v4().to_string();

        let token = generate_share_token(&enc, &track_id, None).unwrap();
        let payload = validate_share_token(&enc, &token).unwrap();

        assert!(payload.expiry.is_none());
    }

    #[test]
    fn test_tampered_token() {
        let enc = test_encryption();
        let track_id = Uuid::new_v4().to_string();

        let token = generate_share_token(&enc, &track_id, None).unwrap();

        // Decode, flip a byte, re-encode
        let mut bytes = URL_SAFE_NO_PAD.decode(&token).unwrap();
        bytes[0] ^= 0xFF;
        let tampered = URL_SAFE_NO_PAD.encode(&bytes);

        let err = validate_share_token(&enc, &tampered).unwrap_err();
        assert!(matches!(err, ShareTokenError::InvalidSignature));
    }

    #[test]
    fn test_different_key_rejects() {
        let enc_a = EncryptionService::new_with_key(&[0xAA; 32]);
        let enc_b = EncryptionService::new_with_key(&[0xBB; 32]);
        let track_id = Uuid::new_v4().to_string();

        let token = generate_share_token(&enc_a, &track_id, None).unwrap();
        let err = validate_share_token(&enc_b, &token).unwrap_err();

        assert!(matches!(err, ShareTokenError::InvalidSignature));
    }

    #[test]
    fn test_invalid_base64() {
        let enc = test_encryption();
        let err = validate_share_token(&enc, "not valid base64!!!").unwrap_err();
        assert!(matches!(err, ShareTokenError::InvalidToken));
    }

    #[test]
    fn test_wrong_length() {
        let enc = test_encryption();
        // Valid base64 but wrong byte count (only 16 bytes)
        let short = URL_SAFE_NO_PAD.encode([0u8; 16]);
        let err = validate_share_token(&enc, &short).unwrap_err();
        assert!(matches!(err, ShareTokenError::InvalidToken));
    }

    #[test]
    fn test_invalid_track_id() {
        let enc = test_encryption();
        let err = generate_share_token(&enc, "not-a-uuid", None).unwrap_err();
        assert!(matches!(err, ShareTokenError::InvalidTrackId(_)));
    }
}
