/// Share grants: give someone access to one release from your library.
///
/// A share grant is a self-contained token (serializable to JSON) that can be
/// passed out-of-band (paste, QR, file). It contains:
/// - Bucket coordinates (where the release files live)
/// - A wrapped payload encrypted to the recipient's X25519 key containing:
///   - The per-release derived encryption key
///   - Optional S3 credentials for accessing the bucket
///
/// The grant is signed by the sender's Ed25519 key so the recipient can verify
/// authenticity.
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::encryption::EncryptionService;
use crate::keys::{self, KeyError, UserKeypair};
use crate::sodium_ffi;

/// A share grant giving access to one release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareGrant {
    pub from_library_id: String,
    pub from_user_pubkey: String, // hex-encoded Ed25519 public key
    pub release_id: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    /// Release key + optional S3 creds, sealed-box encrypted to recipient's X25519 key.
    #[serde(with = "hex_vec")]
    pub wrapped_payload: Vec<u8>,
    /// RFC 3339 expiry timestamp, or None for no expiry.
    pub expires: Option<String>,
    /// Hex-encoded Ed25519 signature over the canonical bytes.
    pub signature: String,
}

/// The inner payload encrypted to the recipient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantPayload {
    #[serde(with = "hex_array_32")]
    pub release_key: [u8; 32],
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShareGrantError {
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Key error: {0}")]
    Key(#[from] KeyError),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Grant has expired")]
    Expired,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Deterministic serialization of the signed fields.
///
/// Covers everything except `signature` (which is what we're computing).
/// The `wrapped_payload` is included because it's bound to the recipient --
/// tampering with it (e.g., replacing with a payload for a different recipient)
/// invalidates the signature.
fn canonical_bytes(grant: &ShareGrant) -> Vec<u8> {
    let canonical = serde_json::json!({
        "bucket": grant.bucket,
        "endpoint": grant.endpoint,
        "expires": grant.expires,
        "from_library_id": grant.from_library_id,
        "from_user_pubkey": grant.from_user_pubkey,
        "region": grant.region,
        "release_id": grant.release_id,
        "wrapped_payload": hex::encode(&grant.wrapped_payload),
    });
    serde_json::to_vec(&canonical).expect("canonical serialization cannot fail")
}

/// Create a share grant for a release.
///
/// Derives the per-release key, wraps it (+ optional S3 creds) to the
/// recipient's X25519 key, and signs the grant.
pub fn create_share_grant(
    sender_keypair: &UserKeypair,
    recipient_ed25519_pubkey_hex: &str,
    encryption_service: &EncryptionService,
    from_library_id: &str,
    release_id: &str,
    bucket: &str,
    region: &str,
    endpoint: Option<&str>,
    s3_access_key: Option<&str>,
    s3_secret_key: Option<&str>,
    expires: Option<&str>,
) -> Result<ShareGrant, ShareGrantError> {
    // Decode and convert recipient's Ed25519 pubkey to X25519.
    let ed25519_pk: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] =
        hex::decode(recipient_ed25519_pubkey_hex)
            .map_err(|e| ShareGrantError::Crypto(format!("invalid recipient pubkey hex: {e}")))?
            .try_into()
            .map_err(|_| ShareGrantError::Crypto("recipient pubkey wrong length".to_string()))?;
    let x25519_pk = keys::ed25519_to_x25519_public_key(&ed25519_pk);

    // Derive the per-release key.
    let release_key = encryption_service.derive_release_key(release_id);

    // Build and serialize the payload.
    let payload = GrantPayload {
        release_key,
        s3_access_key: s3_access_key.map(|s| s.to_string()),
        s3_secret_key: s3_secret_key.map(|s| s.to_string()),
    };
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| ShareGrantError::Serialization(format!("payload: {e}")))?;

    // Encrypt to recipient's X25519 key.
    let wrapped_payload = keys::seal_box_encrypt(&payload_bytes, &x25519_pk);

    // Build the grant (signature placeholder).
    let mut grant = ShareGrant {
        from_library_id: from_library_id.to_string(),
        from_user_pubkey: hex::encode(sender_keypair.public_key),
        release_id: release_id.to_string(),
        bucket: bucket.to_string(),
        region: region.to_string(),
        endpoint: endpoint.map(|s| s.to_string()),
        wrapped_payload,
        expires: expires.map(|s| s.to_string()),
        signature: String::new(),
    };

    // Sign.
    let bytes = canonical_bytes(&grant);
    let sig = sender_keypair.sign(&bytes);
    grant.signature = hex::encode(sig);

    Ok(grant)
}

/// Accept a share grant: verify the signature, check expiry, unwrap the payload.
///
/// Returns the decrypted `GrantPayload` containing the release key and
/// optional S3 credentials.
pub fn accept_share_grant(
    grant: &ShareGrant,
    recipient_keypair: &UserKeypair,
) -> Result<GrantPayload, ShareGrantError> {
    // Verify signature.
    let pk_bytes: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] = hex::decode(&grant.from_user_pubkey)
        .map_err(|e| ShareGrantError::Crypto(format!("invalid sender pubkey hex: {e}")))?
        .try_into()
        .map_err(|_| ShareGrantError::Crypto("sender pubkey wrong length".to_string()))?;

    let sig_bytes: [u8; sodium_ffi::SIGN_BYTES] = hex::decode(&grant.signature)
        .map_err(|e| ShareGrantError::Crypto(format!("invalid signature hex: {e}")))?
        .try_into()
        .map_err(|_| ShareGrantError::Crypto("signature wrong length".to_string()))?;

    let bytes = canonical_bytes(grant);
    if !keys::verify_signature(&sig_bytes, &bytes, &pk_bytes) {
        return Err(ShareGrantError::InvalidSignature);
    }

    // Check expiry.
    if let Some(expires) = &grant.expires {
        let expiry = chrono::DateTime::parse_from_rfc3339(expires)
            .map_err(|e| ShareGrantError::Crypto(format!("invalid expiry timestamp: {e}")))?;
        if Utc::now() > expiry {
            return Err(ShareGrantError::Expired);
        }
    }

    // Decrypt the wrapped payload.
    let x25519_pk = recipient_keypair.to_x25519_public_key();
    let x25519_sk = recipient_keypair.to_x25519_secret_key();

    let plaintext = keys::seal_box_decrypt(&grant.wrapped_payload, &x25519_pk, &x25519_sk)?;

    let payload: GrantPayload = serde_json::from_slice(&plaintext)
        .map_err(|e| ShareGrantError::Serialization(format!("payload: {e}")))?;

    Ok(payload)
}

/// Serde helper for Vec<u8> as hex string.
mod hex_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde helper for [u8; 32] as hex string.
mod hex_array_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::EncryptionService;
    use crate::keys::UserKeypair;
    use crate::sodium_ffi;

    fn gen_keypair() -> UserKeypair {
        crate::encryption::ensure_sodium_init();
        let mut pk = [0u8; sodium_ffi::SIGN_PUBLICKEYBYTES];
        let mut sk = [0u8; sodium_ffi::SIGN_SECRETKEYBYTES];
        let ret =
            unsafe { sodium_ffi::crypto_sign_ed25519_keypair(pk.as_mut_ptr(), sk.as_mut_ptr()) };
        assert_eq!(ret, 0);
        UserKeypair {
            signing_key: sk,
            public_key: pk,
        }
    }

    fn test_encryption_service() -> EncryptionService {
        EncryptionService::new_with_key(&[42u8; 32])
    }

    #[test]
    fn create_and_accept_roundtrip() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();
        let release_id = "rel-123";

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-abc",
            release_id,
            "my-bucket",
            "us-east-1",
            Some("https://s3.example.com"),
            Some("AKID"),
            Some("secret123"),
            None,
        )
        .unwrap();

        // Verify basic fields.
        assert_eq!(grant.from_library_id, "lib-abc");
        assert_eq!(grant.release_id, release_id);
        assert_eq!(grant.bucket, "my-bucket");
        assert_eq!(grant.region, "us-east-1");
        assert_eq!(grant.endpoint.as_deref(), Some("https://s3.example.com"));
        assert!(!grant.signature.is_empty());

        // Accept.
        let payload = accept_share_grant(&grant, &recipient).unwrap();
        assert_eq!(payload.release_key, enc.derive_release_key(release_id));
        assert_eq!(payload.s3_access_key.as_deref(), Some("AKID"));
        assert_eq!(payload.s3_secret_key.as_deref(), Some("secret123"));
    }

    #[test]
    fn accept_with_wrong_keypair_fails() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let wrong = gen_keypair();
        let enc = test_encryption_service();

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let result = accept_share_grant(&grant, &wrong);
        assert!(result.is_err());
    }

    #[test]
    fn tampered_grant_fails_signature() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        let mut grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        // Tamper with a field.
        grant.release_id = "rel-TAMPERED".to_string();

        let result = accept_share_grant(&grant, &recipient);
        assert!(matches!(result, Err(ShareGrantError::InvalidSignature)));
    }

    #[test]
    fn expired_grant_is_rejected() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        // Expire in the past.
        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            Some("2020-01-01T00:00:00Z"),
        )
        .unwrap();

        let result = accept_share_grant(&grant, &recipient);
        assert!(matches!(result, Err(ShareGrantError::Expired)));
    }

    #[test]
    fn grant_without_s3_creds() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None, // no access key
            None, // no secret key
            None,
        )
        .unwrap();

        let payload = accept_share_grant(&grant, &recipient).unwrap();
        assert!(payload.s3_access_key.is_none());
        assert!(payload.s3_secret_key.is_none());
        // Release key should still be correct.
        assert_eq!(payload.release_key, enc.derive_release_key("rel-1"));
    }

    #[test]
    fn grant_serializes_to_json_roundtrip() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            Some("https://s3.example.com"),
            Some("AK"),
            Some("SK"),
            None,
        )
        .unwrap();

        // Serialize to JSON and back.
        let json = serde_json::to_string(&grant).unwrap();
        let deserialized: ShareGrant = serde_json::from_str(&json).unwrap();

        // Accept the deserialized grant.
        let payload = accept_share_grant(&deserialized, &recipient).unwrap();
        assert_eq!(payload.release_key, enc.derive_release_key("rel-1"));
        assert_eq!(payload.s3_access_key.as_deref(), Some("AK"));
    }

    #[test]
    fn grant_with_future_expiry_accepted() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            Some("2099-12-31T23:59:59Z"),
        )
        .unwrap();

        let payload = accept_share_grant(&grant, &recipient).unwrap();
        assert_eq!(payload.release_key, enc.derive_release_key("rel-1"));
    }

    #[test]
    fn canonical_bytes_excludes_signature() {
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        let grant = create_share_grant(
            &sender,
            &hex::encode(recipient.public_key),
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let bytes1 = canonical_bytes(&grant);

        // Changing the signature should NOT change canonical bytes.
        let mut grant2 = grant.clone();
        grant2.signature = "0000".to_string();
        let bytes2 = canonical_bytes(&grant2);

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn invalid_recipient_pubkey_hex() {
        let sender = gen_keypair();
        let enc = test_encryption_service();

        let result = create_share_grant(
            &sender,
            "not-valid-hex",
            &enc,
            "lib-1",
            "rel-1",
            "bucket",
            "region",
            None,
            None,
            None,
            None,
        );

        assert!(matches!(result, Err(ShareGrantError::Crypto(_))));
    }
}
