use std::collections::HashMap;

use super::membership::MembershipChain;

/// Maps Ed25519 public key hex strings to display names.
///
/// This is a local-only mapping -- not synced. Each device maintains
/// its own name assignments (set during invitation or manually).
#[derive(Debug, Clone)]
pub struct AttributionMap {
    names: HashMap<String, String>,
}

/// Format a pubkey hex string as a short label: first 4 chars + "..." + last 4 chars.
fn truncated_pubkey(pubkey_hex: &str) -> String {
    if pubkey_hex.len() <= 12 {
        return pubkey_hex.to_string();
    }
    format!(
        "{}...{}",
        &pubkey_hex[..4],
        &pubkey_hex[pubkey_hex.len() - 4..]
    )
}

impl Default for AttributionMap {
    fn default() -> Self {
        Self::new()
    }
}

impl AttributionMap {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }

    /// Set the display name for a public key.
    pub fn set_name(&mut self, pubkey_hex: &str, name: &str) {
        self.names.insert(pubkey_hex.to_string(), name.to_string());
    }

    /// Get the display name for a public key, if one has been set.
    pub fn get_name(&self, pubkey_hex: &str) -> Option<&str> {
        self.names.get(pubkey_hex).map(|s| s.as_str())
    }

    /// Returns the display name if set, otherwise a truncated pubkey like "abc1...ef23".
    pub fn display_name(&self, pubkey_hex: &str) -> String {
        match self.names.get(pubkey_hex) {
            Some(name) => name.clone(),
            None => truncated_pubkey(pubkey_hex),
        }
    }

    /// Creates a map from a membership chain, using truncated pubkeys as default
    /// names for all current members.
    pub fn from_membership_chain(chain: &MembershipChain) -> Self {
        let mut map = Self::new();
        for (pubkey, _role) in chain.current_members() {
            map.names.insert(pubkey.clone(), truncated_pubkey(&pubkey));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::UserKeypair;
    use crate::sync::membership::{
        sign_membership_entry, MemberRole, MembershipAction, MembershipChain, MembershipEntry,
    };

    fn gen_keypair() -> UserKeypair {
        UserKeypair::generate()
    }

    fn pubkey_hex(kp: &UserKeypair) -> String {
        hex::encode(kp.public_key)
    }

    fn founder_entry(kp: &UserKeypair, timestamp: &str) -> MembershipEntry {
        let pk_hex = pubkey_hex(kp);
        let mut entry = MembershipEntry {
            action: MembershipAction::Add,
            user_pubkey: pk_hex.clone(),
            role: MemberRole::Owner,
            timestamp: timestamp.to_string(),
            author_pubkey: pk_hex,
            signature: String::new(),
        };
        sign_membership_entry(&mut entry, kp);
        entry
    }

    fn make_entry(
        author: &UserKeypair,
        action: MembershipAction,
        subject: &UserKeypair,
        role: MemberRole,
        timestamp: &str,
    ) -> MembershipEntry {
        let mut entry = MembershipEntry {
            action,
            user_pubkey: pubkey_hex(subject),
            role,
            timestamp: timestamp.to_string(),
            author_pubkey: pubkey_hex(author),
            signature: String::new(),
        };
        sign_membership_entry(&mut entry, author);
        entry
    }

    #[test]
    fn display_name_falls_back_to_truncated_pubkey() {
        let map = AttributionMap::new();
        // Ed25519 pubkeys are 32 bytes = 64 hex chars
        let pubkey = "aabbccdd11223344556677889900aabbccddeeff00112233445566778899001122";
        let display = map.display_name(pubkey);
        assert_eq!(display, "aabb...1122");
    }

    #[test]
    fn set_and_get_name() {
        let mut map = AttributionMap::new();
        let pubkey = "aabbccdd11223344556677889900aabbccddeeff00112233445566778899001122";

        assert!(map.get_name(pubkey).is_none());

        map.set_name(pubkey, "Alice");
        assert_eq!(map.get_name(pubkey), Some("Alice"));
        assert_eq!(map.display_name(pubkey), "Alice");
    }

    #[test]
    fn from_membership_chain_creates_entries() {
        let owner = gen_keypair();
        let member = gen_keypair();

        let mut chain = MembershipChain::new();
        chain
            .add_entry(founder_entry(&owner, "0000000001000-0000-dev1"))
            .unwrap();
        chain
            .add_entry(make_entry(
                &owner,
                MembershipAction::Add,
                &member,
                MemberRole::Member,
                "0000000002000-0000-dev1",
            ))
            .unwrap();

        let map = AttributionMap::from_membership_chain(&chain);

        // Both members should have entries (truncated pubkeys as names).
        let owner_pk = pubkey_hex(&owner);
        let member_pk = pubkey_hex(&member);

        assert!(map.get_name(&owner_pk).is_some());
        assert!(map.get_name(&member_pk).is_some());

        // Names should be truncated pubkeys.
        assert_eq!(
            map.get_name(&owner_pk).unwrap(),
            truncated_pubkey(&owner_pk)
        );
        assert_eq!(
            map.get_name(&member_pk).unwrap(),
            truncated_pubkey(&member_pk)
        );
    }
}
