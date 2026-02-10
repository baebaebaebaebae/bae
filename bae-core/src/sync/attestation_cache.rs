/// Attestation cache and exchange primitives.
///
/// Provides DB-backed storage and retrieval of attestations received from peers,
/// plus a JSON Lines wire format for serializing attestations over the network.
use tracing::warn;
use uuid::Uuid;

use crate::db::{Database, DbAttestation};
use crate::sync::attestation::{verify_attestation, Attestation, AttestationError};

/// DB-backed cache of attestations. Wraps the existing Database attestation
/// CRUD with signature verification on ingest.
pub struct AttestationCache<'a> {
    db: &'a Database,
}

impl<'a> AttestationCache<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Verify and store an attestation. Invalid signatures are rejected.
    /// Idempotent: the DB uses INSERT OR REPLACE with a unique index on
    /// (mbid, infohash, author_pubkey).
    pub async fn store_attestation(
        &self,
        attestation: &Attestation,
    ) -> Result<(), AttestationError> {
        verify_attestation(attestation)?;

        let db_att = to_db_attestation(attestation);
        self.db
            .insert_attestation(&db_att)
            .await
            .map_err(|e| AttestationError::InvalidPubkey(format!("db error: {e}")))?;
        Ok(())
    }

    /// Retrieve all attestations for a MusicBrainz release ID.
    pub async fn get_attestations_for_mbid(
        &self,
        mbid: &str,
    ) -> Result<Vec<Attestation>, AttestationError> {
        let rows = self
            .db
            .get_attestations_by_mbid(mbid)
            .await
            .map_err(|e| AttestationError::InvalidPubkey(format!("db error: {e}")))?;
        Ok(rows.into_iter().map(from_db_attestation).collect())
    }

    /// Retrieve all attestations for a BitTorrent infohash.
    pub async fn get_attestations_for_infohash(
        &self,
        infohash: &str,
    ) -> Result<Vec<Attestation>, AttestationError> {
        let rows = self
            .db
            .get_attestations_by_infohash(infohash)
            .await
            .map_err(|e| AttestationError::InvalidPubkey(format!("db error: {e}")))?;
        Ok(rows.into_iter().map(from_db_attestation).collect())
    }

    /// Verify and store multiple attestations from a remote peer.
    /// Invalid attestations are skipped with a warning (not treated as a batch failure).
    pub async fn merge_remote_attestations(&self, attestations: &[Attestation]) -> MergeResult {
        let mut stored = 0usize;
        let mut rejected = 0usize;

        for att in attestations {
            match self.store_attestation(att).await {
                Ok(()) => stored += 1,
                Err(e) => {
                    warn!(
                        "Rejected attestation from {} for mbid={}: {e}",
                        att.author_pubkey, att.mbid,
                    );

                    rejected += 1;
                }
            }
        }

        MergeResult { stored, rejected }
    }

    /// Number of distinct signers attesting to a specific mbid+infohash mapping.
    pub async fn get_confidence(
        &self,
        mbid: &str,
        infohash: &str,
    ) -> Result<usize, AttestationError> {
        self.db
            .get_attestation_confidence(mbid, infohash)
            .await
            .map_err(|e| AttestationError::InvalidPubkey(format!("db error: {e}")))
    }
}

/// Result of merging a batch of remote attestations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub stored: usize,
    pub rejected: usize,
}

// ---------------------------------------------------------------------------
// Wire format: JSON Lines
// ---------------------------------------------------------------------------

/// Serialize attestations to JSON Lines (one JSON object per line).
pub fn serialize_attestations(attestations: &[Attestation]) -> Vec<u8> {
    let mut buf = Vec::new();
    for att in attestations {
        if !buf.is_empty() {
            buf.push(b'\n');
        }
        // serde_json::to_vec produces compact single-line JSON (no trailing newline)
        let line = serde_json::to_vec(att).expect("attestation serialization cannot fail");
        buf.extend_from_slice(&line);
    }
    buf
}

/// Deserialize attestations from JSON Lines, verifying each signature.
pub fn deserialize_attestations(data: &[u8]) -> Result<Vec<Attestation>, AttestationError> {
    let text = std::str::from_utf8(data)
        .map_err(|e| AttestationError::InvalidSignature(format!("invalid utf8: {e}")))?;

    let mut attestations = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        let att: Attestation = serde_json::from_str(line)
            .map_err(|e| AttestationError::InvalidSignature(format!("invalid json: {e}")))?;
        verify_attestation(&att)?;
        attestations.push(att);
    }
    Ok(attestations)
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn to_db_attestation(att: &Attestation) -> DbAttestation {
    DbAttestation {
        id: Uuid::new_v4().to_string(),
        mbid: att.mbid.clone(),
        infohash: att.infohash.clone(),
        content_hash: att.content_hash.clone(),
        format: att.format.clone(),
        author_pubkey: att.author_pubkey.clone(),
        timestamp: att.timestamp.clone(),
        signature: att.signature.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn from_db_attestation(row: DbAttestation) -> Attestation {
    Attestation {
        mbid: row.mbid,
        infohash: row.infohash,
        content_hash: row.content_hash,
        format: row.format,
        author_pubkey: row.author_pubkey,
        timestamp: row.timestamp,
        signature: row.signature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::ensure_sodium_init;
    use crate::keys::UserKeypair;
    use crate::sodium_ffi;
    use crate::sync::attestation::create_attestation;
    use tempfile::TempDir;

    fn gen_keypair() -> UserKeypair {
        ensure_sodium_init();
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

    fn make_attestation(kp: &UserKeypair, mbid: &str, infohash: &str) -> Attestation {
        create_attestation(
            mbid,
            infohash,
            "content_hash_hex",
            "FLAC",
            kp,
            "2026-02-10T14:30:00Z",
        )
    }

    async fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        (db, dir)
    }

    // -- Cache tests --

    #[tokio::test]
    async fn cache_store_and_retrieve_by_mbid() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-1", "infohash-1");

        cache.store_attestation(&att).await.unwrap();
        let results = cache.get_attestations_for_mbid("mbid-1").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].mbid, "mbid-1");
        assert_eq!(results[0].infohash, "infohash-1");
        assert_eq!(results[0].signature, att.signature);
    }

    #[tokio::test]
    async fn cache_store_and_retrieve_by_infohash() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-2", "infohash-2");

        cache.store_attestation(&att).await.unwrap();
        let results = cache
            .get_attestations_for_infohash("infohash-2")
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].infohash, "infohash-2");
    }

    #[tokio::test]
    async fn cache_rejects_invalid_signature() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);
        let kp = gen_keypair();
        let mut att = make_attestation(&kp, "mbid-3", "infohash-3");
        att.format = "TAMPERED".to_string(); // invalidate signature

        let result = cache.store_attestation(&att).await;
        assert!(result.is_err());

        // Nothing stored
        let results = cache.get_attestations_for_mbid("mbid-3").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn merge_stores_valid_rejects_invalid() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);
        let kp = gen_keypair();

        let valid = make_attestation(&kp, "mbid-m1", "infohash-m1");
        let mut invalid = make_attestation(&kp, "mbid-m2", "infohash-m2");
        invalid.format = "TAMPERED".to_string();

        let result = cache.merge_remote_attestations(&[valid, invalid]).await;
        assert_eq!(result.stored, 1);
        assert_eq!(result.rejected, 1);

        let stored = cache.get_attestations_for_mbid("mbid-m1").await.unwrap();
        assert_eq!(stored.len(), 1);
        let not_stored = cache.get_attestations_for_mbid("mbid-m2").await.unwrap();
        assert!(not_stored.is_empty());
    }

    #[tokio::test]
    async fn confidence_counts_distinct_signers() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        // Three different signers attest to the same mbid+infohash
        let att1 = make_attestation(&kp1, "mbid-c", "infohash-c");
        let att2 = make_attestation(&kp2, "mbid-c", "infohash-c");
        let att3 = make_attestation(&kp3, "mbid-c", "infohash-c");

        cache.store_attestation(&att1).await.unwrap();
        cache.store_attestation(&att2).await.unwrap();
        cache.store_attestation(&att3).await.unwrap();

        let confidence = cache.get_confidence("mbid-c", "infohash-c").await.unwrap();
        assert_eq!(confidence, 3);

        // Different infohash has zero confidence
        let confidence = cache
            .get_confidence("mbid-c", "other-infohash")
            .await
            .unwrap();
        assert_eq!(confidence, 0);
    }

    #[tokio::test]
    async fn store_is_idempotent() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-idem", "infohash-idem");

        cache.store_attestation(&att).await.unwrap();
        cache.store_attestation(&att).await.unwrap(); // no error

        let results = cache.get_attestations_for_mbid("mbid-idem").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    // -- Wire format tests --

    #[test]
    fn wire_format_roundtrip() {
        let kp = gen_keypair();
        let att1 = make_attestation(&kp, "mbid-w1", "infohash-w1");
        let att2 = make_attestation(&kp, "mbid-w2", "infohash-w2");

        let bytes = serialize_attestations(&[att1.clone(), att2.clone()]);
        let deserialized = deserialize_attestations(&bytes).unwrap();

        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].mbid, "mbid-w1");
        assert_eq!(deserialized[0].signature, att1.signature);
        assert_eq!(deserialized[1].mbid, "mbid-w2");
        assert_eq!(deserialized[1].signature, att2.signature);
    }

    #[test]
    fn wire_format_rejects_tampered_data() {
        let kp = gen_keypair();
        let mut att = make_attestation(&kp, "mbid-t", "infohash-t");
        // Serialize, then tamper
        let bytes = serialize_attestations(&[att.clone()]);
        let text = String::from_utf8(bytes).unwrap();

        // Manually replace format in the JSON
        att.format = "TAMPERED".to_string();
        let tampered = serde_json::to_string(&att).unwrap();
        let tampered_text = text.replacen(&text, &tampered, 1);

        let result = deserialize_attestations(tampered_text.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn wire_format_empty() {
        let bytes = serialize_attestations(&[]);
        assert!(bytes.is_empty());
        let deserialized = deserialize_attestations(&bytes).unwrap();
        assert!(deserialized.is_empty());
    }

    #[test]
    fn wire_format_handles_blank_lines() {
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-bl", "infohash-bl");
        let line = serde_json::to_string(&att).unwrap();
        let with_blanks = format!("\n{line}\n\n");

        let deserialized = deserialize_attestations(with_blanks.as_bytes()).unwrap();
        assert_eq!(deserialized.len(), 1);
        assert_eq!(deserialized[0].mbid, "mbid-bl");
    }
}
