/// Production conflict handler for changeset application.
///
/// Uses row-level Last-Writer-Wins (LWW) based on the `_updated_at` column,
/// which contains HLC timestamps that sort lexicographically = causally.
///
/// Every synced table has `_updated_at` as its second-to-last column
/// (followed by `created_at`), so the index is always `column_count - 2`.
use super::session_ext::{ConflictAction, ConflictContext, ConflictType};
use tracing::warn;

/// Tracks tables that had CONSTRAINT conflicts (FK violations) so the
/// caller can retry after all other changes have been applied.
#[derive(Default)]
pub struct ConflictTracker {
    /// Tables that had at least one CONSTRAINT conflict omitted.
    pub had_constraint_conflict: bool,
}

impl ConflictTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The production conflict handler for `apply_changeset_with_context`.
///
/// Rules:
/// - **DATA** (same row, both sides edited): compare `_updated_at`. Newer wins.
/// - **NOTFOUND** (row deleted locally, incoming UPDATE): OMIT (delete wins).
/// - **CONFLICT** (row exists, incoming INSERT): compare `_updated_at`. Newer wins.
/// - **CONSTRAINT** (FK violation): OMIT and track for retry.
/// - **FOREIGN_KEY**: OMIT (deferred FK check failure, handled by retry).
pub fn lww_conflict_handler(
    conflict_type: ConflictType,
    ctx: &ConflictContext,
    tracker: &mut ConflictTracker,
) -> ConflictAction {
    match conflict_type {
        ConflictType::Data => {
            // Both sides modified the same row. Compare _updated_at timestamps.
            let updated_at_col = ctx.column_count() - 2;

            let incoming = ctx.new_value(updated_at_col);
            let local = ctx.conflict_value(updated_at_col);

            match (incoming.as_deref(), local.as_deref()) {
                (Some(inc), Some(loc)) if inc > loc => ConflictAction::Replace,
                (Some(_), Some(_)) => ConflictAction::Omit,
                // If either timestamp is missing, keep local (safe default).
                _ => {
                    warn!(
                        table = ctx.table_name(),
                        "DATA conflict without _updated_at values, keeping local"
                    );
                    ConflictAction::Omit
                }
            }
        }

        ConflictType::NotFound => {
            // Row was deleted locally, incoming changeset has an UPDATE.
            // Delete wins.
            ConflictAction::Omit
        }

        ConflictType::Conflict => {
            // Row already exists locally but incoming changeset has an INSERT
            // (duplicate PK). Compare _updated_at to decide which version wins.
            let updated_at_col = ctx.column_count() - 2;

            let incoming = ctx.new_value(updated_at_col);
            let local = ctx.conflict_value(updated_at_col);

            match (incoming.as_deref(), local.as_deref()) {
                (Some(inc), Some(loc)) if inc > loc => ConflictAction::Replace,
                (Some(_), Some(_)) => ConflictAction::Omit,
                _ => {
                    warn!(
                        table = ctx.table_name(),
                        "CONFLICT without _updated_at values, keeping local"
                    );
                    ConflictAction::Omit
                }
            }
        }

        ConflictType::Constraint => {
            // FK constraint violation. The referenced parent row hasn't been
            // applied yet. OMIT and retry after all other changes are in.
            tracker.had_constraint_conflict = true;
            ConflictAction::Omit
        }

        ConflictType::ForeignKey => {
            // Deferred FK check failure. Same treatment as Constraint.
            tracker.had_constraint_conflict = true;
            ConflictAction::Omit
        }
    }
}
