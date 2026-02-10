/// Integration tests for the production sync session management.
///
/// Tests use raw sqlite3 connections via libsqlite3-sys to exercise the
/// session/conflict/apply stack end-to-end without needing a full Database.
use crate::sync::apply::apply_changeset_lww;
use crate::sync::session::{SyncSession, SYNCED_TABLES};
use crate::sync::session_ext::{Changeset, Session};
use crate::sync::test_helpers::*;
use libsqlite3_sys as ffi;

// ---- Session attaches correct tables, captures changes ----

#[test]
fn session_attaches_synced_tables_and_captures_changes() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let session = SyncSession::start(db).expect("start session");

        exec(
            db,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Miles Davis', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Kind of Blue', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db,
            "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) VALUES ('img1', 'cover', 'image/jpeg', 1000, 'import', '0000000001000-0000-dev1', '2026-01-01')",
        );

        let cs = session.changeset().expect("changeset");
        assert!(cs.is_some(), "should have changes");
        let cs = cs.unwrap();
        assert!(!cs.is_empty());

        // Apply to a second DB
        let db2 = open_memory_db();
        create_synced_schema(db2);

        apply_changeset_lww(db2, &cs).expect("apply");

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "Miles Davis");

        let title = query_text(db2, "SELECT title FROM albums WHERE id = 'al1'");
        assert_eq!(title, "Kind of Blue");

        let img_type = query_text(db2, "SELECT type FROM library_images WHERE id = 'img1'");
        assert_eq!(img_type, "cover");

        ffi::sqlite3_close(db);
        ffi::sqlite3_close(db2);
    }
}

#[test]
fn session_does_not_capture_non_synced_tables() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let session = SyncSession::start(db).expect("start session");

        // Write to a non-synced table
        exec(
            db,
            "INSERT INTO storage_profiles (id, name, location, location_path, created_at, updated_at) VALUES ('sp1', 'test', 'local', '/tmp', '2026-01-01', '2026-01-01')",
        );

        let cs = session.changeset().expect("changeset");
        assert!(cs.is_none(), "should have no synced changes");

        ffi::sqlite3_close(db);
    }
}

// ---- Empty changeset returns None ----

#[test]
fn empty_changeset_returns_none() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        let session = SyncSession::start(db).expect("start session");

        let cs = session.changeset().expect("changeset");
        assert!(cs.is_none(), "no changes should return None");

        ffi::sqlite3_close(db);
    }
}

// ---- LWW: newer _updated_at wins on DATA conflict ----

#[test]
fn lww_newer_updated_at_wins_data_conflict() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Original', '0000000001000-0000-dev1', '2026-01-01')",
        );

        let session = Session::new(db1).expect("session");
        session.attach(Some("artists")).expect("attach");

        // Device 1 updates with a NEWER timestamp
        exec(
            db1,
            "UPDATE artists SET name = 'From Dev1', _updated_at = '0000000003000-0000-dev1' WHERE id = 'a1'",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Device 2 has the same row with an OLDER local edit
        let db2 = open_memory_db();
        create_synced_schema(db2);
        exec(
            db2,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'From Dev2', '0000000002000-0000-dev2', '2026-01-01')",
        );

        apply_changeset_lww(db2, &cs).expect("apply");

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "From Dev1", "newer incoming should win");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

#[test]
fn lww_older_updated_at_loses_data_conflict() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Original', '0000000001000-0000-dev1', '2026-01-01')",
        );

        let session = Session::new(db1).expect("session");
        session.attach(Some("artists")).expect("attach");

        exec(
            db1,
            "UPDATE artists SET name = 'From Dev1', _updated_at = '0000000002000-0000-dev1' WHERE id = 'a1'",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Device 2 has a NEWER local edit
        let db2 = open_memory_db();
        create_synced_schema(db2);
        exec(
            db2,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'From Dev2', '0000000005000-0000-dev2', '2026-01-01')",
        );

        apply_changeset_lww(db2, &cs).expect("apply");

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "From Dev2", "older incoming should lose, local kept");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

// ---- Delete wins over edit (NOTFOUND) ----

#[test]
fn delete_wins_over_incoming_update() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Original', '0000000001000-0000-dev1', '2026-01-01')",
        );

        let session = Session::new(db1).expect("session");
        session.attach(Some("artists")).expect("attach");

        exec(
            db1,
            "UPDATE artists SET name = 'Updated', _updated_at = '0000000003000-0000-dev1' WHERE id = 'a1'",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Device 2 has DELETED the row (it never existed)
        let db2 = open_memory_db();
        create_synced_schema(db2);

        apply_changeset_lww(db2, &cs).expect("apply");

        // Row should NOT appear (NOTFOUND -> OMIT)
        let exists = row_exists(db2, "SELECT 1 FROM artists WHERE id = 'a1'");
        assert!(!exists, "deleted row should stay deleted");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

// ---- FK constraint retry succeeds after parent inserted ----

/// Test that applying changesets in seq order (parent first, child second)
/// correctly inserts both rows with FK relationships intact.
#[test]
fn fk_parent_then_child_changesets_apply_correctly() {
    unsafe {
        let db_source = open_memory_db();
        create_synced_schema(db_source);

        // Record album insert (changeset seq=1)
        let session = Session::new(db_source).expect("session");
        session.attach(Some("albums")).expect("attach");
        exec(
            db_source,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Kind of Blue', '0000000001000-0000-dev1', '2026-01-01')",
        );
        let album_cs = session.changeset().expect("changeset");
        drop(session);

        // Record release insert (changeset seq=2, references al1)
        let session = Session::new(db_source).expect("session");
        session.attach(Some("releases")).expect("attach");
        exec(
            db_source,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        let release_cs = session.changeset().expect("changeset");
        drop(session);

        let db_target = open_memory_db();
        create_synced_schema(db_target);

        // Apply in seq order: parent first, then child
        apply_changeset_lww(db_target, &album_cs).expect("apply album");
        apply_changeset_lww(db_target, &release_cs).expect("apply release");

        let has_album = row_exists(db_target, "SELECT 1 FROM albums WHERE id = 'al1'");
        assert!(has_album, "album should exist");

        let has_release = row_exists(db_target, "SELECT 1 FROM releases WHERE id = 'r1'");
        assert!(has_release, "release should exist with parent present");

        ffi::sqlite3_close(db_source);
        ffi::sqlite3_close(db_target);
    }
}

/// Test that a single changeset containing both parent and child rows
/// (from a single session) applies correctly -- the session extension
/// records operations in execution order, so parent comes before child.
#[test]
fn single_changeset_with_fk_deps_applies_correctly() {
    unsafe {
        let db_source = open_memory_db();
        create_synced_schema(db_source);

        // One session captures both parent and child
        let session = SyncSession::start(db_source).expect("session");
        exec(
            db_source,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Kind of Blue', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db_source,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db_source,
            "INSERT INTO tracks (id, release_id, title, _updated_at, created_at) VALUES ('t1', 'r1', 'So What', '0000000001000-0000-dev1', '2026-01-01')",
        );

        let cs = session.changeset().expect("changeset");
        assert!(cs.is_some());
        let cs = cs.unwrap();

        let db_target = open_memory_db();
        create_synced_schema(db_target);

        apply_changeset_lww(db_target, &cs).expect("apply");

        let has_album = row_exists(db_target, "SELECT 1 FROM albums WHERE id = 'al1'");
        assert!(has_album, "album should exist");

        let has_release = row_exists(db_target, "SELECT 1 FROM releases WHERE id = 'r1'");
        assert!(has_release, "release should exist");

        let has_track = row_exists(db_target, "SELECT 1 FROM tracks WHERE id = 't1'");
        assert!(has_track, "track should exist");

        ffi::sqlite3_close(db_source);
        ffi::sqlite3_close(db_target);
    }
}

// ---- Session isolation: applying incoming doesn't contaminate outgoing ----

#[test]
fn applying_incoming_without_session_does_not_contaminate_outgoing() {
    unsafe {
        let db = open_memory_db();
        create_synced_schema(db);

        // Phase 1: start a session and make a local change
        let session = SyncSession::start(db).expect("start");
        exec(
            db,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('local1', 'Local Artist', '0000000001000-0000-dev1', '2026-01-01')",
        );

        // Grab the outgoing changeset
        let outgoing = session.changeset().expect("changeset");
        assert!(outgoing.is_some(), "should have local changes");

        // End the session
        drop(session);

        // Phase 2: create an incoming changeset from another device
        let db_remote = open_memory_db();
        create_synced_schema(db_remote);
        let remote_session = Session::new(db_remote).expect("remote session");
        remote_session.attach(Some("artists")).expect("attach");
        exec(
            db_remote,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('remote1', 'Remote Artist', '0000000002000-0000-dev2', '2026-01-01')",
        );
        let incoming = remote_session.changeset().expect("changeset");
        drop(remote_session);

        // Apply incoming to our DB (NO session active)
        apply_changeset_lww(db, &incoming).expect("apply incoming");

        // Verify the remote data arrived
        let remote_name = query_text(db, "SELECT name FROM artists WHERE id = 'remote1'");
        assert_eq!(remote_name, "Remote Artist");

        // Phase 3: start a NEW session
        let session2 = SyncSession::start(db).expect("start new session");

        // Make another local change
        exec(
            db,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('local2', 'Another Local', '0000000003000-0000-dev1', '2026-01-01')",
        );

        let outgoing2 = session2.changeset().expect("changeset");
        assert!(outgoing2.is_some(), "should have new local changes");
        let outgoing2 = outgoing2.unwrap();

        // Apply outgoing2 to a fresh DB to verify it only contains local2
        let db_verify = open_memory_db();
        create_synced_schema(db_verify);
        apply_changeset_lww(db_verify, &outgoing2).expect("apply outgoing2");

        // Should have local2 but NOT remote1
        let has_local2 = row_exists(db_verify, "SELECT 1 FROM artists WHERE id = 'local2'");
        assert!(has_local2, "outgoing should contain local2");

        let has_remote1 = row_exists(db_verify, "SELECT 1 FROM artists WHERE id = 'remote1'");
        assert!(
            !has_remote1,
            "outgoing should NOT contain remote1 (applied without session)"
        );

        drop(session2);
        ffi::sqlite3_close(db);
        ffi::sqlite3_close(db_remote);
        ffi::sqlite3_close(db_verify);
    }
}

// ---- CONFLICT type (INSERT with existing PK): LWW on _updated_at ----

#[test]
fn insert_conflict_newer_wins() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        let session = Session::new(db1).expect("session");
        session.attach(Some("artists")).expect("attach");

        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Dev1 Version', '0000000003000-0000-dev1', '2026-01-01')",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Device 2 already has the same PK with an OLDER timestamp
        let db2 = open_memory_db();
        create_synced_schema(db2);
        exec(
            db2,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Dev2 Version', '0000000001000-0000-dev2', '2026-01-01')",
        );

        apply_changeset_lww(db2, &cs).expect("apply");

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "Dev1 Version", "newer incoming INSERT should win");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

// ---- Changeset from_bytes roundtrip ----

#[test]
fn changeset_from_bytes_roundtrip() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        let session = Session::new(db1).expect("session");
        session.attach(Some("artists")).expect("attach");
        exec(
            db1,
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'hello', '0000000001000-0000-dev1', '2026-01-01')",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Serialize and deserialize
        let bytes = cs.as_bytes().to_vec();
        let cs2 = Changeset::from_bytes(&bytes);

        let db2 = open_memory_db();
        create_synced_schema(db2);
        apply_changeset_lww(db2, &cs2).expect("apply");

        let name = query_text(db2, "SELECT name FROM artists WHERE id = 'a1'");
        assert_eq!(name, "hello");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

// ---- release_files DATA conflict preserves device-specific columns ----

#[test]
fn release_files_data_conflict_preserves_source_path_and_nonce() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        // Set up parent rows so FK constraints are satisfied
        exec(
            db1,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Album', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db1,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db1,
            "INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, source_path, encryption_nonce, _updated_at, created_at) VALUES ('f1', 'r1', 'track01.flac', 50000, 'audio/flac', '/dev1/music/track01.flac', X'AABBCCDD', '0000000001000-0000-dev1', '2026-01-01')",
        );

        // Dev1 updates the file metadata with a NEWER timestamp
        let session = Session::new(db1).expect("session");
        session.attach(Some("release_files")).expect("attach");
        exec(
            db1,
            "UPDATE release_files SET original_filename = 'track01_renamed.flac', source_path = '/dev1/music/track01_renamed.flac', encryption_nonce = X'11223344', _updated_at = '0000000003000-0000-dev1' WHERE id = 'f1'",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Dev2 has the same row with an OLDER timestamp but its own device-specific paths
        let db2 = open_memory_db();
        create_synced_schema(db2);
        exec(
            db2,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Album', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db2,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db2,
            "INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, source_path, encryption_nonce, _updated_at, created_at) VALUES ('f1', 'r1', 'track01.flac', 50000, 'audio/flac', '/dev2/library/track01.flac', X'DEADBEEF', '0000000002000-0000-dev2', '2026-01-01')",
        );

        apply_changeset_lww(db2, &cs).expect("apply");

        // Shared metadata should come from incoming (dev1 wins by timestamp)
        let filename = query_text(
            db2,
            "SELECT original_filename FROM release_files WHERE id = 'f1'",
        );
        assert_eq!(
            filename, "track01_renamed.flac",
            "incoming shared metadata should win"
        );

        // Device-specific columns should be preserved from local (dev2)
        let source_path = query_text(db2, "SELECT source_path FROM release_files WHERE id = 'f1'");
        assert_eq!(
            source_path, "/dev2/library/track01.flac",
            "local source_path must be preserved"
        );

        let nonce = query_text(
            db2,
            "SELECT hex(encryption_nonce) FROM release_files WHERE id = 'f1'",
        );
        assert_eq!(
            nonce, "DEADBEEF",
            "local encryption_nonce must be preserved"
        );

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

#[test]
fn release_files_data_conflict_local_wins_no_restore_needed() {
    unsafe {
        let db1 = open_memory_db();
        create_synced_schema(db1);

        exec(
            db1,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Album', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db1,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db1,
            "INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, source_path, encryption_nonce, _updated_at, created_at) VALUES ('f1', 'r1', 'track01.flac', 50000, 'audio/flac', '/dev1/path', X'AABB', '0000000001000-0000-dev1', '2026-01-01')",
        );

        // Dev1 updates with an OLDER timestamp
        let session = Session::new(db1).expect("session");
        session.attach(Some("release_files")).expect("attach");
        exec(
            db1,
            "UPDATE release_files SET original_filename = 'old_name.flac', source_path = '/dev1/old', _updated_at = '0000000002000-0000-dev1' WHERE id = 'f1'",
        );
        let cs = session.changeset().expect("changeset");
        drop(session);

        // Dev2 has a NEWER timestamp -- local wins, no restore needed
        let db2 = open_memory_db();
        create_synced_schema(db2);
        exec(
            db2,
            "INSERT INTO albums (id, title, _updated_at, created_at) VALUES ('al1', 'Album', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db2,
            "INSERT INTO releases (id, album_id, import_status, _updated_at, created_at) VALUES ('r1', 'al1', 'complete', '0000000001000-0000-dev1', '2026-01-01')",
        );
        exec(
            db2,
            "INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, source_path, encryption_nonce, _updated_at, created_at) VALUES ('f1', 'r1', 'track01.flac', 50000, 'audio/flac', '/dev2/path', X'CCDD', '0000000005000-0000-dev2', '2026-01-01')",
        );

        apply_changeset_lww(db2, &cs).expect("apply");

        // Local should win entirely -- both shared and device-specific columns untouched
        let filename = query_text(
            db2,
            "SELECT original_filename FROM release_files WHERE id = 'f1'",
        );
        assert_eq!(filename, "track01.flac", "local should win");

        let source_path = query_text(db2, "SELECT source_path FROM release_files WHERE id = 'f1'");
        assert_eq!(source_path, "/dev2/path", "local source_path preserved");

        ffi::sqlite3_close(db1);
        ffi::sqlite3_close(db2);
    }
}

// ---- Verify the SYNCED_TABLES constant ----

#[test]
fn synced_tables_constant_has_correct_count() {
    assert_eq!(SYNCED_TABLES.len(), 11);
    assert!(SYNCED_TABLES.contains(&"artists"));
    assert!(SYNCED_TABLES.contains(&"albums"));
    assert!(SYNCED_TABLES.contains(&"album_discogs"));
    assert!(SYNCED_TABLES.contains(&"album_musicbrainz"));
    assert!(SYNCED_TABLES.contains(&"album_artists"));
    assert!(SYNCED_TABLES.contains(&"releases"));
    assert!(SYNCED_TABLES.contains(&"tracks"));
    assert!(SYNCED_TABLES.contains(&"track_artists"));
    assert!(SYNCED_TABLES.contains(&"release_files"));
    assert!(SYNCED_TABLES.contains(&"audio_formats"));
    assert!(SYNCED_TABLES.contains(&"library_images"));

    // Non-synced tables must NOT be included
    assert!(!SYNCED_TABLES.contains(&"storage_profiles"));
    assert!(!SYNCED_TABLES.contains(&"release_storage"));
    assert!(!SYNCED_TABLES.contains(&"torrents"));
    assert!(!SYNCED_TABLES.contains(&"torrent_piece_mappings"));
    assert!(!SYNCED_TABLES.contains(&"imports"));
}
