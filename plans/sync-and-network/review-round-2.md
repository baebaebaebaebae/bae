# Sync & Network Roadmap -- Review Round 2

## Verdict: Approve with Notes

The response addressed every point from Round 1. The critical issues (table name, transactional atomicity, OpRecorder layering, S3 namespace migration) are all resolved, and the fixes are reflected in the actual roadmap text, not just described in the response. The roadmap is now in a state where it could guide implementation.

There are two new issues introduced by the revisions (one significant, one minor) and a couple of loose ends worth noting. None are blockers.

---

## Points resolved

### Critical issues -- all five resolved

**1. Table name `files` vs `release_files`**: Fixed. Roadmap line 11 now correctly says `files.encryption_nonce`.

**2. Encryption nonce characterization**: Rewritten well. Line 11 now explains that the nonce is embedded as the first 24 bytes of the encrypted blob (confirmed against `encrypt_chunked` at `bae-core/src/encryption.rs` line 130: `let mut output = base_nonce.to_vec()`), and that `files.encryption_nonce` is a cached copy for range-request decryption. The sentence "The nonce and the encryption key are independent: changing the key (Phase 3) does not affect nonce handling" is a good addition -- it clarifies the Phase 3 interaction.

**3. Transactional atomicity**: This was the most important fix. The new "Batch atomicity and ordering" section (roadmap lines 285-291) is thorough:
- `batch_id` column added to `op_log` (line 221).
- Each S3 ops file = one batch = one transaction's worth of ops (line 214).
- Batch replay wrapped in a local SQLite transaction (line 287).
- Intra-batch ordering is by original `seq`, not HLC (line 289).
- Retry after other batches for missing parent rows (line 287-288).
- Merge pseudocode (lines 293-316) now shows `BEGIN TRANSACTION` / `COMMIT`.

Verified against codebase: `insert_album_with_release_and_tracks` at `bae-core/src/db/client.rs` line 521 does `let mut tx = self.pool.begin().await?`, then inserts album (line 522), then album_discogs/album_musicbrainz (lines 540-568), then release (line 570), then tracks. The claim that ops recorded in this order "naturally respects FK dependencies" is correct.

**4. OpRecorder placement**: The architecture diagram (roadmap lines 102-106) is now explicit: `LibraryManager -> OpRecorder -> Database`. The `database()` accessor for read-only queries (line 119) preserves backward compatibility for bae-server and import code. The write method counts are corrected to ~33 (Database) + ~30 (LibraryManager).

**5. device_id to user_pubkey migration**: Eliminated entirely by keeping `heads/` and `ops/` keyed by `device_id` in Phase 2 (roadmap lines 471). Authorship established via `author_pubkey` in the `SignedOp` envelope. This is cleaner than the original design. The membership chain at `membership/{author_pubkey_hex}/{seq}.enc` is new in Phase 2 so no migration is needed. Smart solution.

### Design concerns -- all eight resolved

**6. field_timestamps table size**: Acknowledged with a well-reasoned justification section (roadmap lines 44-53). The comparison of approaches is fair. The 40MB estimate and "reads only during merge" scoping are correct.

**7. Compaction GC safety**: 30-day grace period added (roadmap line 335). Checkpoint metadata includes cursor positions (line 326). Manual "compact now" button for users who know all devices are online (line 337). This is sensible.

**8. Write lock mechanism**: Replaced with informational "last synced" status derived from `heads/` data already fetched during pull (roadmap lines 349-356). No additional S3 calls. No pre-push checks. Much simpler. The note about Phase 2 multi-device correctly explains why advisory locks would be useless (line 356).

**9. Image sync**: New dedicated section (roadmap lines 253-272). Push reliability via `image_sync_queue` table. Images uploaded before ops. Pull-side missing image detection is O(1) per incoming image op, not O(n). The crash recovery logic is sound: if the process crashes between image upload and op push, the op is still in `op_log WHERE pushed = FALSE` and will be retried.

**10. Membership chain concurrent modification**: Redesigned as individual files (`membership/{author_pubkey_hex}/{seq}.enc`). Concurrent additions produce separate S3 objects that can't conflict. Merge-on-read with HLC ordering and validation rules (roadmap lines 473-477). This eliminates the overwrite race entirely.

**11. Per-library keypairs vs. global identity**: Changed to global. Keyring entries are `bae_user_signing_key` / `bae_user_public_key` (roadmap line 420), not library-namespaced. Display names are per-library. Revocation is per-library membership removal, not per-key (line 403). This is the right call.

**12. HKDF parameter usage**: Fixed. Salt is now random 32 bytes (HKDF Extract step), info is `"bae-release-v1:" + release_id` (HKDF Expand step). Version prefix enables future scheme changes (roadmap lines 563-567).

**13. ShareGrant S3 credential exposure**: Fixed. S3 credentials are now inside `wrapped_payload`, encrypted to the recipient's X25519 key via `crypto_box_seal` (roadmap lines 622-630). New `GrantPayload` struct holds both the release key and optional creds.

### Questions -- all four resolved

**14. Orphaned device_ids**: Addressed in Phase 0b (roadmap line 77). Stated as harmless. Cleaned up by GC after compaction grace period.

**15. bae-server in the op log world**: New Phase 1g section (roadmap lines 358-366). Three tiers: baseline checkpoint download, optional `--refresh` with op replay, deferred full incremental refresh. The note about temporarily opening DB in read-write mode for replay (line 364) is important and correctly identified.

**16. audio_formats syncing**: Changed to Yes (roadmap line 946). Rationale is correct -- the data is expensive to recompute and a device without it can't play music. "Same files always produce the same audio_formats" is accurate.

**17. Tombstone semantics**: Clarified at roadmap line 318: "Tombstones are HLC-compared, not permanent." Testing section updated (line 1002-1003) with both directions: earlier insert blocked, later insert succeeds.

---

## New issues introduced by the revisions

### N1. [Significant] HKDF salt distribution to library members is unspecified

Phase 3a (roadmap line 563-565) says:

> salt: a random 32-byte value, generated once per library and stored alongside the master key in the keyring.

Phase 2d (roadmap line 479) says:

> The library encryption key is wrapped (encrypted) to each member's X25519 public key using `crypto_box_seal`.

The problem: When Alice joins a library in Phase 2, she receives the library encryption key via `keys/{alice_pubkey}.enc`. When Phase 3 is deployed, she also needs the HKDF salt to derive release keys. But the key-wrapping payload in Phase 2 only contains the master key -- the salt is not mentioned.

This matters because without the salt, a member who joined pre-Phase-3 cannot derive release keys. And new Phase 3 members need both the master key AND the salt during the invitation flow.

Options:
1. Include the salt in the wrapped payload alongside the master key (bump the payload format).
2. Store the salt in the bucket as a non-secret value (it doesn't need to be secret per RFC 5869 -- salt provides "key separation" but is not a key itself). For example, `salt.enc` in the bucket root. But since the bucket is encrypted, this requires the master key to decrypt.
3. Derive the salt deterministically from the master key (e.g., `salt = HMAC-SHA256(master_key, "bae-hkdf-salt-v1")`). This avoids an extra storage artifact at the cost of slightly weaker HKDF security -- the salt should ideally be independent of the key. But since the master key is already high-entropy (32 random bytes), this is fine in practice.

Option 3 is the simplest and requires no changes to Phase 2's key distribution. Worth specifying either way, because Phase 3 currently silently assumes all members have access to a value that Phase 2 doesn't distribute.

### N2. [Minor] audio_formats sync has an FK ordering dependency on files

The roadmap now syncs `audio_formats` (line 946) and also syncs `files` partially (line 947-954). The `audio_formats` table has:

```sql
file_id TEXT REFERENCES files(id),   -- nullable FK
track_id TEXT NOT NULL UNIQUE,       -- non-null FK to tracks
```

(Verified at `bae-core/migrations/001_initial.sql` line 106-124.)

SQLite has FK enforcement enabled (`PRAGMA foreign_keys = ON`, confirmed at `bae-core/src/db/client.rs` line 31).

If `audio_formats` ops arrive with a non-null `file_id` referencing a `files` row that hasn't been synced yet (because `files` ops are in a different batch), the insert will fail with an FK violation. The batch-retry mechanism (roadmap line 287) should handle this -- the failed batch retries after other batches are processed. But it's worth calling out in the roadmap's "Which tables to track" section that `audio_formats` has FK dependencies on both `tracks` and `files`, and that batch ordering during pull must account for this.

Similarly, `tracks` depends on `releases`, which depends on `albums`. The existing batch atomicity section handles intra-batch FK ordering well, but cross-batch FK dependencies rely on the retry mechanism. If the roadmap's merge implementation processes batches in chronological order (which it should, since `seq` increases over time), this should naturally resolve -- parent rows are created in earlier batches than children. But it's worth making this explicit.

---

## Remaining loose ends (not issues, just notes)

### L1. The `image_sync_queue` table vs the existing image upload path

The roadmap adds an `image_sync_queue` table (line 263) but doesn't specify how it interacts with the existing image upload code path. Today, `MetadataReplicator` handles image uploads as part of its full-snapshot sync. When `MetadataReplicator` is reduced to local-profile-only (Phase 1e, line 341-346), the cloud image upload responsibility moves to the new `SyncService`. The `image_sync_queue` table is the bridge. This is implicit in the roadmap but could benefit from one sentence making it explicit: "The SyncService consumes the image_sync_queue; the MetadataReplicator no longer handles cloud image uploads."

### L2. Effort estimates seem reasonable now

The revised estimates (Phase 0+1: 9-12 weeks, total: 21-30 weeks) are more realistic than the original (5-7 / 18-27 weeks). The additional scope from batch atomicity, image sync, bae-server, and GC safety is correctly reflected. The 3-4 week estimate for Phase 0 (OpRecorder touching ~63 methods) is the one I'd most expect to slip, but it's within a reasonable margin.

### L3. Global keypair and KeyService architecture

The roadmap proposes global keyring entries (`bae_user_signing_key`, `bae_user_public_key`) that are NOT library-namespaced (line 420-421). Today, all `KeyService` entries use `self.account(base)` which appends `:{library_id}` (confirmed at `bae-core/src/keys.rs` line 39-41). The global keypair entries would need to bypass this namespacing. This is straightforward (add a method that doesn't call `account()`, or use the entry name directly), but the implementer should be aware that `KeyService` currently assumes all entries are library-scoped.

---

## Summary

The response was thorough and every fix landed in the actual roadmap. The most impactful changes were:

1. **Batch atomicity** (Critical #3) -- the merge pseudocode is now correct and the transaction boundaries are explicit.
2. **S3 namespace stability** (Critical #5) -- eliminating the migration by keeping `device_id`-keyed paths and using `SignedOp` for authorship was an elegant solution.
3. **Membership chain as individual files** (Design #10) -- this completely eliminates the concurrent-modification race.

The one significant new issue (HKDF salt distribution in Phase 3) is a real gap but it's in a later phase and has a clean fix (option 3: derive the salt from the master key). The audio_formats FK ordering note is minor -- the existing batch-retry mechanism handles it, it just needs a sentence of documentation.

The roadmap is ready to guide Phase 0 and Phase 1 implementation. Phase 2+ details will naturally get refined as earlier phases are built.
