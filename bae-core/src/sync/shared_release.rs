/// Aggregated view of shared releases (Phase 3c).
///
/// Bridges accepted share grants to playback by resolving the bucket
/// coordinates and decryption key for any release the user has access to
/// through a share grant.
use chrono::Utc;

use crate::db::{Database, DbShareGrant};
use crate::keys::UserKeypair;
use crate::sync::share_grant::{self, ShareGrant, ShareGrantError};

/// Everything needed to access a shared release's files.
#[derive(Debug, Clone)]
pub struct SharedRelease {
    pub grant_id: String,
    pub release_id: String,
    pub from_library_id: String,
    pub from_user_pubkey: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub release_key: [u8; 32],
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub expires: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SharedReleaseError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Share grant error: {0}")]
    Grant(#[from] ShareGrantError),
    #[error("Invalid release key in DB: {0}")]
    InvalidKey(String),
}

/// Accept a share grant, unwrap its payload, and store it in the local DB.
///
/// The grant is verified (signature + expiry) and the wrapped payload is
/// decrypted using the recipient's keypair. The unwrapped release key and
/// optional S3 credentials are persisted so future `resolve_release` calls
/// don't need the keypair.
pub async fn accept_and_store_grant(
    db: &Database,
    grant: &ShareGrant,
    recipient_keypair: &UserKeypair,
) -> Result<SharedRelease, SharedReleaseError> {
    // Verify signature, check expiry, decrypt the wrapped payload.
    let payload = share_grant::accept_share_grant(grant, recipient_keypair)?;

    let now = Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let release_key_hex = hex::encode(payload.release_key);

    // Store the grant with the unwrapped key and creds.
    let db_grant = DbShareGrant {
        id: id.clone(),
        from_library_id: grant.from_library_id.clone(),
        from_user_pubkey: grant.from_user_pubkey.clone(),
        release_id: grant.release_id.clone(),
        bucket: grant.bucket.clone(),
        region: grant.region.clone(),
        endpoint: grant.endpoint.clone(),
        wrapped_payload: grant.wrapped_payload.clone(),
        expires: grant.expires.clone(),
        signature: grant.signature.clone(),
        accepted_at: Some(now),
        created_at: Utc::now().to_rfc3339(),
        release_key_hex: Some(release_key_hex),
        s3_access_key: payload.s3_access_key.clone(),
        s3_secret_key: payload.s3_secret_key.clone(),
    };

    db.insert_share_grant(&db_grant).await?;

    Ok(SharedRelease {
        grant_id: id,
        release_id: grant.release_id.clone(),
        from_library_id: grant.from_library_id.clone(),
        from_user_pubkey: grant.from_user_pubkey.clone(),
        bucket: grant.bucket.clone(),
        region: grant.region.clone(),
        endpoint: grant.endpoint.clone(),
        release_key: payload.release_key,
        s3_access_key: payload.s3_access_key,
        s3_secret_key: payload.s3_secret_key,
        expires: grant.expires.clone(),
    })
}

/// Resolve a release_id to a `SharedRelease` if an accepted grant exists.
///
/// Returns None if no active (accepted, non-expired) grant covers this release.
pub async fn resolve_release(
    db: &Database,
    release_id: &str,
) -> Result<Option<SharedRelease>, SharedReleaseError> {
    let grants = db.get_share_grants_for_release(release_id).await?;

    // Find the first accepted, non-expired grant with a stored key.
    for grant in grants {
        if let Some(shared) = try_resolve_grant(&grant)? {
            return Ok(Some(shared));
        }
    }

    Ok(None)
}

/// List all active (accepted, non-expired) shared releases.
pub async fn list_shared_releases(db: &Database) -> Result<Vec<SharedRelease>, SharedReleaseError> {
    let grants = db.get_active_share_grants().await?;
    let mut releases = Vec::new();

    for grant in &grants {
        if let Some(shared) = try_resolve_grant(grant)? {
            releases.push(shared);
        }
    }

    Ok(releases)
}

/// Revoke (delete) a grant from the local DB.
pub async fn revoke_grant(db: &Database, grant_id: &str) -> Result<(), SharedReleaseError> {
    db.delete_share_grant(grant_id).await?;
    Ok(())
}

/// Try to build a `SharedRelease` from a DB grant row.
///
/// Returns None if the grant is not accepted or has expired.
/// Returns an error if the stored key is malformed.
fn try_resolve_grant(grant: &DbShareGrant) -> Result<Option<SharedRelease>, SharedReleaseError> {
    // Must be accepted with a stored key.
    let release_key_hex = match &grant.release_key_hex {
        Some(k) if grant.accepted_at.is_some() => k,
        _ => return Ok(None),
    };

    // Check expiry.
    if let Some(expires) = &grant.expires {
        if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires) {
            if Utc::now() > expiry {
                return Ok(None);
            }
        }
    }

    // Decode the stored key.
    let key_bytes = hex::decode(release_key_hex)
        .map_err(|e| SharedReleaseError::InvalidKey(format!("bad hex: {e}")))?;
    let release_key: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| SharedReleaseError::InvalidKey("not 32 bytes".to_string()))?;

    Ok(Some(SharedRelease {
        grant_id: grant.id.clone(),
        release_id: grant.release_id.clone(),
        from_library_id: grant.from_library_id.clone(),
        from_user_pubkey: grant.from_user_pubkey.clone(),
        bucket: grant.bucket.clone(),
        region: grant.region.clone(),
        endpoint: grant.endpoint.clone(),
        release_key,
        s3_access_key: grant.s3_access_key.clone(),
        s3_secret_key: grant.s3_secret_key.clone(),
        expires: grant.expires.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::EncryptionService;
    use crate::keys::UserKeypair;
    use crate::sodium_ffi;
    use crate::sync::share_grant::create_share_grant;
    use tempfile::TempDir;

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

    async fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        (db, dir)
    }

    fn make_grant(
        sender: &UserKeypair,
        recipient: &UserKeypair,
        enc: &EncryptionService,
        release_id: &str,
        expires: Option<&str>,
    ) -> ShareGrant {
        create_share_grant(
            sender,
            &hex::encode(recipient.public_key),
            enc,
            "lib-sender",
            release_id,
            "test-bucket",
            "us-east-1",
            Some("https://s3.example.com"),
            Some("AKID"),
            Some("secret123"),
            expires,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn accept_store_then_resolve() {
        let (db, _dir) = test_db().await;
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();
        let release_id = "rel-001";

        let grant = make_grant(&sender, &recipient, &enc, release_id, None);

        // Accept and store.
        let shared = accept_and_store_grant(&db, &grant, &recipient)
            .await
            .unwrap();

        assert_eq!(shared.release_id, release_id);
        assert_eq!(shared.bucket, "test-bucket");
        assert_eq!(shared.release_key, enc.derive_release_key(release_id));
        assert_eq!(shared.s3_access_key.as_deref(), Some("AKID"));
        assert_eq!(shared.s3_secret_key.as_deref(), Some("secret123"));

        // Resolve by release_id.
        let resolved = resolve_release(&db, release_id).await.unwrap();
        assert!(resolved.is_some());
        let resolved = resolved.unwrap();
        assert_eq!(resolved.release_key, shared.release_key);
        assert_eq!(resolved.grant_id, shared.grant_id);
    }

    #[tokio::test]
    async fn resolve_unknown_release_returns_none() {
        let (db, _dir) = test_db().await;
        let resolved = resolve_release(&db, "nonexistent").await.unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn expired_grant_filtered_out() {
        let (db, _dir) = test_db().await;
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();
        let release_id = "rel-expired";

        // Create a grant that expires far in the future so accept_share_grant succeeds.
        let grant = make_grant(
            &sender,
            &recipient,
            &enc,
            release_id,
            Some("2099-12-31T23:59:59Z"),
        );
        let shared = accept_and_store_grant(&db, &grant, &recipient)
            .await
            .unwrap();

        // Manually set the expiry to the past in the DB.
        let mut conn = db.writer_mutex().unwrap().lock().await;
        sqlx::query("UPDATE share_grants SET expires = '2020-01-01T00:00:00Z' WHERE id = ?")
            .bind(&shared.grant_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        // Resolve should return None (expired).
        let resolved = resolve_release(&db, release_id).await.unwrap();
        assert!(resolved.is_none());

        // list_shared_releases should also exclude it.
        let all = list_shared_releases(&db).await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn list_shared_releases_returns_active() {
        let (db, _dir) = test_db().await;
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();

        // Accept two grants.
        let grant1 = make_grant(&sender, &recipient, &enc, "rel-a", None);
        accept_and_store_grant(&db, &grant1, &recipient)
            .await
            .unwrap();

        let grant2 = make_grant(&sender, &recipient, &enc, "rel-b", None);
        accept_and_store_grant(&db, &grant2, &recipient)
            .await
            .unwrap();

        let all = list_shared_releases(&db).await.unwrap();
        assert_eq!(all.len(), 2);

        let ids: Vec<&str> = all.iter().map(|s| s.release_id.as_str()).collect();
        assert!(ids.contains(&"rel-a"));
        assert!(ids.contains(&"rel-b"));
    }

    #[tokio::test]
    async fn revoke_removes_grant() {
        let (db, _dir) = test_db().await;
        let sender = gen_keypair();
        let recipient = gen_keypair();
        let enc = test_encryption_service();
        let release_id = "rel-revoke";

        let grant = make_grant(&sender, &recipient, &enc, release_id, None);
        let shared = accept_and_store_grant(&db, &grant, &recipient)
            .await
            .unwrap();

        // Revoke.
        revoke_grant(&db, &shared.grant_id).await.unwrap();

        // Should no longer resolve.
        let resolved = resolve_release(&db, release_id).await.unwrap();
        assert!(resolved.is_none());

        // Should not appear in list.
        let all = list_shared_releases(&db).await.unwrap();
        assert!(all.is_empty());
    }
}
