/// Safe-ish wrappers around the SQLite session extension FFI.
///
/// These operate on raw `*mut sqlite3` pointers so they can be used with
/// sqlx's `LockedSqliteHandle::as_raw_handle()`. This is a spike module --
/// the API will evolve as we build the production sync system.
use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;

use libsqlite3_sys as ffi;

/// A recorded binary changeset from a session.
pub struct Changeset {
    buf: *mut c_void,
    len: c_int,
}

impl Changeset {
    pub fn as_bytes(&self) -> &[u8] {
        if self.buf.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.buf as *const u8, self.len as usize) }
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Drop for Changeset {
    fn drop(&mut self) {
        if !self.buf.is_null() {
            unsafe { ffi::sqlite3_free(self.buf) };
        }
    }
}

/// Action a conflict handler can return.
#[repr(i32)]
pub enum ConflictAction {
    Omit = ffi::SQLITE_CHANGESET_OMIT,
    Replace = ffi::SQLITE_CHANGESET_REPLACE,
    Abort = ffi::SQLITE_CHANGESET_ABORT,
}

/// The type of conflict reported to the conflict handler.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    Data = ffi::SQLITE_CHANGESET_DATA,
    NotFound = ffi::SQLITE_CHANGESET_NOTFOUND,
    Conflict = ffi::SQLITE_CHANGESET_CONFLICT,
    Constraint = ffi::SQLITE_CHANGESET_CONSTRAINT,
    ForeignKey = ffi::SQLITE_CHANGESET_FOREIGN_KEY,
}

impl ConflictType {
    fn from_raw(val: c_int) -> Self {
        match val {
            ffi::SQLITE_CHANGESET_DATA => ConflictType::Data,
            ffi::SQLITE_CHANGESET_NOTFOUND => ConflictType::NotFound,
            ffi::SQLITE_CHANGESET_CONFLICT => ConflictType::Conflict,
            ffi::SQLITE_CHANGESET_CONSTRAINT => ConflictType::Constraint,
            ffi::SQLITE_CHANGESET_FOREIGN_KEY => ConflictType::ForeignKey,
            other => panic!("unknown conflict type: {other}"),
        }
    }
}

/// A session that tracks changes to a database.
///
/// Wraps `sqlite3_session*`. Must be created and used on the same connection,
/// and must be deleted before the connection is closed.
pub struct Session {
    raw: *mut ffi::sqlite3_session,
}

impl Session {
    /// Create a new session on the given database connection, tracking
    /// the "main" database.
    ///
    /// # Safety
    /// `db` must be a valid, open sqlite3 connection pointer.
    pub unsafe fn new(db: *mut ffi::sqlite3) -> Result<Self, i32> {
        let db_name = CString::new("main").unwrap();
        let mut raw: *mut ffi::sqlite3_session = ptr::null_mut();
        let rc = ffi::sqlite3session_create(db, db_name.as_ptr(), &mut raw);
        if rc != ffi::SQLITE_OK as c_int {
            return Err(rc);
        }
        Ok(Session { raw })
    }

    /// Attach a specific table to the session, or all tables if `table` is None.
    pub fn attach(&self, table: Option<&str>) -> Result<(), i32> {
        let c_table: Option<CString> = table.map(|t| CString::new(t).unwrap());
        let ptr: *const c_char = c_table.as_ref().map(|c| c.as_ptr()).unwrap_or(ptr::null());
        let rc = unsafe { ffi::sqlite3session_attach(self.raw, ptr) };
        if rc != ffi::SQLITE_OK as c_int {
            return Err(rc);
        }
        Ok(())
    }

    /// Extract the binary changeset recorded by this session.
    pub fn changeset(&self) -> Result<Changeset, i32> {
        let mut len: c_int = 0;
        let mut buf: *mut c_void = ptr::null_mut();
        let rc = unsafe { ffi::sqlite3session_changeset(self.raw, &mut len, &mut buf) };
        if rc != ffi::SQLITE_OK as c_int {
            return Err(rc);
        }
        Ok(Changeset { buf, len })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3session_delete(self.raw) };
    }
}

/// Apply a changeset to a database connection.
///
/// `conflict_handler` is called for each conflict. It receives the conflict type
/// and should return a `ConflictAction`.
///
/// # Safety
/// `db` must be a valid, open sqlite3 connection pointer.
pub unsafe fn apply_changeset<F>(
    db: *mut ffi::sqlite3,
    changeset: &Changeset,
    mut conflict_handler: F,
) -> Result<(), i32>
where
    F: FnMut(ConflictType) -> ConflictAction,
{
    unsafe extern "C" fn filter_cb(_ctx: *mut c_void, _table: *const c_char) -> c_int {
        // Accept all tables
        1
    }

    unsafe extern "C" fn conflict_cb<F>(
        ctx: *mut c_void,
        conflict_type: c_int,
        _iter: *mut ffi::sqlite3_changeset_iter,
    ) -> c_int
    where
        F: FnMut(ConflictType) -> ConflictAction,
    {
        let handler = &mut *(ctx as *mut F);
        let ct = ConflictType::from_raw(conflict_type);
        handler(ct) as c_int
    }

    let rc = ffi::sqlite3changeset_apply(
        db,
        changeset.len,
        changeset.buf,
        Some(filter_cb),
        Some(conflict_cb::<F>),
        &mut conflict_handler as *mut F as *mut c_void,
    );

    if rc != ffi::SQLITE_OK as c_int {
        return Err(rc);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: open an in-memory sqlite3 database via libsqlite3-sys directly.
    unsafe fn open_memory_db() -> *mut ffi::sqlite3 {
        let mut db: *mut ffi::sqlite3 = ptr::null_mut();
        let rc = ffi::sqlite3_open(c":memory:".as_ptr(), &mut db);
        assert_eq!(rc, ffi::SQLITE_OK as c_int, "Failed to open in-memory DB");
        db
    }

    /// Helper: execute a SQL statement on a raw connection.
    unsafe fn exec(db: *mut ffi::sqlite3, sql: &str) {
        let c_sql = CString::new(sql).unwrap();
        let rc = ffi::sqlite3_exec(db, c_sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut());
        assert_eq!(rc, ffi::SQLITE_OK as c_int, "exec failed for: {sql}");
    }

    /// Helper: query a single integer value.
    unsafe fn query_int(db: *mut ffi::sqlite3, sql: &str) -> i64 {
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

    /// Helper: query a single text value.
    unsafe fn query_text(db: *mut ffi::sqlite3, sql: &str) -> String {
        let c_sql = CString::new(sql).unwrap();
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        let rc = ffi::sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, ptr::null_mut());
        assert_eq!(rc, ffi::SQLITE_OK as c_int, "prepare failed for: {sql}");

        let step = ffi::sqlite3_step(stmt);
        assert_eq!(step, ffi::SQLITE_ROW as c_int, "expected a row for: {sql}");

        let ptr = ffi::sqlite3_column_text(stmt, 0);
        let val = std::ffi::CStr::from_ptr(ptr as *const c_char)
            .to_string_lossy()
            .into_owned();
        ffi::sqlite3_finalize(stmt);
        val
    }

    #[test]
    fn test_basic_changeset_capture() {
        unsafe {
            let db = open_memory_db();
            exec(db, "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)");

            let session = Session::new(db).expect("session create");
            session.attach(Some("items")).expect("attach");

            exec(db, "INSERT INTO items VALUES (1, 'hello')");

            let cs = session.changeset().expect("changeset");
            assert!(!cs.is_empty(), "changeset should not be empty");

            drop(session);
            ffi::sqlite3_close(db);
        }
    }

    #[test]
    fn test_changeset_application() {
        unsafe {
            // DB1: record changes
            let db1 = open_memory_db();
            exec(
                db1,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            );

            let session = Session::new(db1).expect("session create");
            session.attach(Some("items")).expect("attach");

            exec(db1, "INSERT INTO items VALUES (1, 'alpha')");
            exec(db1, "INSERT INTO items VALUES (2, 'beta')");

            let cs = session.changeset().expect("changeset");
            drop(session);

            // DB2: apply changeset
            let db2 = open_memory_db();
            exec(
                db2,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
            );

            apply_changeset(db2, &cs, |_conflict_type| ConflictAction::Abort)
                .expect("apply changeset");

            let count = query_int(db2, "SELECT COUNT(*) FROM items");
            assert_eq!(count, 2, "DB2 should have 2 rows");

            let name = query_text(db2, "SELECT name FROM items WHERE id = 1");
            assert_eq!(name, "alpha");

            let name = query_text(db2, "SELECT name FROM items WHERE id = 2");
            assert_eq!(name, "beta");

            ffi::sqlite3_close(db1);
            ffi::sqlite3_close(db2);
        }
    }

    #[test]
    fn test_conflict_handler() {
        unsafe {
            // DB1: insert a row then update it (session captures the update)
            let db1 = open_memory_db();
            exec(
                db1,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, updated_at TEXT)",
            );
            exec(
                db1,
                "INSERT INTO items VALUES (1, 'original', '2026-01-01T00:00:00Z')",
            );

            let session = Session::new(db1).expect("session create");
            session.attach(Some("items")).expect("attach");

            // Update the row -- session captures this as a change
            exec(
                db1,
                "UPDATE items SET name = 'from_db1', updated_at = '2026-01-03T00:00:00Z' WHERE id = 1",
            );

            let cs = session.changeset().expect("changeset");
            assert!(!cs.is_empty());
            drop(session);

            // DB2: has the same row but with a different updated_at
            let db2 = open_memory_db();
            exec(
                db2,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, updated_at TEXT)",
            );
            exec(
                db2,
                "INSERT INTO items VALUES (1, 'from_db2', '2026-01-02T00:00:00Z')",
            );

            // Apply changeset to DB2 -- this should trigger a DATA conflict
            // because the row exists with different values than the changeset's
            // "old" values.
            let mut conflict_called = false;
            let mut conflict_type_seen = None;

            apply_changeset(db2, &cs, |ct| {
                conflict_called = true;
                conflict_type_seen = Some(ct);
                // REPLACE: let the incoming changeset win
                ConflictAction::Replace
            })
            .expect("apply changeset");

            assert!(conflict_called, "conflict handler should have been called");
            assert_eq!(
                conflict_type_seen,
                Some(ConflictType::Data),
                "should be a DATA conflict"
            );

            // With REPLACE, the incoming changeset (db1's update) should win
            let name = query_text(db2, "SELECT name FROM items WHERE id = 1");
            assert_eq!(name, "from_db1", "incoming changeset should win");

            let updated = query_text(db2, "SELECT updated_at FROM items WHERE id = 1");
            assert_eq!(updated, "2026-01-03T00:00:00Z");

            ffi::sqlite3_close(db1);
            ffi::sqlite3_close(db2);
        }
    }

    #[test]
    fn test_conflict_handler_omit() {
        unsafe {
            // Same setup as above but the conflict handler returns OMIT (local wins)
            let db1 = open_memory_db();
            exec(
                db1,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, updated_at TEXT)",
            );
            exec(
                db1,
                "INSERT INTO items VALUES (1, 'original', '2026-01-01T00:00:00Z')",
            );

            let session = Session::new(db1).expect("session create");
            session.attach(Some("items")).expect("attach");
            exec(
                db1,
                "UPDATE items SET name = 'from_db1', updated_at = '2026-01-02T00:00:00Z' WHERE id = 1",
            );

            let cs = session.changeset().expect("changeset");
            drop(session);

            let db2 = open_memory_db();
            exec(
                db2,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, updated_at TEXT)",
            );
            exec(
                db2,
                "INSERT INTO items VALUES (1, 'from_db2', '2026-01-05T00:00:00Z')",
            );

            apply_changeset(db2, &cs, |_ct| {
                // OMIT: keep local version
                ConflictAction::Omit
            })
            .expect("apply changeset");

            // Local (db2) should still have its own values
            let name = query_text(db2, "SELECT name FROM items WHERE id = 1");
            assert_eq!(name, "from_db2", "local data should be preserved with OMIT");

            ffi::sqlite3_close(db1);
            ffi::sqlite3_close(db2);
        }
    }

    #[test]
    fn test_attach_all_tables() {
        unsafe {
            let db = open_memory_db();
            exec(db, "CREATE TABLE t1 (id INTEGER PRIMARY KEY, val TEXT)");
            exec(db, "CREATE TABLE t2 (id INTEGER PRIMARY KEY, val TEXT)");

            let session = Session::new(db).expect("session create");
            // Attach all tables by passing None
            session.attach(None).expect("attach all");

            exec(db, "INSERT INTO t1 VALUES (1, 'a')");
            exec(db, "INSERT INTO t2 VALUES (1, 'b')");

            let cs = session.changeset().expect("changeset");
            assert!(
                !cs.is_empty(),
                "changeset should capture changes from both tables"
            );

            // Apply to a second DB
            let db2 = open_memory_db();
            exec(db2, "CREATE TABLE t1 (id INTEGER PRIMARY KEY, val TEXT)");
            exec(db2, "CREATE TABLE t2 (id INTEGER PRIMARY KEY, val TEXT)");

            apply_changeset(db2, &cs, |_| ConflictAction::Abort).expect("apply");

            let v1 = query_text(db2, "SELECT val FROM t1 WHERE id = 1");
            assert_eq!(v1, "a");
            let v2 = query_text(db2, "SELECT val FROM t2 WHERE id = 1");
            assert_eq!(v2, "b");

            drop(session);
            ffi::sqlite3_close(db);
            ffi::sqlite3_close(db2);
        }
    }
}
