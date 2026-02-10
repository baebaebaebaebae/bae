/// Tests for the pull service and full sync cycle.
///
/// Uses the shared MockBucket from test_helpers and raw sqlite3 connections
/// for the database.
use std::collections::HashMap;

use libsqlite3_sys as ffi;

use crate::sync::bucket::SyncBucketClient;
use crate::sync::envelope;
use crate::sync::pull;
use crate::sync::push::SCHEMA_VERSION;
use crate::sync::service::SyncService;
use crate::sync::session::SyncSession;
use crate::sync::session_ext::Session;
use crate::sync::test_helpers::*;

/// Helper: capture a changeset from a raw db using a session on specific tables.
unsafe fn capture_changeset(db: *mut ffi::sqlite3, tables: &[&str], sql: &[&str]) -> Vec<u8> {
    let session = Session::new(db).expect("session");
    for table in tables {
        session.attach(Some(table)).expect("attach");
    }
    for &stmt in sql {
        exec(db, stmt);
    }
    let cs = session.changeset().expect("changeset");
    let bytes = cs.as_bytes().to_vec();
    drop(session);
    bytes
}

/// Helper: capture a changeset using SyncSession (all synced tables).
unsafe fn capture_sync_changeset(db: *mut ffi::sqlite3, sql: &[&str]) -> Vec<u8> {
    let session = SyncSession::start(db).expect("session");
    for &stmt in sql {
        exec(db, stmt);
    }
    let cs = session
        .changeset()
        .expect("changeset")
        .expect("should have changes");
    cs.as_bytes().to_vec()
}

// ---- Pull tests ----

#[tokio::test]
async fn pull_no_new_changesets() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        let cursors = HashMap::new();

        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 0);
        assert_eq!(result.devices_pulled, 0);
        assert!(updated.is_empty());

        ffi::sqlite3_close(db);
    }
}

#[tokio::test]
async fn pull_cursors_up_to_date() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        // Remote device has seq=3, and our cursor is already at 3.
        let remote_db = open_memory_db();
        create_synced_schema(remote_db);
        let cs_bytes = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'X', '0000000001000-0000-dev-r', '2026-01-01')"],
        );
        bucket.store_changeset("dev-remote", 3, &cs_bytes, SCHEMA_VERSION);

        let mut cursors = HashMap::new();
        cursors.insert("dev-remote".to_string(), 3);

        let (_, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 0);
        assert_eq!(result.devices_pulled, 0);

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

#[tokio::test]
async fn pull_new_changesets_from_one_device() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let remote_db = open_memory_db();
        create_synced_schema(remote_db);

        let bucket = MockBucket::new();

        // Remote pushes two changesets.
        let cs1 = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev-r', '2026-01-01')"],
        );
        bucket.store_changeset("dev-remote", 1, &cs1, SCHEMA_VERSION);

        let cs2 = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a2', 'John Coltrane', '0000000002000-0000-dev-r', '2026-01-01')"],
        );
        bucket.store_changeset("dev-remote", 2, &cs2, SCHEMA_VERSION);

        let cursors = HashMap::new();
        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 2);
        assert_eq!(result.devices_pulled, 1);
        assert_eq!(updated.get("dev-remote"), Some(&2));

        // Verify data arrived.
        let name1 = query_text(db, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name1, "Miles Davis");
        let name2 = query_text(db, "SELECT name FROM artists WHERE id = 'a2'");
        assert_eq!(name2, "John Coltrane");

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

#[tokio::test]
async fn pull_new_changesets_from_multiple_devices() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();

        // Device A
        let db_a = open_memory_db();
        create_synced_schema(db_a);
        let cs_a = capture_changeset(
            db_a,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'From A', '0000000001000-0000-dev-a', '2026-01-01')"],
        );
        bucket.store_changeset("dev-a", 1, &cs_a, SCHEMA_VERSION);

        // Device B
        let db_b = open_memory_db();
        create_synced_schema(db_b);
        let cs_b = capture_changeset(
            db_b,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a2', 'From B', '0000000002000-0000-dev-b', '2026-01-01')"],
        );
        bucket.store_changeset("dev-b", 1, &cs_b, SCHEMA_VERSION);

        let cursors = HashMap::new();
        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 2);
        assert_eq!(result.devices_pulled, 2);
        assert_eq!(updated.get("dev-a"), Some(&1));
        assert_eq!(updated.get("dev-b"), Some(&1));

        let name1 = query_text(db, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name1, "From A");
        let name2 = query_text(db, "SELECT name FROM artists WHERE id = 'a2'");
        assert_eq!(name2, "From B");

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(db_a);
        ffi::sqlite3_close(db_b);
    }
}

#[tokio::test]
async fn pull_skips_own_device() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();

        // Store a changeset under our own device_id.
        let remote_db = open_memory_db();
        create_synced_schema(remote_db);
        let cs = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'self', '0000000001000-0000-dev-local', '2026-01-01')"],
        );
        bucket.store_changeset("dev-local", 1, &cs, SCHEMA_VERSION);

        let cursors = HashMap::new();
        let (_, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        // Should skip our own device.
        assert_eq!(result.changesets_applied, 0);
        assert_eq!(result.devices_pulled, 0);

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

#[tokio::test]
async fn pull_skips_newer_schema_version() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();

        let remote_db = open_memory_db();
        create_synced_schema(remote_db);
        let cs = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'future', '0000000001000-0000-dev-r', '2026-01-01')"],
        );
        // Store with schema_version one higher than ours.
        bucket.store_changeset("dev-remote", 1, &cs, SCHEMA_VERSION + 1);

        let cursors = HashMap::new();
        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 0);
        assert_eq!(result.skipped_schema, 1);
        // Cursor should still advance past the skipped changeset.
        assert_eq!(updated.get("dev-remote"), Some(&1));

        // Data should NOT be applied.
        let exists = row_exists(db, "SELECT 1 FROM artists WHERE id = 'a1'");
        assert!(!exists);

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

#[tokio::test]
async fn pull_applies_current_schema_skips_future() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        let remote_db = open_memory_db();
        create_synced_schema(remote_db);

        // seq=1: current schema (should apply)
        let cs1 = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'current', '0000000001000-0000-dev-r', '2026-01-01')"],
        );
        bucket.store_changeset("dev-remote", 1, &cs1, SCHEMA_VERSION);

        // seq=2: future schema (should skip)
        let cs2 = capture_changeset(
            remote_db,
            &["artists"],
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a2', 'future', '0000000002000-0000-dev-r', '2026-01-01')"],
        );
        bucket.store_changeset("dev-remote", 2, &cs2, SCHEMA_VERSION + 1);

        let cursors = HashMap::new();
        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");

        assert_eq!(result.changesets_applied, 1);
        assert_eq!(result.skipped_schema, 1);
        assert_eq!(updated.get("dev-remote"), Some(&2));

        let exists_a1 = row_exists(db, "SELECT 1 FROM artists WHERE id = 'a1'");
        assert!(exists_a1, "current schema changeset should apply");

        let exists_a2 = row_exists(db, "SELECT 1 FROM artists WHERE id = 'a2'");
        assert!(!exists_a2, "future schema changeset should be skipped");

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

#[tokio::test]
async fn pull_cursor_advancement_is_incremental() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        let remote_db = open_memory_db();
        create_synced_schema(remote_db);

        // Push 3 changesets.
        for i in 1..=3 {
            let cs = capture_changeset(
                remote_db,
                &["artists"],
                &[&format!(
                    "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a{i}', 'Artist {i}', '000000000{i}000-0000-dev-r', '2026-01-01')"
                )],
            );
            bucket.store_changeset("dev-remote", i, &cs, SCHEMA_VERSION);
        }

        // First pull: from 0, gets all 3.
        let cursors = HashMap::new();
        let (updated, result) = pull::pull_changes(db, &bucket, "dev-local", &cursors)
            .await
            .expect("pull");
        assert_eq!(result.changesets_applied, 3);
        assert_eq!(updated.get("dev-remote"), Some(&3));

        // Second pull with updated cursors: nothing new.
        let (_, result2) = pull::pull_changes(db, &bucket, "dev-local", &updated)
            .await
            .expect("pull2");
        assert_eq!(result2.changesets_applied, 0);

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(remote_db);
    }
}

// ---- Full sync cycle tests ----

#[tokio::test]
async fn sync_cycle_push_then_pull() {
    // Simulates two devices: dev-1 writes, pushes, then dev-2 pulls.
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);
        let db2 = open_memory_db();
        create_synced_schema(db2);

        let bucket = MockBucket::new();

        // Device 1: write some data, create outgoing.
        let svc1 = SyncService::new("dev-1".into());
        let session1 = SyncSession::start(db1).expect("start session");
        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev-1', '2026-01-01')",
        );

        let sync_result = svc1
            .sync(
                db1,
                session1,
                0,
                &HashMap::new(),
                &bucket,
                "2026-02-10T00:00:00Z",
                "Imported Kind of Blue",
            )
            .await
            .expect("sync");

        // Push the outgoing changeset to the bucket.
        let outgoing = sync_result.outgoing.expect("should have outgoing");
        assert_eq!(outgoing.seq, 1);
        bucket
            .put_changeset("dev-1", outgoing.seq, outgoing.packed)
            .await
            .expect("put");
        bucket
            .put_head("dev-1", outgoing.seq, None)
            .await
            .expect("put head");

        // Device 2: pull.
        let cursors2 = HashMap::new();
        let (updated2, pull_result) = pull::pull_changes(db2, &bucket, "dev-2", &cursors2)
            .await
            .expect("pull");

        assert_eq!(pull_result.changesets_applied, 1);
        assert_eq!(pull_result.devices_pulled, 1);
        assert_eq!(updated2.get("dev-1"), Some(&1));

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "Miles Davis");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

#[tokio::test]
async fn sync_cycle_bidirectional() {
    // Both devices write and sync. Each should see the other's changes.
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);
        let db2 = open_memory_db();
        create_synced_schema(db2);

        let bucket = MockBucket::new();

        // Device 1 writes.
        let cs1_bytes = capture_sync_changeset(
            db1,
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'From Dev1', '0000000001000-0000-dev-1', '2026-01-01')"],
        );
        bucket.store_changeset("dev-1", 1, &cs1_bytes, SCHEMA_VERSION);

        // Device 2 writes.
        let cs2_bytes = capture_sync_changeset(
            db2,
            &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a2', 'From Dev2', '0000000002000-0000-dev-2', '2026-01-01')"],
        );
        bucket.store_changeset("dev-2", 1, &cs2_bytes, SCHEMA_VERSION);

        // Device 1 pulls (gets a2 from dev-2).
        let (cursors1, r1) = pull::pull_changes(db1, &bucket, "dev-1", &HashMap::new())
            .await
            .expect("pull1");
        assert_eq!(r1.changesets_applied, 1);
        let name_on_1 = query_text(db1, "SELECT name FROM artists WHERE id = 'a2'");
        assert_eq!(name_on_1, "From Dev2");

        // Device 2 pulls (gets a1 from dev-1).
        let (cursors2, r2) = pull::pull_changes(db2, &bucket, "dev-2", &HashMap::new())
            .await
            .expect("pull2");
        assert_eq!(r2.changesets_applied, 1);
        let name_on_2 = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name_on_2, "From Dev1");

        // Both databases should now have both artists.
        assert_eq!(query_int(db1, "SELECT COUNT(*) FROM artists"), 2);
        assert_eq!(query_int(db2, "SELECT COUNT(*) FROM artists"), 2);

        // Cursors should be correct.
        assert_eq!(cursors1.get("dev-2"), Some(&1));
        assert_eq!(cursors2.get("dev-1"), Some(&1));

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

#[tokio::test]
async fn sync_cycle_no_local_changes_returns_none() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        let svc = SyncService::new("dev-local".into());
        let session = SyncSession::start(db).expect("start");

        // No local writes -- session should produce no outgoing changeset.
        let result = svc
            .sync(
                db,
                session,
                0,
                &HashMap::new(),
                &bucket,
                "2026-02-10T00:00:00Z",
                "",
            )
            .await
            .expect("sync");

        assert!(result.outgoing.is_none());
        assert_eq!(result.pull.changesets_applied, 0);

        ffi::sqlite3_close(db);
    }
}

#[tokio::test]
async fn sync_service_outgoing_has_correct_envelope() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let bucket = MockBucket::new();
        let svc = SyncService::new("dev-local".into());
        let session = SyncSession::start(db).expect("start");

        exec(
            db,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Test', '0000000001000-0000-dev-local', '2026-01-01')",
        );

        let result = svc
            .sync(
                db,
                session,
                5,
                &HashMap::new(),
                &bucket,
                "2026-02-10T12:00:00Z",
                "Added Test artist",
            )
            .await
            .expect("sync");

        let outgoing = result.outgoing.expect("should have outgoing");
        assert_eq!(outgoing.seq, 6); // local_seq (5) + 1

        // Unpack the envelope to verify metadata.
        let (env, cs_bytes) = envelope::unpack(&outgoing.packed).expect("unpack");
        assert_eq!(env.device_id, "dev-local");
        assert_eq!(env.seq, 6);
        assert_eq!(env.schema_version, SCHEMA_VERSION);
        assert_eq!(env.timestamp, "2026-02-10T12:00:00Z");
        assert_eq!(env.message, "Added Test artist");
        assert!(!cs_bytes.is_empty());

        ffi::sqlite3_close(db);
    }
}
