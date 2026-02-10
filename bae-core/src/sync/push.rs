/// Push local changesets to the sync bucket.
///
/// Grabs the changeset from the session, wraps it in an envelope, encrypts,
/// uploads to S3, and updates the head pointer + local seq counter.
use tracing::info;

use super::bucket::{BucketError, SyncBucketClient};
use super::envelope::{self, ChangesetEnvelope};
use super::session::SyncSession;
use crate::encryption::EncryptionService;

/// Current schema version -- matches the latest migration number.
const SCHEMA_VERSION: u32 = 2;

/// Result of a push operation.
#[derive(Debug)]
pub enum PushResult {
    /// No changes to push (session had no writes).
    NothingToPush,
    /// Successfully pushed a changeset with this sequence number.
    Pushed { seq: u64 },
}

/// Push any pending changes from the sync session to the bucket.
///
/// 1. Grabs the changeset from the session (returns NothingToPush if empty).
/// 2. Reads the current local seq, increments it.
/// 3. Builds envelope, packs with changeset, encrypts, uploads.
/// 4. Updates head pointer.
/// 5. Stores new seq in sync_state.
///
/// The `message` parameter is a human-readable description of what changed
/// (e.g., "Imported Kind of Blue"). Callers derive this from the app event
/// that triggered the push.
///
/// `local_seq` is the current sequence number (read from sync_state before
/// calling). The caller is responsible for persisting the incremented value
/// after a successful push.
pub async fn push_changes(
    session: &SyncSession,
    bucket: &dyn SyncBucketClient,
    encryption: &EncryptionService,
    device_id: &str,
    local_seq: u64,
    message: &str,
    timestamp: &str,
) -> Result<PushResult, PushError> {
    // 1. Grab changeset
    let changeset = session
        .changeset()
        .map_err(|e| PushError::Session(e.to_string()))?;

    let changeset = match changeset {
        Some(cs) => cs,
        None => return Ok(PushResult::NothingToPush),
    };

    let changeset_bytes = changeset.as_bytes();
    let new_seq = local_seq + 1;

    // 2. Build envelope
    let envelope = ChangesetEnvelope {
        device_id: device_id.to_string(),
        seq: new_seq,
        schema_version: SCHEMA_VERSION,
        message: message.to_string(),
        timestamp: timestamp.to_string(),
        changeset_size: changeset_bytes.len(),
    };

    // 3. Pack envelope + changeset
    let packed = envelope::pack(&envelope, changeset_bytes);

    // 4. Encrypt
    let encrypted = encryption.encrypt(&packed);

    // 5. Upload changeset
    bucket
        .put_changeset(device_id, new_seq, encrypted)
        .await
        .map_err(|e| PushError::Bucket(e.to_string()))?;

    // 6. Update head pointer
    bucket
        .put_head(device_id, new_seq)
        .await
        .map_err(|e| PushError::Bucket(e.to_string()))?;

    info!(seq = new_seq, device_id, "Pushed changeset to sync bucket");

    Ok(PushResult::Pushed { seq: new_seq })
}

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("session error: {0}")]
    Session(String),
    #[error("bucket error: {0}")]
    Bucket(String),
}

impl From<BucketError> for PushError {
    fn from(e: BucketError) -> Self {
        PushError::Bucket(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bucket::DeviceHead;
    use crate::sync::test_helpers::*;
    use async_trait::async_trait;
    use libsqlite3_sys as ffi;
    use std::sync::Mutex;

    /// In-memory mock of the sync bucket for testing.
    struct MockBucket {
        changesets: Mutex<Vec<(String, u64, Vec<u8>)>>,
        heads: Mutex<Vec<(String, u64)>>,
        images: Mutex<Vec<(String, Vec<u8>)>>,
    }

    impl MockBucket {
        fn new() -> Self {
            Self {
                changesets: Mutex::new(Vec::new()),
                heads: Mutex::new(Vec::new()),
                images: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl SyncBucketClient for MockBucket {
        async fn list_heads(&self) -> Result<Vec<DeviceHead>, BucketError> {
            let heads = self.heads.lock().unwrap();
            Ok(heads
                .iter()
                .map(|(device_id, seq)| DeviceHead {
                    device_id: device_id.clone(),
                    seq: *seq,
                })
                .collect())
        }

        async fn get_changeset(&self, device_id: &str, seq: u64) -> Result<Vec<u8>, BucketError> {
            let changesets = self.changesets.lock().unwrap();
            changesets
                .iter()
                .find(|(d, s, _)| d == device_id && *s == seq)
                .map(|(_, _, data)| data.clone())
                .ok_or_else(|| BucketError::NotFound(format!("changes/{device_id}/{seq}.enc")))
        }

        async fn put_changeset(
            &self,
            device_id: &str,
            seq: u64,
            data: Vec<u8>,
        ) -> Result<(), BucketError> {
            self.changesets
                .lock()
                .unwrap()
                .push((device_id.to_string(), seq, data));
            Ok(())
        }

        async fn put_head(&self, device_id: &str, seq: u64) -> Result<(), BucketError> {
            let mut heads = self.heads.lock().unwrap();
            // Upsert
            if let Some(entry) = heads.iter_mut().find(|(d, _)| d == device_id) {
                entry.1 = seq;
            } else {
                heads.push((device_id.to_string(), seq));
            }
            Ok(())
        }

        async fn upload_image(&self, id: &str, data: Vec<u8>) -> Result<(), BucketError> {
            self.images.lock().unwrap().push((id.to_string(), data));
            Ok(())
        }
    }

    fn test_encryption() -> EncryptionService {
        EncryptionService::new_with_key(&[0x42u8; 32])
    }

    #[tokio::test]
    async fn push_with_no_changes_returns_nothing() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

            let session = SyncSession::start(db).expect("start session");
            let bucket = MockBucket::new();
            let enc = test_encryption();

            let result = push_changes(
                &session,
                &bucket,
                &enc,
                "dev-test",
                0,
                "test",
                "2026-02-10T00:00:00Z",
            )
            .await
            .expect("push should succeed");

            assert!(matches!(result, PushResult::NothingToPush));

            // Nothing should have been uploaded
            assert!(bucket.changesets.lock().unwrap().is_empty());
            assert!(bucket.heads.lock().unwrap().is_empty());

            drop(session);
            ffi::sqlite3_close(db);
        }
    }

    #[tokio::test]
    async fn push_with_changes_uploads_and_increments_seq() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

            let session = SyncSession::start(db).expect("start session");

            // Make a change
            exec(
                db,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
            );

            let bucket = MockBucket::new();
            let enc = test_encryption();

            let result = push_changes(
                &session,
                &bucket,
                &enc,
                "dev-test",
                0,
                "Imported Kind of Blue",
                "2026-02-10T14:30:00Z",
            )
            .await
            .expect("push should succeed");

            match result {
                PushResult::Pushed { seq } => {
                    assert_eq!(seq, 1, "first push should be seq 1");
                }
                PushResult::NothingToPush => panic!("expected Pushed"),
            }

            // Verify changeset was uploaded
            let changesets = bucket.changesets.lock().unwrap();
            assert_eq!(changesets.len(), 1);
            assert_eq!(changesets[0].0, "dev-test");
            assert_eq!(changesets[0].1, 1);

            // Verify the uploaded blob is encrypted (starts with nonce, not JSON)
            let blob = &changesets[0].2;
            assert!(
                blob.len() > 24,
                "blob should be encrypted (nonce + ciphertext)"
            );

            // Decrypt and unpack to verify roundtrip
            let decrypted = enc.decrypt(blob).expect("decrypt should succeed");
            let (envelope, cs_bytes) = envelope::unpack(&decrypted).expect("unpack should succeed");
            assert_eq!(envelope.device_id, "dev-test");
            assert_eq!(envelope.seq, 1);
            assert_eq!(envelope.schema_version, 2);
            assert_eq!(envelope.message, "Imported Kind of Blue");
            assert!(!cs_bytes.is_empty());
            assert_eq!(envelope.changeset_size, cs_bytes.len());

            // Verify head was updated
            let heads = bucket.heads.lock().unwrap();
            assert_eq!(heads.len(), 1);
            assert_eq!(heads[0], ("dev-test".to_string(), 1));

            drop(session);
            ffi::sqlite3_close(db);
        }
    }

    #[tokio::test]
    async fn push_increments_from_existing_seq() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

            let session = SyncSession::start(db).expect("start session");

            exec(
                db,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles', '0000000001000-0000-dev1', '2026-01-01')",
            );

            let bucket = MockBucket::new();
            let enc = test_encryption();

            // Start from seq 41 (simulating prior pushes)
            let result = push_changes(
                &session,
                &bucket,
                &enc,
                "dev-test",
                41,
                "test",
                "2026-02-10T00:00:00Z",
            )
            .await
            .expect("push should succeed");

            match result {
                PushResult::Pushed { seq } => {
                    assert_eq!(seq, 42, "should increment from 41 to 42");
                }
                PushResult::NothingToPush => panic!("expected Pushed"),
            }

            // Verify the uploaded seq
            let changesets = bucket.changesets.lock().unwrap();
            assert_eq!(changesets[0].1, 42);

            let heads = bucket.heads.lock().unwrap();
            assert_eq!(heads[0].1, 42);

            drop(session);
            ffi::sqlite3_close(db);
        }
    }

    /// Verify that pushed blobs can be decrypted and the changeset
    /// can be applied to a fresh database.
    #[tokio::test]
    async fn pushed_changeset_is_applicable() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

            let session = SyncSession::start(db).expect("start session");

            exec(
                db,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
            );
            exec(
                db,
                "INSERT INTO albums (id, title, _updated_at, created_at) \
                 VALUES ('al1', 'Kind of Blue', '0000000001000-0000-dev1', '2026-01-01')",
            );

            let bucket = MockBucket::new();
            let enc = test_encryption();

            push_changes(
                &session,
                &bucket,
                &enc,
                "dev-test",
                0,
                "test",
                "2026-02-10T00:00:00Z",
            )
            .await
            .expect("push");

            // Extract the changeset bytes from the uploaded blob
            let changesets = bucket.changesets.lock().unwrap();
            let decrypted = enc.decrypt(&changesets[0].2).expect("decrypt");
            let (_, cs_bytes) = envelope::unpack(&decrypted).expect("unpack");

            // Apply to a fresh DB
            let db2 = open_memory_db();
            create_synced_schema(db2);

            let cs = crate::sync::session_ext::Changeset::from_bytes(&cs_bytes);
            crate::sync::apply::apply_changeset_lww(db2, &cs).expect("apply");

            let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
            assert_eq!(name, "Miles Davis");

            let title = query_text(db2, "SELECT title FROM albums WHERE id = 'al1'");
            assert_eq!(title, "Kind of Blue");

            drop(session);
            ffi::sqlite3_close(db);
            ffi::sqlite3_close(db2);
        }
    }
}
