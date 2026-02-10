# Response to Review Round 1

Thorough review, genuinely helpful. Every point was checked against the codebase. Here's what changed and why.

---

## Critical issues

### 1. Table name: `files` not `release_files`

**Fixed.** The roadmap now correctly references `files.encryption_nonce` instead of `release_files.encryption_nonce`. Verified against `bae-core/migrations/001_initial.sql` line 94: the table is `CREATE TABLE files`.

### 2. Encryption nonce characterization

**Rewritten.** The "What exists today" section now explains that the nonce is embedded as the first 24 bytes of each encrypted blob (by `encrypt_chunked` at `encryption.rs` line 130), and that `files.encryption_nonce` is a **cached copy** for efficient range-request decryption (`decrypt_range_with_offset` at line 336). The nonce and encryption key are independent, so changing keys in Phase 3 does not affect nonce handling. This was a meaningful nuance to get right since the original phrasing implied a tighter coupling.

### 3. Transactional atomicity gap

**New dedicated section added: "Batch atomicity and ordering" in Phase 1c.**

This was the most important find in the review. The original merge pseudocode processed ops individually with no transaction boundary, which would leave partial state if a pull was interrupted mid-batch.

Changes:
- Added `batch_id` column to the `op_log` table to group ops from a single transaction.
- Specified that each S3 ops file is one batch (one transaction's worth of ops).
- Merge replay wraps each batch in a local SQLite transaction. Failure rolls back the entire batch.
- Intra-batch ordering is by original `seq` (insertion order), not by HLC. This respects FK dependencies: `insert_album_with_release_and_tracks` (client.rs line 515) inserts album first, then release, then tracks, and the ops are recorded in that order.
- If a batch fails due to missing parent rows (e.g., from another not-yet-processed batch), it's retried after other batches are processed.

### 4. OpRecorder sits between LibraryManager and Database

**Rewritten.** The architecture is now explicit:

```
LibraryManager
  -> OpRecorder (intercepts writes, records timestamps/ops)
    -> Database (executes raw SQL)
```

`LibraryManager` holds `OpRecorder` instead of `Database` directly. `OpRecorder` exposes a `database()` accessor for read-only queries, so `bae-server`, import code, and all read paths continue working unchanged. Only write paths change.

The write method count is corrected to ~33 (Database) + ~30 (LibraryManager) based on the actual grep results, not the original "~40" estimate.

### 5. device_id to user_pubkey migration (Phase 1 -> Phase 2)

**Resolved by eliminating the namespace change.** The original roadmap proposed switching from `heads/{device_id}` to `heads/{user_pubkey_hex}` in Phase 2. The review correctly identified this as a non-trivial S3 migration (S3 doesn't support renames).

The solution: **don't change the namespace.** Phase 2 keeps `heads/` and `ops/` keyed by `device_id`. A user may have multiple devices, and each device maintains its own op stream with its own sequence numbers. Authorship is established cryptographically via the `SignedOp` envelope (which includes `author_pubkey` and a signature), not via the S3 key path. This means:
- No S3 migration at all.
- Multi-device users naturally have separate op streams per device.
- Membership validation checks the `author_pubkey` in the signed op, not the path it was fetched from.
- The membership chain is stored under `membership/{author_pubkey_hex}/{seq}.enc` (new in Phase 2, no migration from Phase 1).

This is cleaner than the original design and eliminates the migration problem entirely.

---

## Design concerns

### 6. field_timestamps table size

**Acknowledged with justification.** Added a new section "Why a separate table instead of a JSON column on each tracked table?" that compares the two approaches and explains the decision.

The JSON column alternative (adding `field_hlcs TEXT` to each tracked table) was seriously considered. It has real advantages: co-located data, no join, one row per entity. But the downsides are substantial: widening 9 tables, maintaining JSON blobs in every write method, harder to index, and JSON extraction in SQLite is less ergonomic than flat column queries.

The ~400K rows from backfill are within SQLite's comfort zone (~40MB at ~100 bytes/row). Reads only happen during merge, not during UI rendering, so the performance cost is isolated to sync operations.

### 7. Compaction GC safety

**New GC policy added.** The roadmap now specifies: keep ops for 30 days after the checkpoint that includes them. This gives offline devices a month to come back online. After 30 days, covered ops are eligible for deletion. A device offline for more than 30 days re-bootstraps from the latest checkpoint.

Also added a manual "compact now" button concept for users who know all devices are online and want to reclaim space immediately.

The checkpoint metadata file (`checkpoints/{hlc_timestamp}.meta.enc`) now explicitly records cursor positions, making it unambiguous which ops a checkpoint covers.

### 8. Write lock mechanism

**Replaced with simpler concurrent edit detection.** The original advisory lock was over-engineered:
- Checking all `heads/` before every push adds latency and S3 cost.
- In Phase 2, a single user with two devices would always trigger the warning.
- A hard lock over S3 is impossible (as the roadmap already acknowledged).

The new approach: after each pull (which already fetches `heads/`), show a simple status: "Last synced: 2 minutes ago. Device X also synced 5 minutes ago." No pre-push lock check. No additional S3 calls. The system merges concurrent writes correctly regardless; the indicator is informational.

### 9. Image sync

**New dedicated "Image sync" section added to Phase 1.** Addresses all three gaps the review identified:

1. **Push reliability:** Images are uploaded before ops. An `image_sync_queue` table tracks pending uploads. If the process crashes between uploading the image and pushing the op, the op is re-pushed on the next sync cycle (it's still in `op_log WHERE pushed = FALSE`).

2. **Retry on push failure:** The `image_sync_queue` table tracks status. Pending images are retried on each sync cycle.

3. **Pull-side missing image detection:** On pull, for each `library_images` Insert/Update op in the incoming batch, check if the local file `images/ab/cd/{id}` exists. If not, queue a targeted GET. This is O(1) per incoming image op, not O(n) across all images. No full bucket LIST needed.

### 10. Membership chain concurrent modification

**Fundamental redesign.** The original single-file `membership.enc` is replaced with individual files: `membership/{author_pubkey_hex}/{seq}.enc`. Each membership entry is a separate S3 object.

Concurrent additions by two owners produce two separate files that both survive (S3 objects can't conflict if they have different keys). On read, the client downloads all files under `membership/`, orders by HLC, and validates:
- First entry (lowest HLC) must be Add + Owner, self-signed.
- Subsequent Add/Remove entries must be signed by someone who was already an Owner at that HLC.
- Concurrent additions of the same member are idempotent (membership is a set).

This eliminates the overwrite race entirely.

### 11. Per-library keypairs vs. global identity

**Changed to global.** The review made a compelling argument that per-library keypairs would fragment identity in Phase 4 (attestation trust) and complicate Phase 3 (cross-library sharing).

The keypair is now global, stored in non-library-namespaced keyring entries (`bae_user_signing_key`, `bae_user_public_key`). Libraries reference the user's public key in their membership chain. Display names are per-library. Revocation is per-library (remove the pubkey from that library's membership chain), not per-key. Key compromise is handled by generating a new keypair and re-joining libraries, which is a rare event.

### 12. HKDF parameter usage

**Fixed.** The KDF now follows RFC 5869 standard parameter convention:
- `salt`: random 32-byte value, generated once per library, stored in the keyring alongside the master key. Used in the Extract step.
- `info`: `"bae-release-v1:" + release_id`. Context string with a version prefix, used in the Expand step.

The version prefix ("v1:") allows changing the derivation scheme without silently producing different keys for the same release_id. The original roadmap used `"bae-release-key"` as the salt and the release_id as the info, which had the parameters in non-standard positions and lacked versioning.

### 13. ShareGrant S3 credential exposure

**Fixed.** S3 credentials are now inside the `wrapped_payload`, which is encrypted to the recipient's X25519 key via `crypto_box_seal`. The `ShareGrant` struct no longer has plaintext `s3_access_key` / `s3_secret_key` fields. A new `GrantPayload` struct holds the release key and optional S3 creds, and this entire payload is wrapped. Even if the grant blob is sent over email or paste, the credentials are encrypted to the specific recipient.

---

## Questions

### 14. Orphaned device_ids

**Addressed in Phase 0b.** Orphaned device_ids are explicitly stated as harmless. The old device's `heads/` and `ops/` entries are stale data that never advances. The 30-day compaction grace period (Phase 1d) will eventually allow their ops to be cleaned up alongside any checkpoint that covers them. No special cleanup mechanism needed.

### 15. bae-server in the op log world

**New section added: Phase 1g.** This was a genuine scope gap. The roadmap now explicitly describes three tiers of bae-server support:

1. **Phase 1 baseline (required):** bae-server downloads the latest checkpoint on boot (`checkpoints/{latest}.db.enc` instead of `library.db.enc`). Opens DB read-only as today. Minimal code change -- just pointing at a different S3 key.

2. **Optional `--refresh`:** After downloading the checkpoint, pull and replay ops since that checkpoint. Requires temporarily opening the DB in read-write mode for the replay, then switching to read-only for serving. Scoped as an enhancement, not a blocker.

3. **Full incremental refresh (deferred):** Background loop pulling new ops while the server is running. Deferred to post-Phase-1 since "restart to refresh" is acceptable for the server use case.

### 16. audio_formats syncing

**Changed to Yes.** The auditor is right that `audio_formats` contains expensive-to-recompute data (FLAC headers, seektables, byte offset calculations via ffmpeg). A device that pulls ops but doesn't have `audio_formats` data can't serve audio via Subsonic or play locally without a full re-scan -- and the re-scan requires having the actual audio files, which a new device may not have yet.

Since the data is deterministic (same files always produce the same audio_formats), syncing it is safe and purely additive. The `file_id` FK is nullable in the schema, so audio_formats rows can exist on a device before the actual file is transferred.

Added a paragraph in the "Which tables to track" section explaining the rationale.

### 17. Tombstone semantics

**Clarified.** The merge algorithm now explicitly states: "Tombstones are HLC-compared, not permanent. A delete with HLC T1 prevents resurrection from inserts with HLC < T1, but an insert with HLC > T1 (a legitimate re-creation after deletion) will succeed."

The testing section is corrected: "Insert with HLC earlier than a delete -- tombstone prevents resurrection. Insert with HLC later than a delete -- insert succeeds (legitimate re-creation)." This replaces the misleading "tombstone prevents resurrection" phrasing.

---

## Effort estimate revision

Phase 0 + Phase 1 increased from ~5-7 weeks to ~9-12 weeks. The additional scope comes from:
- Batch atomicity implementation and testing
- Image sync queue and reliability
- bae-server checkpoint mode
- GC safety with grace periods
- audio_formats syncing (more tables to intercept)

The total increased from ~18-27 weeks to ~21-30 weeks. The per-phase estimates are slightly padded compared to the originals, reflecting the additional complexity that surfaced during review.

---

## What didn't change

The review validated these decisions and I kept them as-is:
- Row-level LWW (vs. semantic ops)
- HLC with 24-hour future-clock protection
- libsodium FFI reuse (vs. adding `ed25519-dalek`)
- The table sync categorization (except audio_formats)
- The overall phasing and dependency graph
- Snapshot sync for local profiles, ops for cloud
