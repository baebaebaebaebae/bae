# Profile deletion orphans releases

## Problem

Deleting a storage profile that still has releases linked to it leaves those releases in a broken state. Their files become inaccessible.

## What happens today

`delete_storage_profile()` in `bae-core/src/db/client.rs:1594` runs a bare `DELETE FROM storage_profiles WHERE id = ?`. It does not check whether any releases use the profile.

The foreign key on `release_storage.storage_profile_id` has no `ON DELETE` action (migration `001_initial.sql:185`), so depending on whether `PRAGMA foreign_keys` is enabled on the connection:

- **FK enforcement off** (SQLite default): the profile row is deleted, but `release_storage` rows remain pointing at a non-existent profile. Subsequent queries that join through `release_storage` to `storage_profiles` silently return no profile, breaking playback and file access.
- **FK enforcement on**: the DELETE fails with a constraint error that surfaces as an unhandled `sqlx::Error`.

Neither outcome is good. The UI delete confirmation (`bae-ui/src/components/settings/storage_profiles.rs:459`) just says "Are you sure?" with no mention of affected releases.

## Why it matters

The storage profile holds the information needed to locate and decrypt a release's files — `location_path` for local, S3 credentials + bucket for cloud, and the `encrypted` flag. Once the profile is gone:

- `source_path` values in `files` still exist, but there's no way to know if decryption is needed
- For cloud profiles, the S3 credentials needed to reach the files are gone
- The files themselves are fine (still on disk or in S3), but bae can no longer access them

## What should happen

Before deleting a profile, check `release_storage` for linked releases. If any exist:

- Show the user how many releases use this profile
- Offer options: transfer releases to another profile first, or convert them to unmanaged (remove `release_storage` rows, leave files in place)
- Only allow deletion once no releases are linked

Also: the FK should probably have `ON DELETE RESTRICT` to make the constraint explicit, and `PRAGMA foreign_keys = ON` should be set on every connection.
