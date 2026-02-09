# Release deletion doesn't use deferred cleanup for cloud files

## Problem

When deleting a release, cloud file deletions that fail are logged and skipped. The DB records are then deleted, making the orphaned cloud files unrecoverable.

## What happens today

In `bae-core/src/library/manager.rs:416-450`, `delete_release()`:

1. Gets the storage profile and creates a storage reader
2. Loops through files, calling `storage.delete(source_path)` for each
3. If any delete fails, logs a warning and continues
4. Deletes the release from the DB (cascades to `files`, `tracks`, etc.)

After step 4, the DB no longer knows about the files. If step 2 failed for some files (S3 unreachable, network timeout, rate limit), those files remain in S3 with no record pointing to them.

## Contrast with transfer

The transfer service (`storage/transfer.rs`) handles this correctly. It writes old file paths to a `pending_deletions.json` manifest, and a cleanup service (`storage/cleanup.rs`) retries deletions later with exponential backoff.

## Fix

Use the same deferred deletion pattern:

1. Record files to be deleted in the pending deletions manifest
2. Delete the DB records
3. Let the cleanup service handle actual file deletion asynchronously

This way, if cloud deletion fails, it gets retried. The cleanup service already knows how to delete from both local and cloud storage.
