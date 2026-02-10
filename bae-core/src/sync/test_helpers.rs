/// Shared test helpers for sync module tests.
///
/// These operate on raw sqlite3 connections via libsqlite3-sys.
use libsqlite3_sys as ffi;
use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

/// Open an in-memory sqlite3 database via libsqlite3-sys directly.
pub unsafe fn open_memory_db() -> *mut ffi::sqlite3 {
    let mut db: *mut ffi::sqlite3 = ptr::null_mut();
    let rc = ffi::sqlite3_open(c":memory:".as_ptr(), &mut db);
    assert_eq!(rc, ffi::SQLITE_OK as c_int, "Failed to open in-memory DB");
    db
}

/// Execute a SQL statement on a raw connection.
pub unsafe fn exec(db: *mut ffi::sqlite3, sql: &str) {
    let c_sql = CString::new(sql).unwrap();
    let rc = ffi::sqlite3_exec(db, c_sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut());
    assert_eq!(rc, ffi::SQLITE_OK as c_int, "exec failed for: {sql}");
}

/// Query a single integer value.
pub unsafe fn query_int(db: *mut ffi::sqlite3, sql: &str) -> i64 {
    let c_sql = CString::new(sql).unwrap();
    let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
    let rc = ffi::sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, ptr::null_mut());
    assert_eq!(rc, ffi::SQLITE_OK as c_int, "prepare failed for: {sql}");

    let step = ffi::sqlite3_step(stmt);
    assert_eq!(step, ffi::SQLITE_ROW as c_int, "expected a row for: {sql}");

    let val = ffi::sqlite3_column_int64(stmt, 0);
    ffi::sqlite3_finalize(stmt);
    val
}

/// Query a single text value.
pub unsafe fn query_text(db: *mut ffi::sqlite3, sql: &str) -> String {
    let c_sql = CString::new(sql).unwrap();
    let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
    let rc = ffi::sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, ptr::null_mut());
    assert_eq!(rc, ffi::SQLITE_OK as c_int, "prepare failed for: {sql}");

    let step = ffi::sqlite3_step(stmt);
    assert_eq!(step, ffi::SQLITE_ROW as c_int, "expected a row for: {sql}");

    let ptr = ffi::sqlite3_column_text(stmt, 0);
    let val = CStr::from_ptr(ptr as *const c_char)
        .to_string_lossy()
        .into_owned();
    ffi::sqlite3_finalize(stmt);
    val
}

/// Query whether a row exists.
pub unsafe fn row_exists(db: *mut ffi::sqlite3, sql: &str) -> bool {
    let c_sql = CString::new(sql).unwrap();
    let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
    let rc = ffi::sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, ptr::null_mut());
    assert_eq!(rc, ffi::SQLITE_OK as c_int, "prepare failed for: {sql}");
    let step = ffi::sqlite3_step(stmt);
    ffi::sqlite3_finalize(stmt);
    step == ffi::SQLITE_ROW as c_int
}

/// Create the full bae schema on a raw connection (synced tables + essential non-synced).
pub unsafe fn create_synced_schema(db: *mut ffi::sqlite3) {
    exec(db, "PRAGMA foreign_keys = ON");
    exec(
        db,
        "CREATE TABLE artists (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            sort_name TEXT,
            discogs_artist_id TEXT,
            bandcamp_artist_id TEXT,
            musicbrainz_artist_id TEXT,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
    );
    exec(
        db,
        "CREATE TABLE albums (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            year INTEGER,
            bandcamp_album_id TEXT,
            cover_release_id TEXT,
            cover_art_url TEXT,
            is_compilation BOOLEAN NOT NULL DEFAULT FALSE,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
    );
    exec(
        db,
        "CREATE TABLE album_discogs (
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL UNIQUE,
            discogs_master_id TEXT,
            discogs_release_id TEXT NOT NULL,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE album_musicbrainz (
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL UNIQUE,
            musicbrainz_release_group_id TEXT NOT NULL,
            musicbrainz_release_id TEXT NOT NULL,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE album_artists (
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL,
            artist_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
            FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
            UNIQUE(album_id, artist_id)
        )",
    );
    exec(
        db,
        "CREATE TABLE releases (
            id TEXT PRIMARY KEY,
            album_id TEXT NOT NULL,
            release_name TEXT,
            year INTEGER,
            discogs_release_id TEXT,
            bandcamp_release_id TEXT,
            format TEXT,
            label TEXT,
            catalog_number TEXT,
            country TEXT,
            barcode TEXT,
            import_status TEXT NOT NULL DEFAULT 'queued',
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE tracks (
            id TEXT PRIMARY KEY,
            release_id TEXT NOT NULL,
            title TEXT NOT NULL,
            disc_number INTEGER,
            track_number INTEGER,
            duration_ms INTEGER,
            discogs_position TEXT,
            import_status TEXT NOT NULL DEFAULT 'queued',
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE track_artists (
            id TEXT PRIMARY KEY,
            track_id TEXT NOT NULL,
            artist_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            role TEXT,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
            FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE release_files (
            id TEXT PRIMARY KEY,
            release_id TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            content_type TEXT NOT NULL,
            source_path TEXT,
            encryption_nonce BLOB,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE audio_formats (
            id TEXT PRIMARY KEY,
            track_id TEXT NOT NULL UNIQUE,
            content_type TEXT NOT NULL,
            flac_headers BLOB,
            needs_headers BOOLEAN NOT NULL DEFAULT FALSE,
            start_byte_offset INTEGER,
            end_byte_offset INTEGER,
            pregap_ms INTEGER,
            frame_offset_samples INTEGER,
            exact_sample_count INTEGER,
            sample_rate INTEGER NOT NULL,
            bits_per_sample INTEGER NOT NULL,
            seektable_json TEXT NOT NULL,
            audio_data_start INTEGER NOT NULL,
            file_id TEXT REFERENCES release_files(id),
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
        )",
    );
    exec(
        db,
        "CREATE TABLE library_images (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            content_type TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            width INTEGER,
            height INTEGER,
            source TEXT NOT NULL,
            source_url TEXT,
            _updated_at TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
    );
    exec(
        db,
        "CREATE TABLE storage_profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            location TEXT NOT NULL,
            location_path TEXT NOT NULL,
            encrypted BOOLEAN NOT NULL DEFAULT FALSE,
            is_default BOOLEAN NOT NULL DEFAULT FALSE,
            is_home BOOLEAN NOT NULL DEFAULT FALSE,
            cloud_bucket TEXT,
            cloud_region TEXT,
            cloud_endpoint TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    );
    exec(
        db,
        "CREATE TABLE release_storage (
            id TEXT PRIMARY KEY,
            release_id TEXT NOT NULL,
            storage_profile_id TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
            FOREIGN KEY (storage_profile_id) REFERENCES storage_profiles (id)
        )",
    );
}
