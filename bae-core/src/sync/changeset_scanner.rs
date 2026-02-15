/// Scan changeset bytes for `library_images` table operations.
///
/// Uses the SQLite changeset iterator API to walk through all operations in a
/// changeset and extract image IDs from inserts, updates, and deletes on the
/// `library_images` table. This lets the sync loop know which images need to
/// be uploaded (push) or downloaded (pull).
use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use libsqlite3_sys as ffi;

/// Result of scanning a changeset for image-related operations.
pub struct ChangesetImageScan {
    /// Image IDs that were inserted or updated (need upload on push, download on pull).
    pub upserted_image_ids: Vec<String>,
    /// Image IDs that were deleted.
    pub deleted_image_ids: Vec<String>,
}

/// Scan a changeset for `library_images` operations.
///
/// Iterates all operations in the changeset and collects image IDs from
/// INSERT/UPDATE (into `upserted_image_ids`) and DELETE (into `deleted_image_ids`).
/// Operations on other tables are ignored.
///
/// Returns empty lists for empty changesets.
pub fn scan_changeset_for_images(changeset_bytes: &[u8]) -> Result<ChangesetImageScan, String> {
    if changeset_bytes.is_empty() {
        return Ok(ChangesetImageScan {
            upserted_image_ids: Vec::new(),
            deleted_image_ids: Vec::new(),
        });
    }

    let mut upserted = Vec::new();
    let mut deleted = Vec::new();

    unsafe {
        let mut iter: *mut ffi::sqlite3_changeset_iter = ptr::null_mut();
        let rc = ffi::sqlite3changeset_start(
            &mut iter,
            changeset_bytes.len() as c_int,
            changeset_bytes.as_ptr() as *mut c_void,
        );
        if rc != ffi::SQLITE_OK as c_int {
            return Err(format!("sqlite3changeset_start failed (rc={rc})"));
        }

        loop {
            let step = ffi::sqlite3changeset_next(iter);
            if step == ffi::SQLITE_DONE as c_int {
                break;
            }
            if step != ffi::SQLITE_ROW as c_int {
                ffi::sqlite3changeset_finalize(iter);
                return Err(format!("sqlite3changeset_next failed (rc={step})"));
            }

            // Get table name and operation type.
            let mut table: *const c_char = ptr::null();
            let mut ncol: c_int = 0;
            let mut op: c_int = 0;
            let mut indirect: c_int = 0;
            ffi::sqlite3changeset_op(iter, &mut table, &mut ncol, &mut op, &mut indirect);

            let table_name = CStr::from_ptr(table).to_str().unwrap_or("");
            if table_name != "library_images" {
                continue;
            }

            // Column 0 is the `id` column (TEXT PRIMARY KEY).
            // INSERT: id is in new values (all columns are "new").
            // UPDATE: id is in old values (PK is always in "old"; new values
            //         only contain modified columns).
            // DELETE: id is in old values (all columns are "old").
            match op {
                ffi::SQLITE_INSERT => {
                    if let Some(id) = extract_new_value(iter, 0) {
                        upserted.push(id);
                    }
                }
                ffi::SQLITE_UPDATE => {
                    if let Some(id) = extract_old_value(iter, 0) {
                        upserted.push(id);
                    }
                }
                ffi::SQLITE_DELETE => {
                    if let Some(id) = extract_old_value(iter, 0) {
                        deleted.push(id);
                    }
                }
                _ => {}
            }
        }

        let rc = ffi::sqlite3changeset_finalize(iter);
        if rc != ffi::SQLITE_OK as c_int {
            return Err(format!("sqlite3changeset_finalize failed (rc={rc})"));
        }
    }

    Ok(ChangesetImageScan {
        upserted_image_ids: upserted,
        deleted_image_ids: deleted,
    })
}

/// Extract the "new" value for a column from the current changeset iterator position.
/// Used for INSERT (all columns are "new") and UPDATE (changed columns).
unsafe fn extract_new_value(iter: *mut ffi::sqlite3_changeset_iter, col: c_int) -> Option<String> {
    let mut val: *mut ffi::sqlite3_value = ptr::null_mut();
    let rc = ffi::sqlite3changeset_new(iter, col, &mut val);
    if rc != ffi::SQLITE_OK as c_int || val.is_null() {
        return None;
    }
    value_to_string(val)
}

/// Extract the "old" value for a column from the current changeset iterator position.
/// Used for DELETE (all columns are "old") and UPDATE (original values).
unsafe fn extract_old_value(iter: *mut ffi::sqlite3_changeset_iter, col: c_int) -> Option<String> {
    let mut val: *mut ffi::sqlite3_value = ptr::null_mut();
    let rc = ffi::sqlite3changeset_old(iter, col, &mut val);
    if rc != ffi::SQLITE_OK as c_int || val.is_null() {
        return None;
    }
    value_to_string(val)
}

/// Convert a sqlite3_value to a String, or None if NULL.
unsafe fn value_to_string(val: *mut ffi::sqlite3_value) -> Option<String> {
    let vtype = ffi::sqlite3_value_type(val);
    if vtype == ffi::SQLITE_NULL as c_int {
        return None;
    }
    let text = ffi::sqlite3_value_text(val);
    if text.is_null() {
        return None;
    }
    Some(
        CStr::from_ptr(text as *const c_char)
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::session_ext::Session;
    use crate::sync::test_helpers::*;

    /// Capture a changeset from an in-memory DB after executing SQL statements.
    unsafe fn make_changeset(db: *mut ffi::sqlite3, tables: &[&str], stmts: &[&str]) -> Vec<u8> {
        let session = Session::new(db).expect("session");
        for table in tables {
            session.attach(Some(table)).expect("attach");
        }
        for stmt in stmts {
            exec(db, stmt);
        }
        let cs = session.changeset().expect("changeset");
        let bytes = cs.as_bytes().to_vec();
        drop(session);
        bytes
    }

    fn create_images_table(db: *mut ffi::sqlite3) {
        unsafe {
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
        }
    }

    #[test]
    fn detects_insert() {
        unsafe {
            let db = open_memory_db();
            create_images_table(db);

            let cs = make_changeset(
                db,
                &["library_images"],
                &["INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                   VALUES ('img-001', 'cover', 'image/jpeg', 12345, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert_eq!(scan.upserted_image_ids, vec!["img-001"]);
            assert!(scan.deleted_image_ids.is_empty());

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn detects_multiple_inserts() {
        unsafe {
            let db = open_memory_db();
            create_images_table(db);

            let cs = make_changeset(
                db,
                &["library_images"],
                &[
                    "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                     VALUES ('img-001', 'cover', 'image/jpeg', 100, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                     VALUES ('img-002', 'cover', 'image/png', 200, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                ],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert_eq!(scan.upserted_image_ids.len(), 2);
            assert!(scan.upserted_image_ids.contains(&"img-001".to_string()));
            assert!(scan.upserted_image_ids.contains(&"img-002".to_string()));
            assert!(scan.deleted_image_ids.is_empty());

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn detects_update() {
        unsafe {
            let db = open_memory_db();
            create_images_table(db);

            // Insert first (outside the session).
            exec(
                db,
                "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                 VALUES ('img-001', 'cover', 'image/jpeg', 100, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            );

            // Update inside the session.
            let cs = make_changeset(
                db,
                &["library_images"],
                &["UPDATE library_images SET file_size = 999, _updated_at = '2026-01-02T00:00:00Z' WHERE id = 'img-001'"],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert_eq!(scan.upserted_image_ids, vec!["img-001"]);
            assert!(scan.deleted_image_ids.is_empty());

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn detects_delete() {
        unsafe {
            let db = open_memory_db();
            create_images_table(db);

            // Insert first (outside the session).
            exec(
                db,
                "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                 VALUES ('img-001', 'cover', 'image/jpeg', 100, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            );

            // Delete inside the session.
            let cs = make_changeset(
                db,
                &["library_images"],
                &["DELETE FROM library_images WHERE id = 'img-001'"],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert!(scan.upserted_image_ids.is_empty());
            assert_eq!(scan.deleted_image_ids, vec!["img-001"]);

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn ignores_other_tables() {
        unsafe {
            let db = open_memory_db();
            exec(
                db,
                "CREATE TABLE artists (id TEXT PRIMARY KEY, name TEXT NOT NULL, _updated_at TEXT NOT NULL, created_at TEXT NOT NULL)",
            );

            let cs = make_changeset(
                db,
                &["artists"],
                &["INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Miles', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert!(scan.upserted_image_ids.is_empty());
            assert!(scan.deleted_image_ids.is_empty());

            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn empty_changeset() {
        let scan = scan_changeset_for_images(&[]).expect("scan");
        assert!(scan.upserted_image_ids.is_empty());
        assert!(scan.deleted_image_ids.is_empty());
    }

    #[test]
    fn mixed_tables_only_picks_up_images() {
        unsafe {
            let db = open_memory_db();
            create_images_table(db);
            exec(
                db,
                "CREATE TABLE artists (id TEXT PRIMARY KEY, name TEXT NOT NULL, _updated_at TEXT NOT NULL, created_at TEXT NOT NULL)",
            );

            let cs = make_changeset(
                db,
                &["library_images", "artists"],
                &[
                    "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('a1', 'Miles', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    "INSERT INTO library_images (id, type, content_type, file_size, source, _updated_at, created_at) \
                     VALUES ('img-001', 'cover', 'image/jpeg', 100, 'local', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                ],
            );

            let scan = scan_changeset_for_images(&cs).expect("scan");
            assert_eq!(scan.upserted_image_ids, vec!["img-001"]);
            assert!(scan.deleted_image_ids.is_empty());

            ffi::sqlite3_close(db);
        }
    }
}
