/// Invitation and revocation flow for shared library membership.
///
/// `create_invitation()` is called by the library owner to invite a new member.
/// `accept_invitation()` is called by the invitee to unwrap the library key.
/// `revoke_member()` is called by the library owner to remove a member and rotate the key.
use crate::encryption;
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
    #[error("User {0} is not a current member")]
    NotAMember(String),
    #[error("Cannot revoke the last owner of a library")]
    LastOwner,
}

/// Determine the next seq for an author's membership entries in the bucket.
async fn next_membership_seq(
    bucket: &dyn SyncBucketClient,
    author_pubkey_hex: &str,
) -> Result<u64, InviteError> {
    let existing_entries = bucket.list_membership_entries().await?;
    Ok(existing_entries
        .iter()
        .filter(|(author, _)| author == author_pubkey_hex)
        .map(|(_, seq)| seq)
        .max()
        .map_or(1, |max| max + 1))
}

/// Decode and convert an Ed25519 hex pubkey to X25519 for sealed box encryption.
fn ed25519_hex_to_x25519(
    ed25519_pubkey_hex: &str,
) -> Result<[u8; sodium_ffi::CURVE25519_PUBLICKEYBYTES], InviteError> {
    let pk_bytes: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] = hex::decode(ed25519_pubkey_hex)
        .map_err(|e| InviteError::Crypto(format!("invalid pubkey hex: {e}")))?
        .try_into()
        .map_err(|_| InviteError::Crypto("pubkey wrong length".to_string()))?;
    Ok(keys::ed25519_to_x25519_public_key(&pk_bytes))
}

/// Upload a signed membership entry to the bucket.
async fn upload_membership_entry(
    bucket: &dyn SyncBucketClient,
    entry: &MembershipEntry,
    author_pubkey_hex: &str,
) -> Result<(), InviteError> {
    let next_seq = next_membership_seq(bucket, author_pubkey_hex).await?;

    let entry_bytes =
        serde_json::to_vec(entry).map_err(|e| InviteError::Crypto(format!("serialize: {e}")))?;
    bucket
        .put_membership_entry(author_pubkey_hex, next_seq, entry_bytes)
        .await?;

    Ok(())
}

/// Create an invitation for a new member.
///
/// This wraps the library encryption key to the invitee's X25519 public key,
/// creates and signs a membership entry (Add), validates it against the local
/// chain, and only then uploads both to the bucket.
pub async fn create_invitation(
    bucket: &dyn SyncBucketClient,
    chain: &mut MembershipChain,
    owner_keypair: &UserKeypair,
    invitee_ed25519_pubkey: &str,
    role: MemberRole,
    encryption_key: &[u8; 32],
    timestamp: &str,
) -> Result<(), InviteError> {
    // Convert Ed25519 -> X25519 for sealed box encryption.
    let invitee_x25519_pk = ed25519_hex_to_x25519(invitee_ed25519_pubkey)?;

    // Wrap the library encryption key.
    let wrapped_key = keys::seal_box_encrypt(encryption_key, &invitee_x25519_pk);

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

    // Validate against the local chain BEFORE any bucket writes.
    chain.add_entry(entry.clone())?;

    // Upload wrapped key and membership entry.
    bucket
        .put_wrapped_key(invitee_ed25519_pubkey, wrapped_key)
        .await?;

    let author_pubkey_hex = hex::encode(owner_keypair.public_key);
    upload_membership_entry(bucket, &entry, &author_pubkey_hex).await?;

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

/// Revoke a member from the library. This:
/// 1. Creates a Remove membership entry signed by the owner
/// 2. Generates a new library encryption key
/// 3. Re-wraps the new key to all remaining members
/// 4. Deletes the revoked member's wrapped key
/// 5. Uploads updated entries and keys
///
/// Returns the new encryption key (caller must persist it and start using it).
pub async fn revoke_member(
    bucket: &dyn SyncBucketClient,
    chain: &mut MembershipChain,
    owner_keypair: &UserKeypair,
    revokee_pubkey: &str,
    timestamp: &str,
) -> Result<[u8; 32], InviteError> {
    let members = chain.current_members();

    // Verify the revokee is a current member.
    if !members.iter().any(|(pk, _)| pk == revokee_pubkey) {
        return Err(InviteError::NotAMember(revokee_pubkey.to_string()));
    }

    // Ensure at least one owner would remain after the removal.
    let remaining_owners = members
        .iter()
        .filter(|(pk, role)| pk != revokee_pubkey && *role == MemberRole::Owner)
        .count();
    if remaining_owners == 0 {
        return Err(InviteError::LastOwner);
    }

    // Create and sign a Remove entry.
    let mut entry = MembershipEntry {
        action: MembershipAction::Remove,
        user_pubkey: revokee_pubkey.to_string(),
        role: MemberRole::Member, // role field is not meaningful for Remove, but required
        timestamp: timestamp.to_string(),
        author_pubkey: String::new(),
        signature: String::new(),
    };
    sign_membership_entry(&mut entry, owner_keypair);

    // Validate against the local chain BEFORE any bucket writes.
    chain.add_entry(entry.clone())?;

    // Upload the Remove entry.
    let author_pubkey_hex = hex::encode(owner_keypair.public_key);
    upload_membership_entry(bucket, &entry, &author_pubkey_hex).await?;

    // Generate a new random encryption key.
    let new_key = encryption::generate_random_key();

    // Re-wrap the new key to all remaining members.
    let remaining_members = chain.current_members();
    for (member_pubkey, _) in &remaining_members {
        let x25519_pk = ed25519_hex_to_x25519(member_pubkey)?;
        let wrapped = keys::seal_box_encrypt(&new_key, &x25519_pk);
        bucket.put_wrapped_key(member_pubkey, wrapped).await?;
    }

    // Delete the revoked member's wrapped key.
    bucket.delete_wrapped_key(revokee_pubkey).await?;

    Ok(new_key)
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

    #[tokio::test]
    async fn revoke_member_roundtrip() {
        let owner = gen_keypair();
        let member = gen_keypair();
        let old_key: [u8; 32] = [42u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Owner invites the member.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member),
            MemberRole::Member,
            &old_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Member can unwrap the key.
        let unwrapped = accept_invitation(&bucket, &member).await.unwrap();
        assert_eq!(unwrapped, old_key);

        // Owner revokes the member.
        let new_key = revoke_member(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member),
            "0000000003000-0000-dev1",
        )
        .await
        .unwrap();

        // New key should be different from old key.
        assert_ne!(new_key, old_key);

        // Member is no longer in the chain.
        let members = chain.current_members();
        assert!(!members.iter().any(|(pk, _)| pk == &pubkey_hex(&member)));
        assert!(members.iter().any(|(pk, _)| pk == &pubkey_hex(&owner)));

        // Chain should still validate.
        chain.validate().unwrap();

        // Revoked member's wrapped key was deleted from the bucket.
        let result = bucket.get_wrapped_key(&pubkey_hex(&member)).await;
        assert!(result.is_err());

        // Owner can still unwrap the new key.
        let owner_unwrapped = accept_invitation(&bucket, &owner).await.unwrap();
        assert_eq!(owner_unwrapped, new_key);

        // The Remove entry was uploaded to the bucket.
        let entries = bucket.list_membership_entries().await.unwrap();
        let owner_entries: Vec<_> = entries
            .iter()
            .filter(|(author, _)| author == &pubkey_hex(&owner))
            .collect();
        // 1 for invite + 1 for revoke = 2
        assert_eq!(owner_entries.len(), 2);
    }

    #[tokio::test]
    async fn revoke_member_with_multiple_remaining() {
        let owner = gen_keypair();
        let member1 = gen_keypair();
        let member2 = gen_keypair();
        let old_key: [u8; 32] = [10u8; 32];

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Invite two members.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member1),
            MemberRole::Member,
            &old_key,
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member2),
            MemberRole::Member,
            &old_key,
            "0000000003000-0000-dev1",
        )
        .await
        .unwrap();

        // Revoke member1.
        let new_key = revoke_member(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member1),
            "0000000004000-0000-dev1",
        )
        .await
        .unwrap();

        // Both remaining members (owner + member2) can unwrap the new key.
        let owner_key = accept_invitation(&bucket, &owner).await.unwrap();
        assert_eq!(owner_key, new_key);

        let member2_key = accept_invitation(&bucket, &member2).await.unwrap();
        assert_eq!(member2_key, new_key);

        // member1 cannot get a wrapped key.
        let result = bucket.get_wrapped_key(&pubkey_hex(&member1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn revoke_non_member_fails() {
        let owner = gen_keypair();
        let outsider = gen_keypair();

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        let result = revoke_member(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&outsider),
            "0000000002000-0000-dev1",
        )
        .await;

        assert!(matches!(result, Err(InviteError::NotAMember(_))));
    }

    #[tokio::test]
    async fn revoke_last_owner_fails() {
        let owner = gen_keypair();
        let member = gen_keypair();

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Add a regular member.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member),
            MemberRole::Member,
            &[42u8; 32],
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        // Owner tries to revoke themselves (the only owner).
        let result = revoke_member(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&owner),
            "0000000003000-0000-dev1",
        )
        .await;

        assert!(matches!(result, Err(InviteError::LastOwner)));
    }

    #[tokio::test]
    async fn non_owner_revoke_fails() {
        let owner = gen_keypair();
        let member1 = gen_keypair();
        let member2 = gen_keypair();

        let bucket = MockBucket::new();
        let mut chain = bootstrap_chain(&owner);

        // Add two members.
        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member1),
            MemberRole::Member,
            &[42u8; 32],
            "0000000002000-0000-dev1",
        )
        .await
        .unwrap();

        create_invitation(
            &bucket,
            &mut chain,
            &owner,
            &pubkey_hex(&member2),
            MemberRole::Member,
            &[42u8; 32],
            "0000000003000-0000-dev1",
        )
        .await
        .unwrap();

        // Member (not owner) tries to revoke another member.
        let result = revoke_member(
            &bucket,
            &mut chain,
            &member1,
            &pubkey_hex(&member2),
            "0000000004000-0000-dev1",
        )
        .await;

        assert!(matches!(result, Err(InviteError::Membership(_))));
    }
}
