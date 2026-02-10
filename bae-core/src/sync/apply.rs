/// Apply a changeset to a database with the production conflict handler.
///
/// Within a single changeset, SQLite defers FK checks -- parent and child
/// rows in the same changeset are applied in recording order. Cross-changeset
/// FK dependencies are handled by applying changesets in seq order (parents
/// are always in earlier changesets than children).
///
/// If a FK violation remains after applying a changeset, the conflict handler
/// reports it via `FOREIGN_KEY` type and the tracker notes it for the caller.
use super::conflict::{lww_conflict_handler, ConflictTracker};
use super::session::SyncError;
use super::session_ext::{apply_changeset_with_context, Changeset};

/// Result of applying a changeset.
pub struct ApplyResult {
    /// True if any FK constraint violations were reported. The caller may
    /// want to retry this changeset after applying other changesets that
    /// contain the missing parent rows.
    pub had_fk_violations: bool,
}

/// Apply a changeset to the given database connection using LWW conflict
/// resolution.
///
/// # Safety
/// `db` must be a valid, open sqlite3 connection pointer.
pub unsafe fn apply_changeset_lww(
    db: *mut libsqlite3_sys::sqlite3,
    changeset: &Changeset,
) -> Result<ApplyResult, SyncError> {
    let mut tracker = ConflictTracker::new();

    apply_changeset_with_context(db, changeset, |ct, ctx| {
        lww_conflict_handler(ct, ctx, &mut tracker)
    })
    .map_err(SyncError::ChangesetApply)?;

    Ok(ApplyResult {
        had_fk_violations: tracker.had_constraint_conflict,
    })
}
