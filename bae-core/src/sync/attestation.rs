/// Attestations: signed statements linking MusicBrainz release IDs to BitTorrent infohashes.
///
/// When a user imports a release and matches it to a MusicBrainz release ID,
/// they can sign an attestation that links the MBID to the torrent infohash and
/// a deterministic content hash of the release's files. These attestations are
/// self-contained and verifiable without external context.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::keys::{self, UserKeypair};

#[derive(Debug, thiserror::Error)]
pub enum AttestationError {
    #[error("invalid author_pubkey hex: {0}")]
    InvalidPubkey(String),
    #[error("invalid signature hex: {0}")]
    InvalidSignature(String),
    #[error("signature verification failed")]
    VerificationFailed,
    #[error("database error: {0}")]
    Database(String),
}

/// A signed attestation linking a MusicBrainz release ID to a BitTorrent infohash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    /// MusicBrainz release ID
    pub mbid: String,
    /// BitTorrent infohash of the release files
    pub infohash: String,
    /// SHA-256 of the ordered file hashes (content fingerprint)
    pub content_hash: String,
    /// Audio format, e.g. "FLAC", "MP3 320"
    pub format: String,
    /// Hex-encoded Ed25519 public key of the author
    pub author_pubkey: String,
    /// RFC 3339 timestamp
    pub timestamp: String,
    /// Hex-encoded Ed25519 detached signature
    pub signature: String,
}

/// Deterministic serialization of the signed fields (everything except signature).
///
/// Uses serde_json::json! which produces alphabetically sorted keys,
/// matching the pattern in membership.rs.
pub fn canonical_bytes(attestation: &Attestation) -> Vec<u8> {
    let canonical = serde_json::json!({
        "author_pubkey": attestation.author_pubkey,
        "content_hash": attestation.content_hash,
        "format": attestation.format,
        "infohash": attestation.infohash,
        "mbid": attestation.mbid,
        "timestamp": attestation.timestamp,
    });
    serde_json::to_vec(&canonical).expect("canonical serialization cannot fail")
}

/// Create a signed attestation.
pub fn create_attestation(
    mbid: &str,
    infohash: &str,
    content_hash: &str,
    format: &str,
    keypair: &UserKeypair,
    timestamp: &str,
) -> Attestation {
    let mut attestation = Attestation {
        mbid: mbid.to_string(),
        infohash: infohash.to_string(),
        content_hash: content_hash.to_string(),
        format: format.to_string(),
        author_pubkey: hex::encode(keypair.public_key),
        timestamp: timestamp.to_string(),
        signature: String::new(),
    };

    let bytes = canonical_bytes(&attestation);
    let sig = keypair.sign(&bytes);
    attestation.signature = hex::encode(sig);
    attestation
}

/// Verify the signature on an attestation.
pub fn verify_attestation(attestation: &Attestation) -> Result<(), AttestationError> {
    let pk_bytes: [u8; keys::SIGN_PUBLICKEYBYTES] = hex::decode(&attestation.author_pubkey)
        .map_err(|e| AttestationError::InvalidPubkey(e.to_string()))?
        .try_into()
        .map_err(|_| AttestationError::InvalidPubkey("wrong length".to_string()))?;

    let sig_bytes: [u8; keys::SIGN_BYTES] = hex::decode(&attestation.signature)
        .map_err(|e| AttestationError::InvalidSignature(e.to_string()))?
        .try_into()
        .map_err(|_| AttestationError::InvalidSignature("wrong length".to_string()))?;

    let bytes = canonical_bytes(attestation);
    if keys::verify_signature(&sig_bytes, &bytes, &pk_bytes) {
        Ok(())
    } else {
        Err(AttestationError::VerificationFailed)
    }
}

/// Compute a deterministic content hash from ordered file SHA-256 hashes.
///
/// Takes a sorted list of hex-encoded SHA-256 hashes, concatenates the raw bytes,
/// and hashes the result with SHA-256. This fingerprints a release's file contents
/// regardless of filenames.
pub fn compute_content_hash(file_hashes: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for hash_hex in file_hashes {
        // Decode hex to raw bytes so the hash is over bytes, not ASCII
        let bytes = hex::decode(hash_hex).expect("file hashes must be valid hex");
        hasher.update(&bytes);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gen_keypair() -> UserKeypair {
        UserKeypair::generate()
    }

    #[test]
    fn create_and_verify_roundtrip() {
        let kp = gen_keypair();
        let att = create_attestation(
            "12345678-1234-1234-1234-123456789012",
            "aabbccdd",
            "content_hash_hex",
            "FLAC",
            &kp,
            "2026-02-10T14:30:00Z",
        );

        verify_attestation(&att).expect("valid attestation should verify");
    }

    #[test]
    fn tampered_attestation_fails_verification() {
        let kp = gen_keypair();
        let mut att = create_attestation(
            "12345678-1234-1234-1234-123456789012",
            "aabbccdd",
            "content_hash_hex",
            "FLAC",
            &kp,
            "2026-02-10T14:30:00Z",
        );

        // Tamper with the format after signing
        att.format = "MP3 320".to_string();
        let result = verify_attestation(&att);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_key_fails_verification() {
        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let mut att = create_attestation(
            "mbid",
            "infohash",
            "content_hash",
            "FLAC",
            &kp1,
            "2026-02-10T14:30:00Z",
        );

        // Replace pubkey with a different key (signature won't match)
        att.author_pubkey = hex::encode(kp2.public_key);
        let result = verify_attestation(&att);
        assert!(result.is_err());
    }

    #[test]
    fn canonical_bytes_is_deterministic() {
        let att = Attestation {
            mbid: "mbid-1".to_string(),
            infohash: "infohash-1".to_string(),
            content_hash: "ch-1".to_string(),
            format: "FLAC".to_string(),
            author_pubkey: "aabb".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            signature: "does-not-matter".to_string(),
        };

        let b1 = canonical_bytes(&att);
        let b2 = canonical_bytes(&att);
        assert_eq!(b1, b2);

        // Signature is not included in canonical bytes
        let mut att2 = att.clone();
        att2.signature = "something-else".to_string();
        assert_eq!(canonical_bytes(&att2), b1);
    }

    #[test]
    fn content_hash_determinism() {
        let hashes = [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        ];

        let h1 = compute_content_hash(&hashes);
        let h2 = compute_content_hash(&hashes);
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_changes_with_different_files() {
        let hashes_a = [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        ];
        let hashes_b = [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ];

        assert_ne!(
            compute_content_hash(&hashes_a),
            compute_content_hash(&hashes_b)
        );
    }

    #[test]
    fn content_hash_changes_with_order() {
        let hashes_a = [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        ];
        let hashes_b = [
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ];

        // Order matters -- callers must sort before calling
        assert_ne!(
            compute_content_hash(&hashes_a),
            compute_content_hash(&hashes_b)
        );
    }
}
