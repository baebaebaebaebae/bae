# S3 credentials stored in plaintext SQLite

## Problem

Storage profile S3 credentials (`cloud_access_key`, `cloud_secret_key`) are stored as plaintext TEXT columns in the `storage_profiles` table (`001_initial.sql:173-174`), while the encryption master key and cloud sync credentials are properly stored in the OS keyring via `KeyService`.

## Why it's inconsistent

The encryption key protects file contents. The S3 credentials grant access to the bucket that holds those same encrypted files. Anyone who can read the plaintext `library.db` can extract the S3 credentials and access the bucket directly. The encryption key being in the keyring doesn't help much if the attacker also grabs the encrypted files from S3 and brute-forces or finds the key elsewhere.

The cloud sync credentials (`cloud_sync_access_key`, `cloud_sync_secret_key`) are stored in `KeyService` — same kind of S3 credentials, different treatment.

## Additional context

This becomes more relevant once the library DB is synced to cloud. The DB is encrypted during upload (`cloud_sync.rs` encrypts with XChaCha20-Poly1305), so the credentials are protected in transit and at rest in S3. But locally, the DB file is plaintext SQLite, so the credentials are readable by any process with file access.

## Options

1. **Move to KeyService** — store per-profile S3 credentials in the OS keyring, keyed by profile ID. The DB stores only non-sensitive profile metadata.
2. **Encrypt at rest** — less ideal since it's the same problem in a different form, and SQLCipher isn't implemented yet.
3. **Accept the risk** — document that local DB access implies S3 access. This might be reasonable given that the DB also contains the full music catalog, and local disk access already implies access to locally-stored files.
