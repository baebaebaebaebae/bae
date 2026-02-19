/// Forward lookup: "I want this release."
///
/// Orchestrates the flow: MBID -> DHT peer discovery -> attestation collection
/// -> confidence aggregation -> best infohash selection.
///
/// The actual peer-to-peer attestation exchange (BEP 10 extension messages) is
/// not yet implemented. This module provides the orchestration layer that ties
/// together DhtService and AttestationCache, and accepts attestations from
/// whatever transport delivers them.
use std::collections::HashMap;

use crate::sync::attestation::Attestation;
use crate::sync::attestation_cache::{AttestationCache, MergeResult};
use crate::torrent::dht::{compute_rendezvous_key, DhtService, ALERT_DHT_GET_PEERS_REPLY};
use crate::torrent::ffi::AlertData;

/// A peer endpoint discovered via DHT.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerEndpoint {
    pub address: String,
    pub port: u16,
}

/// Attestations for a single infohash, with aggregated confidence.
#[derive(Debug, Clone)]
pub struct InfohashCandidate {
    pub infohash: String,
    /// Number of distinct signers attesting to this mbid+infohash mapping.
    pub confidence: usize,
    /// All attestations for this infohash (may include multiple formats/signers).
    pub attestations: Vec<Attestation>,
}

/// Result of a forward lookup: discovered peers and attestation candidates.
#[derive(Debug, Clone)]
pub struct LookupResult {
    pub mbid: String,
    /// Peers discovered via DHT, available for attestation exchange.
    pub peers: Vec<PeerEndpoint>,
    /// Attestations grouped by infohash, sorted by confidence (highest first).
    pub candidates: Vec<InfohashCandidate>,
}

/// Extract peer endpoints from DHT alerts that match a given MBID.
///
/// Filters alerts for `dht_get_peers_reply` with a rendezvous key matching
/// the MBID, then parses the peer list. Non-matching alerts are ignored.
pub fn collect_dht_peers(mbid: &str, alerts: &[AlertData]) -> Vec<PeerEndpoint> {
    let expected_hash = rendezvous_hex(mbid);
    let mut seen = std::collections::HashSet::new();
    let mut peers = Vec::new();

    for alert in alerts {
        if alert.alert_type != ALERT_DHT_GET_PEERS_REPLY {
            continue;
        }

        if alert.info_hash != expected_hash {
            continue;
        }

        for peer_str in &alert.peers {
            if let Some(ep) = parse_peer_endpoint(peer_str) {
                if seen.insert(ep.clone()) {
                    peers.push(ep);
                }
            }
        }
    }

    peers
}

/// Group attestations by infohash, counting distinct signers per group.
/// Returns candidates sorted by confidence descending (highest first).
pub fn group_by_infohash(attestations: Vec<Attestation>) -> Vec<InfohashCandidate> {
    let mut by_infohash: HashMap<String, Vec<Attestation>> = HashMap::new();
    for att in attestations {
        by_infohash
            .entry(att.infohash.clone())
            .or_default()
            .push(att);
    }

    let mut candidates: Vec<InfohashCandidate> = by_infohash
        .into_iter()
        .map(|(infohash, atts)| {
            let mut signers = atts
                .iter()
                .map(|a| a.author_pubkey.as_str())
                .collect::<Vec<_>>();
            signers.sort_unstable();
            signers.dedup();
            let confidence = signers.len();

            InfohashCandidate {
                infohash,
                confidence,
                attestations: atts,
            }
        })
        .collect();

    // Sort by confidence descending, then by infohash for determinism
    candidates.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then(a.infohash.cmp(&b.infohash))
    });
    candidates
}

/// Orchestrates forward lookup: MBID -> peers -> attestations -> best infohash.
pub struct ForwardLookupService<'a> {
    dht: &'a DhtService,
    cache: &'a AttestationCache<'a>,
}

impl<'a> ForwardLookupService<'a> {
    pub fn new(dht: &'a DhtService, cache: &'a AttestationCache<'a>) -> Self {
        Self { dht, cache }
    }

    /// Initiate a DHT lookup for peers who have a release with the given MBID.
    ///
    /// This is fire-and-forget: results arrive asynchronously via DHT alerts.
    /// Call `collect_dht_peers` on subsequent alerts to gather discovered peers.
    pub async fn start_lookup(
        &self,
        mbid: &str,
    ) -> Result<(), crate::torrent::client::TorrentError> {
        self.dht.get_peers(mbid).await
    }

    /// Merge attestations received from a peer into the local cache.
    ///
    /// Invalid signatures are rejected individually (not a batch failure).
    pub async fn ingest_attestations(&self, attestations: &[Attestation]) -> MergeResult {
        self.cache.merge_remote_attestations(attestations).await
    }

    /// Build the aggregated lookup result for an MBID.
    ///
    /// Queries the local attestation cache, groups by infohash, computes
    /// confidence (distinct signers per infohash), and sorts candidates
    /// by confidence descending.
    pub async fn aggregate(
        &self,
        mbid: &str,
        peers: Vec<PeerEndpoint>,
    ) -> Result<LookupResult, crate::sync::attestation::AttestationError> {
        let attestations = self.cache.get_attestations_for_mbid(mbid).await?;
        let candidates = group_by_infohash(attestations);

        Ok(LookupResult {
            mbid: mbid.to_string(),
            peers,
            candidates,
        })
    }

    /// Return the infohash with the highest confidence for a given MBID,
    /// or None if no attestations exist.
    pub async fn best_infohash(
        &self,
        mbid: &str,
    ) -> Result<Option<String>, crate::sync::attestation::AttestationError> {
        let attestations = self.cache.get_attestations_for_mbid(mbid).await?;
        let candidates = group_by_infohash(attestations);
        Ok(candidates.first().map(|c| c.infohash.clone()))
    }
}

/// Compute the uppercase hex rendezvous key for an MBID.
fn rendezvous_hex(mbid: &str) -> String {
    let key = compute_rendezvous_key(mbid);
    key.iter().map(|b| format!("{:02X}", b)).collect()
}

/// Parse an "ip:port" string into a PeerEndpoint.
fn parse_peer_endpoint(s: &str) -> Option<PeerEndpoint> {
    let (addr, port_str) = s.rsplit_once(':')?;
    let port = port_str.parse::<u16>().ok()?;
    Some(PeerEndpoint {
        address: addr.to_string(),
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::keys::UserKeypair;
    use crate::sync::attestation::create_attestation;
    use tempfile::TempDir;

    fn gen_keypair() -> UserKeypair {
        UserKeypair::generate()
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

    fn make_dht_alert(info_hash: &str, peers: Vec<String>) -> AlertData {
        AlertData {
            alert_type: ALERT_DHT_GET_PEERS_REPLY,
            info_hash: info_hash.to_string(),
            tracker_url: String::new(),
            tracker_message: String::new(),
            num_peers: peers.len() as i32,
            num_seeds: 0,
            file_path: String::new(),
            progress: 0.0,
            error_message: String::new(),
            peers,
        }
    }

    // -- parse_peer_endpoint --

    #[test]
    fn parse_valid_ipv4_endpoint() {
        let ep = parse_peer_endpoint("192.168.1.1:6881").unwrap();
        assert_eq!(ep.address, "192.168.1.1");
        assert_eq!(ep.port, 6881);
    }

    #[test]
    fn parse_valid_ipv6_endpoint() {
        let ep = parse_peer_endpoint("[::1]:6881").unwrap();
        assert_eq!(ep.address, "[::1]");
        assert_eq!(ep.port, 6881);
    }

    #[test]
    fn parse_invalid_endpoint_no_port() {
        assert!(parse_peer_endpoint("192.168.1.1").is_none());
    }

    #[test]
    fn parse_invalid_endpoint_bad_port() {
        assert!(parse_peer_endpoint("192.168.1.1:abc").is_none());
    }

    // -- rendezvous_hex --

    #[test]
    fn rendezvous_hex_matches_dht_to_hex() {
        let mbid = "12345678-1234-1234-1234-123456789012";
        let key = compute_rendezvous_key(mbid);
        let expected: String = key.iter().map(|b| format!("{:02X}", b)).collect();
        assert_eq!(rendezvous_hex(mbid), expected);
    }

    // -- collect_dht_peers --

    #[test]
    fn collect_peers_filters_by_info_hash() {
        let mbid = "test-mbid";
        let expected_hash = rendezvous_hex(mbid);

        let matching_alert = make_dht_alert(&expected_hash, vec!["1.2.3.4:6881".to_string()]);
        let other_alert = make_dht_alert(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            vec!["5.6.7.8:6882".to_string()],
        );

        let peers = collect_dht_peers(mbid, &[matching_alert, other_alert]);

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].address, "1.2.3.4");
        assert_eq!(peers[0].port, 6881);
    }

    #[test]
    fn collect_peers_ignores_non_dht_alerts() {
        let mbid = "test-mbid";
        let expected_hash = rendezvous_hex(mbid);

        let mut alert = make_dht_alert(&expected_hash, vec!["1.2.3.4:6881".to_string()]);
        alert.alert_type = 0; // not a DHT reply

        let peers = collect_dht_peers(mbid, &[alert]);
        assert!(peers.is_empty());
    }

    #[test]
    fn collect_peers_handles_multiple_peers_in_one_alert() {
        let mbid = "multi-peer";
        let hash = rendezvous_hex(mbid);
        let alert = make_dht_alert(
            &hash,
            vec![
                "10.0.0.1:6881".to_string(),
                "10.0.0.2:6882".to_string(),
                "10.0.0.3:6883".to_string(),
            ],
        );

        let peers = collect_dht_peers(mbid, &[alert]);
        assert_eq!(peers.len(), 3);
    }

    #[test]
    fn collect_peers_empty_alerts() {
        let peers = collect_dht_peers("any-mbid", &[]);
        assert!(peers.is_empty());
    }

    // -- group_by_infohash --

    #[test]
    fn group_single_attestation() {
        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-1", "infohash-1");
        let candidates = group_by_infohash(vec![att]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].infohash, "infohash-1");
        assert_eq!(candidates[0].confidence, 1);
        assert_eq!(candidates[0].attestations.len(), 1);
    }

    #[test]
    fn group_multiple_signers_same_infohash() {
        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        let candidates = group_by_infohash(vec![
            make_attestation(&kp1, "mbid-1", "infohash-A"),
            make_attestation(&kp2, "mbid-1", "infohash-A"),
            make_attestation(&kp3, "mbid-1", "infohash-A"),
        ]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, 3);
    }

    #[test]
    fn group_sorts_by_confidence_descending() {
        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        // infohash-B has 2 signers, infohash-A has 1
        let candidates = group_by_infohash(vec![
            make_attestation(&kp1, "mbid-1", "infohash-A"),
            make_attestation(&kp2, "mbid-1", "infohash-B"),
            make_attestation(&kp3, "mbid-1", "infohash-B"),
        ]);

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].infohash, "infohash-B");
        assert_eq!(candidates[0].confidence, 2);
        assert_eq!(candidates[1].infohash, "infohash-A");
        assert_eq!(candidates[1].confidence, 1);
    }

    #[test]
    fn group_same_signer_counted_once() {
        let kp = gen_keypair();

        // Same signer attests twice to the same infohash
        let candidates = group_by_infohash(vec![
            make_attestation(&kp, "mbid-1", "infohash-A"),
            make_attestation(&kp, "mbid-1", "infohash-A"),
        ]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, 1); // deduplicated
        assert_eq!(candidates[0].attestations.len(), 2); // both kept
    }

    #[test]
    fn group_empty_input() {
        let candidates = group_by_infohash(vec![]);
        assert!(candidates.is_empty());
    }

    // -- Integration tests with real DB --

    #[tokio::test]
    async fn aggregate_groups_cached_attestations() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp1 = gen_keypair();
        let kp2 = gen_keypair();

        cache
            .store_attestation(&make_attestation(&kp1, "mbid-agg", "infohash-X"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp2, "mbid-agg", "infohash-X"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp1, "mbid-agg", "infohash-Y"))
            .await
            .unwrap();

        let all = cache.get_attestations_for_mbid("mbid-agg").await.unwrap();
        let candidates = group_by_infohash(all);

        assert_eq!(candidates.len(), 2);
        // infohash-X has 2 signers, infohash-Y has 1
        assert_eq!(candidates[0].infohash, "infohash-X");
        assert_eq!(candidates[0].confidence, 2);
        assert_eq!(candidates[1].infohash, "infohash-Y");
        assert_eq!(candidates[1].confidence, 1);
    }

    #[tokio::test]
    async fn best_infohash_picks_highest_confidence() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp1 = gen_keypair();
        let kp2 = gen_keypair();
        let kp3 = gen_keypair();

        // infohash-W: 1 signer, infohash-Z: 2 signers
        cache
            .store_attestation(&make_attestation(&kp1, "mbid-best", "infohash-W"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp2, "mbid-best", "infohash-Z"))
            .await
            .unwrap();
        cache
            .store_attestation(&make_attestation(&kp3, "mbid-best", "infohash-Z"))
            .await
            .unwrap();

        let all = cache.get_attestations_for_mbid("mbid-best").await.unwrap();
        let candidates = group_by_infohash(all);
        let best = candidates.first().map(|c| c.infohash.clone());

        assert_eq!(best, Some("infohash-Z".to_string()));
    }

    #[tokio::test]
    async fn best_infohash_none_when_no_attestations() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let all = cache
            .get_attestations_for_mbid("nonexistent")
            .await
            .unwrap();
        let candidates = group_by_infohash(all);
        assert!(candidates.first().is_none());
    }

    #[tokio::test]
    async fn ingest_attestations_merges_into_cache() {
        let (db, _dir) = test_db().await;
        let cache = AttestationCache::new(&db);

        let kp = gen_keypair();
        let att = make_attestation(&kp, "mbid-ingest", "infohash-ing");

        let result = cache.merge_remote_attestations(&[att]).await;
        assert_eq!(result.stored, 1);
        assert_eq!(result.rejected, 0);

        let stored = cache
            .get_attestations_for_mbid("mbid-ingest")
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].infohash, "infohash-ing");
    }
}
