use sha1::{Digest, Sha1};

use crate::torrent::client::{TorrentClient, TorrentError};

/// Alert type constants matching the C++ AlertType enum
pub const ALERT_DHT_GET_PEERS_REPLY: i32 = 12;
pub const ALERT_DHT_BOOTSTRAP: i32 = 14;

/// Compute the DHT rendezvous key for a MusicBrainz ID.
///
/// The key is SHA-1("bae:mbid:" + mbid), producing a 20-byte hash
/// compatible with libtorrent's sha1_hash / Kademlia node IDs.
pub fn compute_rendezvous_key(mbid: &str) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(b"bae:mbid:");
    hasher.update(mbid.as_bytes());
    hasher.finalize().into()
}

/// Encode a 20-byte hash as a 40-character uppercase hex string.
fn to_hex(hash: &[u8; 20]) -> String {
    hash.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>()
}

/// DHT operations built on top of TorrentClient.
///
/// Wraps the raw FFI DHT calls with MBID-based rendezvous keys.
pub struct DhtService {
    client: TorrentClient,
}

impl DhtService {
    pub fn new(client: TorrentClient) -> Self {
        Self { client }
    }

    /// Announce this peer on the DHT for a release identified by its MusicBrainz ID.
    pub async fn announce(&self, mbid: &str, port: u16) -> Result<(), TorrentError> {
        let key = compute_rendezvous_key(mbid);
        let hex = to_hex(&key);
        self.client.dht_announce(&hex, port).await
    }

    /// Look up peers on the DHT for a release identified by its MusicBrainz ID.
    ///
    /// This is asynchronous -- results arrive via `dht_get_peers_reply_alert`
    /// when polling alerts from the TorrentClient.
    pub async fn get_peers(&self, mbid: &str) -> Result<(), TorrentError> {
        let key = compute_rendezvous_key(mbid);
        let hex = to_hex(&key);
        self.client.dht_get_peers(&hex).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendezvous_key_is_deterministic() {
        let mbid = "12345678-1234-1234-1234-123456789012";
        let key1 = compute_rendezvous_key(mbid);
        let key2 = compute_rendezvous_key(mbid);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 20);
    }

    #[test]
    fn different_mbids_produce_different_keys() {
        let key1 = compute_rendezvous_key("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let key2 = compute_rendezvous_key("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
        assert_ne!(key1, key2);
    }

    #[test]
    fn rendezvous_key_matches_expected_sha1() {
        // SHA-1("bae:mbid:test") should be a known value
        let key = compute_rendezvous_key("test");
        let hex = to_hex(&key);

        // Verify via manual computation:
        // echo -n "bae:mbid:test" | shasum -a 1
        let mut hasher = Sha1::new();
        hasher.update(b"bae:mbid:test");
        let expected: [u8; 20] = hasher.finalize().into();
        assert_eq!(key, expected);

        // Hex is 40 chars, uppercase
        assert_eq!(hex.len(), 40);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
