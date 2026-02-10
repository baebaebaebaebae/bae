/// Snapshots and garbage collection for the sync system.
///
/// Periodically, a device creates a full snapshot of the database via
/// `VACUUM INTO`, encrypts it, and uploads as `snapshot.db.enc`. This
/// allows new devices to bootstrap without replaying the entire changeset
/// history, and enables GC of old changesets.
///
/// Snapshot creation policy: after every N changesets (default 100) or
/// T hours (default 24) since the last snapshot.
use std::ffi::CString;
use std::path::Path;

use libsqlite3_sys as ffi;
use tracing::{info, warn};

use super::bucket::{BucketError, SyncBucketClient};
use crate::encryption::EncryptionService;

/// Default: create a snapshot after this many changesets since the last one.
const SNAPSHOT_CHANGESET_THRESHOLD: u64 = 100;

/// Default: create a snapshot after this many hours since the last one.
const SNAPSHOT_HOURS_THRESHOLD: u64 = 24;

/// Error type for snapshot operations.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("VACUUM INTO failed: {0}")]
    VacuumFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bucket error: {0}")]
    Bucket(#[from] BucketError),
    #[error("decryption failed: {0}")]
    Decryption(String),
}

/// Create a snapshot of the database as encrypted bytes.
///
/// Uses `VACUUM INTO` to create a clean copy of the database at a temp path,
/// reads the bytes, encrypts, and returns the encrypted blob.
///
/// # Safety
/// `db` must be a valid, open sqlite3 connection pointer.
pub unsafe fn create_snapshot(
    db: *mut ffi::sqlite3,
    temp_dir: &Path,
    encryption: &EncryptionService,
) -> Result<Vec<u8>, SnapshotError> {
    let snapshot_path = temp_dir.join("snapshot.db");
    let path_str = snapshot_path
        .to_str()
        .expect("temp path should be valid UTF-8");

    // Remove any leftover snapshot file from a previous failed attempt.
    let _ = std::fs::remove_file(&snapshot_path);

    // VACUUM INTO creates a clean, defragmented copy of the database.
    let sql = format!("VACUUM INTO '{}'", path_str.replace('\'', "''"));
    let c_sql = CString::new(sql).expect("SQL should not contain null bytes");
    let rc = ffi::sqlite3_exec(
        db,
        c_sql.as_ptr(),
        None,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    if rc != ffi::SQLITE_OK {
        let err = ffi::sqlite3_errmsg(db);
        let msg = if err.is_null() {
            format!("sqlite3 error code {rc}")
        } else {
            std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned()
        };
        let _ = std::fs::remove_file(&snapshot_path);
        return Err(SnapshotError::VacuumFailed(msg));
    }

    // Read the snapshot file and encrypt.
    let plaintext = std::fs::read(&snapshot_path)?;
    let _ = std::fs::remove_file(&snapshot_path);

    let encrypted = encryption.encrypt(&plaintext);

    info!(
        plaintext_size = plaintext.len(),
        encrypted_size = encrypted.len(),
        "created snapshot"
    );

    Ok(encrypted)
}

/// Upload a snapshot to the sync bucket and update the device head.
pub async fn push_snapshot(
    bucket: &dyn SyncBucketClient,
    encrypted_snapshot: Vec<u8>,
    device_id: &str,
    current_seq: u64,
) -> Result<(), SnapshotError> {
    let size = encrypted_snapshot.len();
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Upload snapshot (overwrites previous).
    bucket.put_snapshot(encrypted_snapshot).await?;

    // Update head with snapshot_seq.
    bucket
        .put_head(device_id, current_seq, Some(current_seq), &timestamp)
        .await?;

    info!(
        device_id,
        snapshot_seq = current_seq,
        size,
        "pushed snapshot to sync bucket"
    );

    Ok(())
}

/// Check whether it's time to create a new snapshot.
///
/// Returns true if:
/// - `changesets_since_snapshot` >= the changeset threshold (100), OR
/// - `hours_since_snapshot` >= the time threshold (24h), OR
/// - No snapshot has ever been created (`last_snapshot_seq` is None)
///   AND at least one changeset has been pushed.
pub fn should_create_snapshot(
    local_seq: u64,
    last_snapshot_seq: Option<u64>,
    hours_since_snapshot: Option<u64>,
) -> bool {
    // Never created a snapshot, and we have at least one changeset.
    let Some(snap_seq) = last_snapshot_seq else {
        return local_seq > 0;
    };

    let changesets_since = local_seq.saturating_sub(snap_seq);
    if changesets_since >= SNAPSHOT_CHANGESET_THRESHOLD {
        return true;
    }

    if let Some(hours) = hours_since_snapshot {
        if hours >= SNAPSHOT_HOURS_THRESHOLD && changesets_since > 0 {
            return true;
        }
    }

    false
}

/// Delete changesets that are superseded by a snapshot.
///
/// For each device in the bucket, deletes changesets with seq <= `snapshot_seq`.
/// This is safe because the snapshot contains the full database state up to
/// that point. Any device that missed these changesets can bootstrap from
/// the snapshot instead.
///
/// The caller is responsible for the 30-day grace period -- only call this
/// when enough time has passed since the snapshot was created.
pub async fn garbage_collect(
    bucket: &dyn SyncBucketClient,
    snapshot_seq: u64,
) -> Result<GcResult, SnapshotError> {
    let heads = bucket.list_heads().await?;
    let mut deleted = 0u64;
    let mut errors = 0u64;

    for head in &heads {
        let seqs = match bucket.list_changesets(&head.device_id).await {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    device_id = %head.device_id,
                    error = %e,
                    "failed to list changesets for GC, skipping device"
                );
                errors += 1;
                continue;
            }
        };

        for seq in seqs {
            if seq > snapshot_seq {
                continue;
            }

            match bucket.delete_changeset(&head.device_id, seq).await {
                Ok(()) => deleted += 1,
                Err(e) => {
                    warn!(
                        device_id = %head.device_id,
                        seq,
                        error = %e,
                        "failed to delete changeset during GC"
                    );
                    errors += 1;
                }
            }
        }
    }

    info!(deleted, errors, snapshot_seq, "garbage collection complete");

    Ok(GcResult { deleted, errors })
}

/// Result of a garbage collection run.
#[derive(Debug, PartialEq, Eq)]
pub struct GcResult {
    /// Number of changesets successfully deleted.
    pub deleted: u64,
    /// Number of errors encountered (logged but not fatal).
    pub errors: u64,
}

/// Bootstrap a new device from a snapshot.
///
/// Downloads `snapshot.db.enc`, decrypts, and writes the plaintext database
/// to `target_path`. The caller should then open this as their local database
/// and pull any changesets with seq > the snapshot's seq.
///
/// Returns the snapshot_seq (maximum across all device heads) so the caller
/// knows where to start pulling changesets from.
pub async fn bootstrap_from_snapshot(
    bucket: &dyn SyncBucketClient,
    encryption: &EncryptionService,
    target_path: &Path,
) -> Result<u64, SnapshotError> {
    // Download encrypted snapshot.
    let encrypted = bucket.get_snapshot().await?;

    // Decrypt.
    let plaintext = encryption
        .decrypt(&encrypted)
        .map_err(|e| SnapshotError::Decryption(e.to_string()))?;

    // Write to target path.
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target_path, &plaintext)?;

    // Determine the snapshot_seq from the heads.
    let heads = bucket.list_heads().await?;
    let snapshot_seq = heads
        .iter()
        .filter_map(|h| h.snapshot_seq)
        .max()
        .unwrap_or(0);

    info!(
        snapshot_seq,
        db_size = plaintext.len(),
        path = %target_path.display(),
        "bootstrapped from snapshot"
    );

    Ok(snapshot_seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bucket::DeviceHead;
    use crate::sync::session::SyncSession;
    use crate::sync::test_helpers::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Full-featured mock bucket for snapshot tests.
    struct MockBucket {
        changesets: Mutex<HashMap<String, Vec<u8>>>,
        heads: Mutex<HashMap<String, (u64, Option<u64>)>>,
        snapshot: Mutex<Option<Vec<u8>>>,
        min_schema_version: Mutex<Option<u32>>,
    }

    impl MockBucket {
        fn new() -> Self {
            MockBucket {
                changesets: Mutex::new(HashMap::new()),
                heads: Mutex::new(HashMap::new()),
                snapshot: Mutex::new(None),
                min_schema_version: Mutex::new(None),
            }
        }

        /// Helper to add a changeset directly.
        fn add_changeset(&self, device_id: &str, seq: u64, data: Vec<u8>) {
            let key = format!("{device_id}/{seq}");
            self.changesets.lock().unwrap().insert(key, data);

            let mut heads = self.heads.lock().unwrap();
            let entry = heads.entry(device_id.to_string()).or_insert((0, None));
            if seq > entry.0 {
                entry.0 = seq;
            }
        }

        /// Count remaining changesets.
        fn changeset_count(&self) -> usize {
            self.changesets.lock().unwrap().len()
        }

        /// Get stored snapshot data.
        fn get_stored_snapshot(&self) -> Option<Vec<u8>> {
            self.snapshot.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SyncBucketClient for MockBucket {
        async fn list_heads(&self) -> Result<Vec<DeviceHead>, BucketError> {
            let heads = self.heads.lock().unwrap();
            Ok(heads
                .iter()
                .map(|(id, (seq, snap))| DeviceHead {
                    device_id: id.clone(),
                    seq: *seq,
                    snapshot_seq: *snap,
                    last_sync: None,
                })
                .collect())
        }

        async fn get_changeset(&self, device_id: &str, seq: u64) -> Result<Vec<u8>, BucketError> {
            let key = format!("{device_id}/{seq}");
            let cs = self.changesets.lock().unwrap();
            cs.get(&key).cloned().ok_or(BucketError::NotFound(key))
        }

        async fn put_changeset(
            &self,
            device_id: &str,
            seq: u64,
            data: Vec<u8>,
        ) -> Result<(), BucketError> {
            let key = format!("{device_id}/{seq}");
            self.changesets.lock().unwrap().insert(key, data);
            Ok(())
        }

        async fn put_head(
            &self,
            device_id: &str,
            seq: u64,
            snapshot_seq: Option<u64>,
            _timestamp: &str,
        ) -> Result<(), BucketError> {
            let mut heads = self.heads.lock().unwrap();
            let entry = heads.entry(device_id.to_string()).or_insert((0, None));
            entry.0 = seq;
            if snapshot_seq.is_some() {
                entry.1 = snapshot_seq;
            }
            Ok(())
        }

        async fn upload_image(&self, _id: &str, _data: Vec<u8>) -> Result<(), BucketError> {
            Ok(())
        }

        async fn download_image(&self, id: &str) -> Result<Vec<u8>, BucketError> {
            Err(BucketError::NotFound(format!("images/{id}")))
        }

        async fn put_snapshot(&self, data: Vec<u8>) -> Result<(), BucketError> {
            *self.snapshot.lock().unwrap() = Some(data);
            Ok(())
        }

        async fn get_snapshot(&self) -> Result<Vec<u8>, BucketError> {
            self.snapshot
                .lock()
                .unwrap()
                .clone()
                .ok_or(BucketError::NotFound("snapshot.db.enc".into()))
        }

        async fn delete_changeset(&self, device_id: &str, seq: u64) -> Result<(), BucketError> {
            let key = format!("{device_id}/{seq}");
            self.changesets.lock().unwrap().remove(&key);
            Ok(())
        }

        async fn list_changesets(&self, device_id: &str) -> Result<Vec<u64>, BucketError> {
            let prefix = format!("{device_id}/");
            let cs = self.changesets.lock().unwrap();
            let mut seqs: Vec<u64> = cs
                .keys()
                .filter_map(|k| k.strip_prefix(&prefix).and_then(|s| s.parse().ok()))
                .collect();
            seqs.sort();
            Ok(seqs)
        }

        async fn get_min_schema_version(&self) -> Result<Option<u32>, BucketError> {
            Ok(*self.min_schema_version.lock().unwrap())
        }

        async fn set_min_schema_version(&self, version: u32) -> Result<(), BucketError> {
            *self.min_schema_version.lock().unwrap() = Some(version);
            Ok(())
        }
    }

    fn test_encryption() -> EncryptionService {
        EncryptionService::new_with_key(&[0x42u8; 32])
    }

    // ---- should_create_snapshot tests ----

    #[test]
    fn snapshot_policy_no_previous_snapshot_with_changes() {
        assert!(should_create_snapshot(1, None, None));
        assert!(should_create_snapshot(50, None, None));
    }

    #[test]
    fn snapshot_policy_no_previous_snapshot_no_changes() {
        assert!(!should_create_snapshot(0, None, None));
    }

    #[test]
    fn snapshot_policy_below_threshold() {
        // 10 changesets since last snapshot, only 1 hour elapsed.
        assert!(!should_create_snapshot(60, Some(50), Some(1)));
    }

    #[test]
    fn snapshot_policy_changeset_threshold_reached() {
        // Exactly 100 changesets since snapshot.
        assert!(should_create_snapshot(150, Some(50), Some(1)));
        // Over 100.
        assert!(should_create_snapshot(200, Some(50), Some(1)));
    }

    #[test]
    fn snapshot_policy_time_threshold_reached() {
        // Only 10 changesets but 24+ hours have passed.
        assert!(should_create_snapshot(60, Some(50), Some(24)));
        assert!(should_create_snapshot(60, Some(50), Some(48)));
    }

    #[test]
    fn snapshot_policy_time_threshold_no_new_changes() {
        // 24 hours but zero changesets since snapshot.
        assert!(!should_create_snapshot(50, Some(50), Some(24)));
    }

    // ---- create_snapshot tests ----

    #[test]
    fn create_snapshot_produces_encrypted_db() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

            exec(
                db,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
            );

            let temp = tempfile::tempdir().unwrap();
            let enc = test_encryption();

            let encrypted =
                create_snapshot(db, temp.path(), &enc).expect("create_snapshot should succeed");

            // Should be non-empty encrypted bytes.
            assert!(!encrypted.is_empty());

            // Should be decryptable.
            let plaintext = enc.decrypt(&encrypted).expect("decrypt should succeed");
            assert!(!plaintext.is_empty());

            // The plaintext should be a valid SQLite database (starts with "SQLite format 3\0").
            assert!(
                plaintext.starts_with(b"SQLite format 3\0"),
                "snapshot should be a valid SQLite database"
            );

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn create_snapshot_contains_data() {
        unsafe {
            let db = open_memory_db();
            create_synced_schema(db);

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

            let temp = tempfile::tempdir().unwrap();
            let enc = test_encryption();

            let encrypted = create_snapshot(db, temp.path(), &enc).expect("snapshot");
            let plaintext = enc.decrypt(&encrypted).expect("decrypt");

            // Write to file and open to verify contents.
            let db_path = temp.path().join("verify.db");
            std::fs::write(&db_path, &plaintext).unwrap();

            let db2 = {
                let c_path = CString::new(db_path.to_str().unwrap()).unwrap();
                let mut ptr: *mut ffi::sqlite3 = std::ptr::null_mut();
                let rc = ffi::sqlite3_open(c_path.as_ptr(), &mut ptr);
                assert_eq!(rc, ffi::SQLITE_OK);
                ptr
            };

            let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
            assert_eq!(name, "Miles Davis");

            let title = query_text(db2, "SELECT title FROM albums WHERE id = 'al1'");
            assert_eq!(title, "Kind of Blue");

            ffi::sqlite3_close(db2);
            ffi::sqlite3_close(db);
        }
    }

    // ---- push_snapshot tests ----

    #[tokio::test]
    async fn push_snapshot_uploads_and_updates_head() {
        let bucket = MockBucket::new();
        let data = vec![1, 2, 3, 4, 5];

        push_snapshot(&bucket, data.clone(), "dev-1", 42)
            .await
            .expect("push_snapshot should succeed");

        // Snapshot should be stored.
        assert_eq!(bucket.get_stored_snapshot(), Some(data));

        // Head should be updated with snapshot_seq.
        let heads = bucket.list_heads().await.unwrap();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].device_id, "dev-1");
        assert_eq!(heads[0].seq, 42);
        assert_eq!(heads[0].snapshot_seq, Some(42));
    }

    // ---- garbage_collect tests ----

    #[tokio::test]
    async fn gc_deletes_changesets_up_to_snapshot_seq() {
        let bucket = MockBucket::new();

        // Device A: changesets 1-5.
        for seq in 1..=5 {
            bucket.add_changeset("dev-a", seq, vec![seq as u8]);
        }
        // Device B: changesets 1-3.
        for seq in 1..=3 {
            bucket.add_changeset("dev-b", seq, vec![seq as u8]);
        }

        assert_eq!(bucket.changeset_count(), 8);

        // Snapshot at seq 3 -- delete changesets <= 3 from all devices.
        let result = garbage_collect(&bucket, 3).await.expect("gc");

        assert_eq!(result.deleted, 6); // dev-a: 1,2,3 + dev-b: 1,2,3
        assert_eq!(result.errors, 0);
        assert_eq!(bucket.changeset_count(), 2); // dev-a: 4,5 remain

        // Verify remaining changesets.
        let remaining_a = bucket.list_changesets("dev-a").await.unwrap();
        assert_eq!(remaining_a, vec![4, 5]);

        let remaining_b = bucket.list_changesets("dev-b").await.unwrap();
        assert!(remaining_b.is_empty());
    }

    #[tokio::test]
    async fn gc_with_no_changesets_to_delete() {
        let bucket = MockBucket::new();
        bucket.add_changeset("dev-a", 10, vec![10]);

        let result = garbage_collect(&bucket, 5).await.expect("gc");

        assert_eq!(result.deleted, 0);
        assert_eq!(bucket.changeset_count(), 1);
    }

    #[tokio::test]
    async fn gc_with_empty_bucket() {
        let bucket = MockBucket::new();

        let result = garbage_collect(&bucket, 100).await.expect("gc");

        assert_eq!(result.deleted, 0);
        assert_eq!(result.errors, 0);
    }

    // ---- bootstrap_from_snapshot tests ----

    #[tokio::test]
    async fn bootstrap_downloads_decrypts_and_writes_db() {
        unsafe {
            // First create a snapshot from a real database.
            let db = open_memory_db();
            create_synced_schema(db);

            exec(
                db,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
            );

            let temp = tempfile::tempdir().unwrap();
            let enc = test_encryption();

            let encrypted = create_snapshot(db, temp.path(), &enc).expect("snapshot");
            ffi::sqlite3_close(db);

            // Put snapshot in mock bucket.
            let bucket = MockBucket::new();
            bucket.put_snapshot(encrypted).await.unwrap();
            // Set a head with snapshot_seq so bootstrap can find it.
            bucket
                .put_head("dev-1", 10, Some(10), "2026-02-10T00:00:00Z")
                .await
                .unwrap();

            // Bootstrap a new database.
            let target = temp.path().join("bootstrapped.db");
            let snapshot_seq = bootstrap_from_snapshot(&bucket, &enc, &target)
                .await
                .expect("bootstrap");

            assert_eq!(snapshot_seq, 10);
            assert!(target.exists());

            // Open the bootstrapped DB and verify data.
            let c_path = CString::new(target.to_str().unwrap()).unwrap();
            let mut db2: *mut ffi::sqlite3 = std::ptr::null_mut();
            let rc = ffi::sqlite3_open(c_path.as_ptr(), &mut db2);
            assert_eq!(rc, ffi::SQLITE_OK);

            let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
            assert_eq!(name, "Miles Davis");

            ffi::sqlite3_close(db2);
        }
    }

    #[tokio::test]
    async fn bootstrap_fails_when_no_snapshot_exists() {
        let bucket = MockBucket::new();
        let enc = test_encryption();
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("nope.db");

        let result = bootstrap_from_snapshot(&bucket, &enc, &target).await;

        assert!(result.is_err());
        assert!(!target.exists());
    }

    // ---- Integration: create, push, bootstrap, verify ----

    #[tokio::test]
    async fn full_snapshot_round_trip() {
        unsafe {
            // Device 1 creates some data.
            let db = open_memory_db();
            create_synced_schema(db);

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

            let temp = tempfile::tempdir().unwrap();
            let enc = test_encryption();
            let bucket = MockBucket::new();

            // Create and push snapshot at seq 5.
            let encrypted = create_snapshot(db, temp.path(), &enc).expect("snapshot");
            push_snapshot(&bucket, encrypted, "dev-1", 5)
                .await
                .expect("push");

            ffi::sqlite3_close(db);

            // Device 2 bootstraps.
            let target = temp.path().join("device2.db");
            let snapshot_seq = bootstrap_from_snapshot(&bucket, &enc, &target)
                .await
                .expect("bootstrap");

            assert_eq!(snapshot_seq, 5);

            // Open and verify.
            let c_path = CString::new(target.to_str().unwrap()).unwrap();
            let mut db2: *mut ffi::sqlite3 = std::ptr::null_mut();
            let rc = ffi::sqlite3_open(c_path.as_ptr(), &mut db2);
            assert_eq!(rc, ffi::SQLITE_OK);

            let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
            assert_eq!(name, "Miles Davis");

            let title = query_text(db2, "SELECT title FROM albums WHERE id = 'al1'");
            assert_eq!(title, "Kind of Blue");

            // Device 2 can now pull only changesets > snapshot_seq.
            // (Not tested here since pull is already tested in pull_tests.rs.)

            ffi::sqlite3_close(db2);
        }
    }

    /// Verify that a snapshot + subsequent changesets produces the same state
    /// as applying all changesets from scratch (roadmap test item #6).
    #[tokio::test]
    async fn snapshot_plus_changesets_equals_full_replay() {
        unsafe {
            let enc = test_encryption();
            let temp = tempfile::tempdir().unwrap();

            // --- Phase 1: create data, snapshot, then more data ---

            let db_source = open_memory_db();
            create_synced_schema(db_source);

            // Initial data (before snapshot).
            let session1 = SyncSession::start(db_source).expect("session");
            exec(
                db_source,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
            );
            exec(
                db_source,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a2', 'John Coltrane', '0000000002000-0000-dev1', '2026-01-01')",
            );
            let cs1 = session1.changeset().unwrap().unwrap();
            let cs1_bytes = cs1.as_bytes().to_vec();
            drop(session1);

            // Create snapshot after cs1.
            let snapshot_encrypted =
                create_snapshot(db_source, temp.path(), &enc).expect("snapshot");

            // More data after snapshot.
            let session2 = SyncSession::start(db_source).expect("session2");
            exec(
                db_source,
                "INSERT INTO artists (id, name, _updated_at, created_at) \
                 VALUES ('a3', 'Bill Evans', '0000000003000-0000-dev1', '2026-01-01')",
            );
            exec(
                db_source,
                "UPDATE artists SET name = 'Miles Dewey Davis' \
                 WHERE id = 'a1'",
            );
            let cs2 = session2.changeset().unwrap().unwrap();
            let cs2_bytes = cs2.as_bytes().to_vec();
            drop(session2);

            ffi::sqlite3_close(db_source);

            // --- Path A: bootstrap from snapshot + apply cs2 ---

            let snapshot_plain = enc.decrypt(&snapshot_encrypted).unwrap();
            let path_a = temp.path().join("path_a.db");
            std::fs::write(&path_a, &snapshot_plain).unwrap();

            let db_a = {
                let c = CString::new(path_a.to_str().unwrap()).unwrap();
                let mut p: *mut ffi::sqlite3 = std::ptr::null_mut();
                ffi::sqlite3_open(c.as_ptr(), &mut p);
                p
            };

            let cs2_obj = crate::sync::session_ext::Changeset::from_bytes(&cs2_bytes);
            crate::sync::apply::apply_changeset_lww(db_a, &cs2_obj).expect("apply cs2");

            // --- Path B: fresh DB + apply cs1 + apply cs2 ---

            let db_b = open_memory_db();
            create_synced_schema(db_b);

            let cs1_obj = crate::sync::session_ext::Changeset::from_bytes(&cs1_bytes);
            crate::sync::apply::apply_changeset_lww(db_b, &cs1_obj).expect("apply cs1");

            let cs2_obj2 = crate::sync::session_ext::Changeset::from_bytes(&cs2_bytes);
            crate::sync::apply::apply_changeset_lww(db_b, &cs2_obj2).expect("apply cs2");

            // --- Compare: both paths should have identical data ---

            let count_a = query_int(db_a, "SELECT COUNT(*) FROM artists");
            let count_b = query_int(db_b, "SELECT COUNT(*) FROM artists");
            assert_eq!(count_a, count_b, "artist count should match");
            assert_eq!(count_a, 3);

            let name_a = query_text(db_a, "SELECT name FROM artists WHERE id = 'a1'");
            let name_b = query_text(db_b, "SELECT name FROM artists WHERE id = 'a1'");
            assert_eq!(name_a, name_b);
            assert_eq!(name_a, "Miles Dewey Davis");

            let name_a3 = query_text(db_a, "SELECT name FROM artists WHERE id = 'a3'");
            let name_b3 = query_text(db_b, "SELECT name FROM artists WHERE id = 'a3'");
            assert_eq!(name_a3, name_b3);
            assert_eq!(name_a3, "Bill Evans");

            ffi::sqlite3_close(db_a);
            ffi::sqlite3_close(db_b);
        }
    }
}
