/// Pull changesets from the sync bucket and apply them to the local database.
///
/// Protocol:
/// 1. List heads from the sync bucket (one S3 LIST call).
/// 2. Compare each device's seq to our local `sync_cursors` table.
/// 3. For each device that's ahead of our cursor, fetch new changesets.
/// 4. Unpack envelope, check schema_version, apply with LWW.
/// 5. Update sync_cursors for that device.
///
/// The bucket client handles encryption/decryption, so pull works with
/// plaintext envelopes.
use std::collections::HashMap;

use tracing::{info, warn};

use super::apply::apply_changeset_lww;
use super::bucket::SyncBucketClient;
use super::envelope;
use super::session_ext::Changeset;

/// Current schema version. Changesets with a higher version are skipped.
pub const SCHEMA_VERSION: u32 = 2;

/// Summary of a pull operation.
#[derive(Debug)]
pub struct PullResult {
    /// Total changesets successfully applied.
    pub changesets_applied: u64,
    /// Number of distinct remote devices we pulled from.
    pub devices_pulled: u64,
    /// Changesets skipped due to schema version being newer than ours.
    pub skipped_schema: u64,
}

/// Pull and apply all new changesets from the sync bucket.
///
/// `db` is a raw sqlite3 connection pointer. The caller MUST ensure no
/// SyncSession is active -- the protocol requires ending the session before
/// pulling to avoid contaminating the next outgoing changeset.
///
/// `cursors` maps device_id -> last_seq we've applied from that device.
///
/// Returns the updated cursors map and a summary of what was applied.
///
/// # Safety
/// `db` must be a valid, open sqlite3 connection pointer.
pub async unsafe fn pull_changes(
    db: *mut libsqlite3_sys::sqlite3,
    bucket: &dyn SyncBucketClient,
    our_device_id: &str,
    cursors: &HashMap<String, u64>,
) -> Result<(HashMap<String, u64>, PullResult), PullError> {
    let heads = bucket.list_heads().await.map_err(PullError::Bucket)?;

    let mut updated_cursors = cursors.clone();
    let mut result = PullResult {
        changesets_applied: 0,
        devices_pulled: 0,
        skipped_schema: 0,
    };

    for head in &heads {
        // Skip our own device.
        if head.device_id == our_device_id {
            continue;
        }

        let local_seq = cursors.get(&head.device_id).copied().unwrap_or(0);
        if head.seq <= local_seq {
            continue;
        }

        info!(
            device_id = %head.device_id,
            local_seq,
            remote_seq = head.seq,
            "pulling changesets"
        );

        let mut pulled_any = false;

        for seq in (local_seq + 1)..=head.seq {
            let decrypted = match bucket.get_changeset(&head.device_id, seq).await {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        device_id = %head.device_id,
                        seq,
                        error = %e,
                        "failed to fetch changeset, stopping pull for this device"
                    );
                    break;
                }
            };

            let (env, changeset_bytes) =
                envelope::unpack(&decrypted).ok_or(PullError::InvalidEnvelope)?;

            // Schema version check: skip changesets from a newer schema.
            if env.schema_version > SCHEMA_VERSION {
                warn!(
                    device_id = %head.device_id,
                    seq,
                    remote_version = env.schema_version,
                    local_version = SCHEMA_VERSION,
                    "skipping changeset with newer schema version"
                );
                result.skipped_schema += 1;
                // Advance cursor past this seq so we don't re-fetch it.
                // When we upgrade, a new snapshot will reconcile.
                updated_cursors.insert(head.device_id.clone(), seq);
                continue;
            }

            if changeset_bytes.is_empty() {
                updated_cursors.insert(head.device_id.clone(), seq);
                continue;
            }

            let cs = Changeset::from_bytes(&changeset_bytes);
            apply_changeset_lww(db, &cs).map_err(PullError::Apply)?;

            result.changesets_applied += 1;
            pulled_any = true;
            updated_cursors.insert(head.device_id.clone(), seq);
        }

        if pulled_any {
            result.devices_pulled += 1;
        }
    }

    Ok((updated_cursors, result))
}

#[derive(Debug)]
pub enum PullError {
    Bucket(super::bucket::BucketError),
    InvalidEnvelope,
    Apply(super::session::SyncError),
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullError::Bucket(e) => write!(f, "bucket error: {e}"),
            PullError::InvalidEnvelope => write!(f, "invalid changeset envelope"),
            PullError::Apply(e) => write!(f, "changeset apply failed: {e}"),
        }
    }
}

impl std::error::Error for PullError {}
