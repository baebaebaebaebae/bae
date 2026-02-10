/// Invitation flow: orchestrates adding new members to a shared library.
///
/// `create_invitation()` is called by the library owner to invite a new member.
/// `accept_invitation()` is called by the invitee to unwrap the library key.
use crate::keys::{self, KeyError, UserKeypair};
use crate::sodium_ffi;

use super::bucket::{BucketError, SyncBucketClient};
use super::membership::{
    sign_membership_entry, MemberRole, MembershipAction, MembershipChain, MembershipEntry,
    MembershipError,
};

#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    #[error("Bucket error: {0}")]
    Bucket(#[from] BucketError),
    #[error("Key error: {0}")]
    Key(#[from] KeyError),
    #[error("Membership error: {0}")]
    Membership(#[from] MembershipError),
    #[error("Crypto error: {0}")]
    Crypto(String),
}

/// Create an invitation for a new member.
///
/// This wraps the library encryption key to the invitee's X25519 public key,
/// creates and signs a membership entry (Add), uploads both to the bucket,
/// and appends the entry to the local chain.
pub async fn create_invitation(
    bucket: &dyn SyncBucketClient,
    chain: &mut MembershipChain,
    owner_keypair: &UserKeypair,
    invitee_ed25519_pubkey: &str,
    role: MemberRole,
    encryption_key: &[u8; 32],
    timestamp: &str,
) -> Result<(), InviteError> {
    // Decode the invitee's Ed25519 public key from hex.
    let invitee_pk_bytes: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] =
        hex::decode(invitee_ed25519_pubkey)
            .map_err(|e| InviteError::Crypto(format!("invalid invitee pubkey hex: {e}")))?
            .try_into()
            .map_err(|_| InviteError::Crypto("invitee pubkey wrong length".to_string()))?;

    // Convert Ed25519 -> X25519 for sealed box encryption.
    let invitee_x25519_pk = keys::ed25519_to_x25519_public_key(&invitee_pk_bytes);

    // Wrap the library encryption key.
    let wrapped_key = keys::seal_box_encrypt(encryption_key, &invitee_x25519_pk);

    // Upload wrapped key.
    bucket
        .put_wrapped_key(invitee_ed25519_pubkey, wrapped_key)
        .await?;

    // Create and sign a membership entry.
    let mut entry = MembershipEntry {
        action: MembershipAction::Add,
        user_pubkey: invitee_ed25519_pubkey.to_string(),
        role,
        timestamp: timestamp.to_string(),
        author_pubkey: String::new(),
        signature: String::new(),
    };
    sign_membership_entry(&mut entry, owner_keypair);

    // Determine the next seq for this author in the bucket.
    let author_pubkey_hex = hex::encode(owner_keypair.public_key);
    let existing_entries = bucket.list_membership_entries().await?;
    let next_seq = existing_entries
        .iter()
        .filter(|(author, _)| author == &author_pubkey_hex)
        .map(|(_, seq)| seq)
        .max()
        .map_or(1, |max| max + 1);

    // Upload membership entry.
    let entry_bytes =
        serde_json::to_vec(&entry).map_err(|e| InviteError::Crypto(format!("serialize: {e}")))?;
    bucket
        .put_membership_entry(&author_pubkey_hex, next_seq, entry_bytes)
        .await?;

    // Add to the local chain (validates signature and author ownership).
    chain.add_entry(entry)?;

    Ok(())
}

/// Accept an invitation by downloading and unwrapping the library encryption key.
///
/// The invitee calls this after receiving an invitation. It downloads the
/// wrapped key from the bucket and decrypts it with the invitee's X25519 keys.
pub async fn accept_invitation(
    bucket: &dyn SyncBucketClient,
    keypair: &UserKeypair,
) -> Result<[u8; 32], InviteError> {
    let pubkey_hex = hex::encode(keypair.public_key);

    // Download wrapped key.
    let wrapped_key = bucket.get_wrapped_key(&pubkey_hex).await?;

    // Decrypt with our X25519 keys.
    let x25519_pk = keypair.to_x25519_public_key();
    let x25519_sk = keypair.to_x25519_secret_key();

    let plaintext = keys::seal_box_decrypt(&wrapped_key, &x25519_pk, &x25519_sk)?;

    let encryption_key: [u8; 32] = plaintext
        .try_into()
        .map_err(|_| InviteError::Crypto("unwrapped key is not 32 bytes".to_string()))?;

    Ok(encryption_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::membership::MemberRole;
    use crate::sync::test_helpers::MockBucket;

    /// Generate a keypair directly (bypasses KeyService env-var issues).
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

    fn pubkey_hex(kp: &UserKeypair) -> String {
        hex::encode(kp.public_key)
    }

    /// Bootstrap a chain with a founder entry.
    fn bootstrap_chain(owner: &UserKeypair) -> MembershipChain {
        let pk_hex = pubkey_hex(owner);
        let mut entry = MembershipEntry {
            action: MembershipAction::Add,
            user_pubkey: pk_hex.clone(),
            role: MemberRole::Owner,
            timestamp: "0000000001000-0000-dev1".to_string(),
            author_pubkey: pk_hex,
            signature: String::new(),
        };
        sign_membership_entry(&mut entry, owner);

        let mut chain = MembershipChain::new();
        chain.add_entry(entry).unwrap();
        chain
    }

    #[tokio::test]
    async fn create_and_accept_invitation() {
        let owner = gen_keypair();
        let invitee = gen_keypair();
        let encryption_key: [u8; 32] = [42u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Owner invites the new member.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&invitee),
            MemberRole::Member,
            &encryption_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Chain should now have 2 entries.
        assert_eq!(chain.entries().len(), 2);
        chain.validate().unwrap();

        // Invitee should be a current member.
        let members = chain.current_members();
        assert!(members
            .iter()
            .any(|(pk, r)| pk == &pubkey_hex(&invitee) && *r == MemberRole::Member));

        // Invitee accepts the invitation.
        let unwrapped = accept_invitation(&bucket, &invitee).await.unwrap();
        assert_eq!(unwrapped, encryption_key);
    }

    #[tokio::test]
    async fn accept_invitation_wrong_key_fails() {
        let owner = gen_keypair();
        let invitee = gen_keypair();
        let wrong_keypair = gen_keypair();
        let encryption_key: [u8; 32] = [7u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&invitee),
            MemberRole::Member,
            &encryption_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Someone else tries to accept -- should fail.
        let result = accept_invitation(&bucket, &wrong_keypair).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_invitation_invalid_pubkey_hex() {
        let owner = gen_keypair();
        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);
        let encryption_key: [u8; 32] = [0u8; 32];

        let result = create_invitation(
            &bucket,
            &mut chain,
            &owner,
            "not-valid-hex",
            MemberRole::Member,
            &encryption_key,
            "0000000002000-0000-dev1",
        )
        .await;

        assert!(matches!(result, Err(InviteError::Crypto(_))));
    }

    #[tokio::test]
    async fn create_invitation_non_owner_fails() {
        let owner = gen_keypair();
        let member = gen_keypair();
        let invitee = gen_keypair();
        let encryption_key: [u8; 32] = [0u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Add member first.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member),
            MemberRole::Member,
            &encryption_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Member (not owner) tries to invite someone.
        let result = create_invitation(
            &bucket,
            &mut chain,
            &member,
            &pubkey_hex(&invitee),
            MemberRole::Member,
            &encryption_key,
            "0000000003000-0000-dev1",
        )
        .await;

        assert!(matches!(result, Err(InviteError::Membership(_))));
    }

    #[tokio::test]
    async fn membership_entry_uploaded_to_bucket() {
        let owner = gen_keypair();
        let invitee = gen_keypair();
        let encryption_key: [u8; 32] = [1u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&invitee),
            MemberRole::Member,
            &encryption_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Verify the membership entry was uploaded.
        let entries = bucket.list_membership_entries().await.unwrap();
        let owner_entries: Vec<_> = entries
            .iter()
            .filter(|(author, _)| author == &pubkey_hex(&owner))
            .collect();
        assert_eq!(owner_entries.len(), 1);

        // Verify the wrapped key was uploaded.
        let wrapped = bucket.get_wrapped_key(&pubkey_hex(&invitee)).await.unwrap();
        assert!(!wrapped.is_empty());
    }
}
