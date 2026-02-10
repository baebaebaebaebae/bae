/// Reverse lookup: "What are these files?"
///
/// Given a BitTorrent infohash (the user has files), discover which MusicBrainz
/// release ID (MBID) they correspond to by querying the local attestation cache.
///
/// This is the structural inverse of forward lookup:
/// - Forward: MBID -> attestations -> group by infohash -> best infohash
/// - Reverse: infohash -> attestations -> group by MBID -> best MBID
use std::collections::HashMap;

use crate::sync::attestation::Attestation;
use crate::sync::attestation_cache::AttestationCache;

/// Attestations for a single MBID, with aggregated confidence.
#[derive(Debug, Clone)]
pub struct MbidCandidate {
    pub mbid: String,
    /// Number of distinct signers attesting to this infohash+mbid mapping.
    pub confidence: usize,
    /// All attestations for this MBID (may include multiple formats/signers).
    pub attestations: Vec<Attestation>,
}

/// Result of a reverse lookup: MBID candidates for a given infohash.
#[derive(Debug, Clone)]
pub struct ReverseLookupResult {
    pub infohash: String,
    /// MBID candidates sorted by confidence (highest first).
    pub candidates: Vec<MbidCandidate>,
}

/// Group attestations by MBID, counting distinct signers per group.
/// Returns candidates sorted by confidence descending (highest first).
pub fn group_by_mbid(attestations: Vec<Attestation>) -> Vec<MbidCandidate> {
    let mut by_mbid: HashMap<String, Vec<Attestation>> = HashMap::new();
    for att in attestations {
        by_mbid.entry(att.mbid.clone()).or_default().push(att);
    }

    let mut candidates: Vec<MbidCandidate> = by_mbid
        .into_iter()
        .map(|(mbid, atts)| {
            let mut signers = atts
                .iter()
                .map(|a| a.author_pubkey.as_str())
                .collect::<Vec<_>>();
            signers.sort_unstable();
            signers.dedup();
            let confidence = signers.len();

            MbidCandidate {
                mbid,
                confidence,
                attestations: atts,
            }
        })
        .collect();

    // Sort by confidence descending, then by MBID for determinism
    candidates.sort_by(|a, b| b.confidence.cmp(&a.confidence).then(a.mbid.cmp(&b.mbid)));
    candidates
}

/// Orchestrates reverse lookup: infohash -> attestations -> best MBID.
pub struct ReverseLookupService<'a> {
    cache: &'a AttestationCache<'a>,
}

impl<'a> ReverseLookupService<'a> {
    pub fn new(cache: &'a AttestationCache<'a>) -> Self {
        Self { cache }
    }

    /// Build the aggregated lookup result for an infohash.
    ///
    /// Queries the local attestation cache, groups by MBID, computes
    /// confidence (distinct signers per MBID), and sorts candidates
    /// by confidence descending.
    pub async fn lookup(
        &self,
        infohash: &str,
    ) -> Result<ReverseLookupResult, crate::sync::attestation::AttestationError> {
        let attestations = self.cache.get_attestations_for_infohash(infohash).await?;
        let candidates = group_by_mbid(attestations);

        Ok(ReverseLookupResult {
            infohash: infohash.to_string(),
            candidates,
        })
    }

    /// Return the MBID with the highest confidence for a given infohash,
    /// or None if no attestations exist.
    pub async fn best_mbid(
        &self,
        infohash: &str,
    ) -> Result<Option<String>, crate::sync::attestation::AttestationError> {
        let attestations = self.cache.get_attestations_for_infohash(infohash).await?;
        let candidates = group_by_mbid(attestations);
        Ok(candidates.first().map(|c| c.mbid.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
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

    // -- group_by_mbid --

    #[test]
    fn group_single_attestation() {
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-1", "infohash-1");
        let candidates = group_by_mbid(vec![att]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mbid, "mbid-1");
        assert_eq!(candidates[0].confidence, 1);
        assert_eq!(candidates[0].attestations.len(), 1);
    }

    #[test]
    fn group_multiple_signers_same_mbid() {
        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        let candidates = group_by_mbid(vec![
            make_attestation(&kp1, "mbid-A", "infohash-1"),
            make_attestation(&kp2, "mbid-A", "infohash-1"),
            make_attestation(&kp3, "mbid-A", "infohash-1"),
        ]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, 3);
    }

    #[test]
    fn group_sorts_by_confidence_descending() {
        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        // mbid-B has 2 signers, mbid-A has 1
        let candidates = group_by_mbid(vec![
            make_attestation(&kp1, "mbid-A", "infohash-1"),
            make_attestation(&kp2, "mbid-B", "infohash-1"),
            make_attestation(&kp3, "mbid-B", "infohash-1"),
        ]);

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].mbid, "mbid-B");
        assert_eq!(candidates[0].confidence, 2);
        assert_eq!(candidates[1].mbid, "mbid-A");
        assert_eq!(candidates[1].confidence, 1);
    }

    #[test]
    fn group_same_signer_counted_once() {
        let kp = gen_keypair();

        // Same signer attests twice to the same MBID
        let candidates = group_by_mbid(vec![
            make_attestation(&kp, "mbid-A", "infohash-1"),
            make_attestation(&kp, "mbid-A", "infohash-1"),
        ]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, 1); // deduplicated
        assert_eq!(candidates[0].attestations.len(), 2); // both kept
    }

    #[test]
    fn group_empty_input() {
        let candidates = group_by_mbid(vec![]);
        assert!(candidates.is_empty());
    }

    // -- Integration tests with real DB --

    #[tokio::test]
    async fn lookup_groups_cached_attestations() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp1 = gen_keypair();
        let kp2 = gen_keypair();

        cache
            .store_attestation(&make_attestation(&kp1, "mbid-X", "infohash-rev"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp2, "mbid-X", "infohash-rev"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp1, "mbid-Y", "infohash-rev"))
            .await
            .unwrap();

        let service = ReverseLookupService::new(&cache);
        let result = service.lookup("infohash-rev").await.unwrap();

        assert_eq!(result.infohash, "infohash-rev");
        assert_eq!(result.candidates.len(), 2);
        // mbid-X has 2 signers, mbid-Y has 1
        assert_eq!(result.candidates[0].mbid, "mbid-X");
        assert_eq!(result.candidates[0].confidence, 2);
        assert_eq!(result.candidates[1].mbid, "mbid-Y");
        assert_eq!(result.candidates[1].confidence, 1);
    }

    #[tokio::test]
    async fn best_mbid_picks_highest_confidence() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        // mbid-W: 1 signer, mbid-Z: 2 signers
        cache
            .store_attestation(&make_attestation(&kp1, "mbid-W", "infohash-best"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp2, "mbid-Z", "infohash-best"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp3, "mbid-Z", "infohash-best"))
            .await
            .unwrap();

        let service = ReverseLookupService::new(&cache);
        let best = service.best_mbid("infohash-best").await.unwrap();

        assert_eq!(best, Some("mbid-Z".to_string()));
    }

    #[tokio::test]
    async fn best_mbid_none_when_no_attestations() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let service = ReverseLookupService::new(&cache);
        let best = service.best_mbid("nonexistent").await.unwrap();

        assert!(best.is_none());
    }

    #[tokio::test]
    async fn lookup_empty_for_unknown_infohash() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let service = ReverseLookupService::new(&cache);
        let result = service.lookup("unknown").await.unwrap();

        assert_eq!(result.infohash, "unknown");
        assert!(result.candidates.is_empty());
    }
}
