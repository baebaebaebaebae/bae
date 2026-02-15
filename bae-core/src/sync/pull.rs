/// Pull changesets from the sync bucket and apply them to the local database.
///
/// Protocol:
/// 1. List heads from the sync bucket (one S3 LIST call).
/// 2. Compare each device's seq to our local `sync_cursors` table.
/// 3. For each device that's ahead of our cursor, fetch new changesets.
/// 4. Unpack envelope, check schema_version, apply with LWW.
/// 5. Update sync_cursors for that device.
///
/// After all changesets are applied, any that had FK constraint violations
/// are retried once -- the parent rows should now exist from other devices'
/// changesets applied in the same batch.
use std::collections::HashMap;

use tracing::{info, warn};

use super::apply::apply_changeset_lww;
use super::bucket::{DeviceHead, SyncBucketClient};
use super::changeset_scanner;
use super::envelope::{self, verify_changeset_signature};
use super::membership::MembershipChain;
use super::push::SCHEMA_VERSION;
use super::session_ext::Changeset;
use crate::library_dir::LibraryDir;

/// Summary of a pull operation.
#[derive(Debug)]
pub struct PullResult {
    /// Total changesets successfully applied.
    pub changesets_applied: u64,
    /// Number of distinct remote devices we pulled from.
    pub devices_pulled: u64,
    /// Changesets skipped due to schema version being newer than ours.
    pub skipped_schema: u64,
    /// All device heads fetched during this pull (including our own).
    /// Used by the sync status UI to show other devices' activity.
    pub remote_heads: Vec<DeviceHead>,
}

/// A changeset that had FK violations on first apply and needs retry.
struct DeferredChangeset {
    device_id: String,
    seq: u64,
    changeset: Changeset,
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
    membership_chain: Option<&MembershipChain>,
    library_dir: &LibraryDir,
) -> Result<(HashMap<String, u64>, PullResult), PullError> {
    // Check min_schema_version before processing any changesets.
    // If the bucket has a minimum that's higher than ours, refuse to sync.
    if let Some(min_version) = bucket
        .get_min_schema_version()
        .await
        .map_err(PullError::Bucket)?
    {
        if SCHEMA_VERSION < min_version {
            return Err(PullError::SchemaVersionTooOld {
                local_version: SCHEMA_VERSION,
                min_version,
            });
        }
    }

    let heads = bucket.list_heads().await.map_err(PullError::Bucket)?;

    let mut updated_cursors = cursors.clone();
    let mut result = PullResult {
        changesets_applied: 0,
        devices_pulled: 0,
        skipped_schema: 0,
        remote_heads: heads.clone(),
    };
    let mut deferred: Vec<DeferredChangeset> = Vec::new();

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
            // The bucket client returns already-decrypted bytes per its trait
            // contract. Implementations handle download + decryption internally.
            let envelope_bytes = match bucket.get_changeset(&head.device_id, seq).await {
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
                envelope::unpack(&envelope_bytes).ok_or(PullError::InvalidEnvelope)?;

            // Validate that changeset_size in the envelope matches the actual
            // bytes. A mismatch indicates corruption or a buggy encoder.
            if env.changeset_size != changeset_bytes.len() {
                warn!(
                    device_id = %head.device_id,
                    seq,
                    expected = env.changeset_size,
                    actual = changeset_bytes.len(),
                    "changeset_size mismatch in envelope"
                );
            }

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

            // Signature check: reject changesets with invalid signatures.
            if !verify_changeset_signature(&env, &changeset_bytes) {
                warn!(
                    device_id = %head.device_id,
                    seq,
                    "changeset has invalid signature, skipping"
                );
                updated_cursors.insert(head.device_id.clone(), seq);
                continue;
            }

            // Membership validation: if a chain exists, verify the author
            // was a member at the time the changeset was created.
            if let Some(chain) = membership_chain {
                if let Some(ref pk) = env.author_pubkey {
                    if !chain.is_member_at(pk, &env.timestamp) {
                        warn!(
                            device_id = %head.device_id,
                            seq,
                            author = %pk,
                            "changeset author not a member at timestamp, skipping"
                        );
                        updated_cursors.insert(head.device_id.clone(), seq);
                        continue;
                    }
                }

                // Unsigned changesets in a chain-enabled library: skip them
                // unless they predate the chain's first entry (grandfathered).
                if env.author_pubkey.is_none() {
                    let first_entry_ts = chain.entries().first().map(|e| e.timestamp.as_str());
                    if first_entry_ts.is_some_and(|ts| env.timestamp.as_str() >= ts) {
                        warn!(
                            device_id = %head.device_id,
                            seq,
                            "unsigned changeset after membership chain created, skipping"
                        );
                        updated_cursors.insert(head.device_id.clone(), seq);
                        continue;
                    }
                }
            }

            if changeset_bytes.is_empty() {
                updated_cursors.insert(head.device_id.clone(), seq);
                continue;
            }

            let cs = Changeset::from_bytes(&changeset_bytes);
            let apply_result = apply_changeset_lww(db, &cs).map_err(PullError::Apply)?;

            // Download any images referenced by this changeset.
            download_changeset_images(&changeset_bytes, bucket, library_dir).await;

            if apply_result.had_fk_violations {
                deferred.push(DeferredChangeset {
                    device_id: head.device_id.clone(),
                    seq,
                    changeset: Changeset::from_bytes(&changeset_bytes),
                });
            }

            result.changesets_applied += 1;
            pulled_any = true;
            updated_cursors.insert(head.device_id.clone(), seq);
        }

        if pulled_any {
            result.devices_pulled += 1;
        }
    }

    // Retry changesets that had FK constraint violations. After applying all
    // changesets from all devices, the parent rows should now exist.
    if !deferred.is_empty() {
        info!(
            count = deferred.len(),
            "retrying changesets with FK violations"
        );

        for d in &deferred {
            let retry_result = apply_changeset_lww(db, &d.changeset).map_err(PullError::Apply)?;

            if retry_result.had_fk_violations {
                warn!(
                    device_id = %d.device_id,
                    seq = d.seq,
                    "changeset still has FK violations after retry, skipping"
                );
            }
        }
    }

    Ok((updated_cursors, result))
}

/// Download images referenced by a changeset's library_images operations.
///
/// Scans the changeset for upserted image IDs and downloads any that don't
/// already exist locally. Failures are logged but do not fail the pull --
/// the image might not be in the bucket yet if push was partial, and the
/// next sync cycle will retry.
async fn download_changeset_images(
    changeset_bytes: &[u8],
    bucket: &dyn SyncBucketClient,
    library_dir: &LibraryDir,
) {
    let scan = match changeset_scanner::scan_changeset_for_images(changeset_bytes) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to scan changeset for images: {e}");
            return;
        }
    };

    for image_id in &scan.upserted_image_ids {
        let image_path = library_dir.image_path(image_id);
        if image_path.exists() {
            continue;
        }

        match bucket.download_image(image_id).await {
            Ok(bytes) => {
                if let Some(parent) = image_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        warn!(image_id, error = %e, "failed to create image directory");
                        continue;
                    }
                }

                if let Err(e) = std::fs::write(&image_path, bytes) {
                    warn!(image_id, error = %e, "failed to write image");
                }
            }
            Err(e) => {
                warn!(image_id, error = %e, "failed to download image");
            }
        }
    }
}

#[derive(Debug)]
pub enum PullError {
    Bucket(super::bucket::BucketError),
    InvalidEnvelope,
    Apply(super::session::SyncError),
    /// The sync bucket requires a schema version newer than ours.
    /// The client must upgrade before syncing.
    SchemaVersionTooOld {
        local_version: u32,
        min_version: u32,
    },
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullError::Bucket(e) => write!(f, "bucket error: {e}"),
            PullError::InvalidEnvelope => write!(f, "invalid changeset envelope"),
            PullError::Apply(e) => write!(f, "changeset apply failed: {e}"),
            PullError::SchemaVersionTooOld {
                local_version,
                min_version,
            } => write!(
                f,
                "local schema version {local_version} is below the bucket minimum {min_version}, upgrade required"
            ),
        }
    }
}

impl std::error::Error for PullError {}
