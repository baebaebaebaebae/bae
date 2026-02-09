# Multiple default profiles possible

## Problem

Creating or updating a storage profile with `is_default = true` does not clear the default flag on other profiles, allowing multiple profiles to be marked as default simultaneously.

## What happens today

There are two code paths that set `is_default`:

1. **Explicit "Set Default" button** — calls `set_default_storage_profile()` at `client.rs:1602`, which first clears all defaults (`UPDATE ... SET is_default = FALSE WHERE is_default = TRUE`), then sets the new one. This is correct.

2. **Create/update via `save_storage_profile()`** — at `app_service.rs:1041`. For new profiles, `insert_storage_profile()` writes the row with whatever `is_default` value is on the struct. For updates, `update_storage_profile()` at `client.rs:1566` writes `is_default` directly in the UPDATE. Neither path clears other defaults first.

The UI profile editor (`bae-ui/.../storage_profiles.rs:544`) defaults `is_default` to `true` for new profiles, so creating a second profile immediately produces two defaults.

`get_default_storage_profile()` at `client.rs:1556` uses `fetch_optional` on `WHERE is_default = TRUE`, which returns an arbitrary row when multiple match.

## Fix

The insert and update paths should call `set_default_storage_profile()` when `is_default` is true, or the DB layer should enforce the invariant (e.g., a trigger that clears other defaults on insert/update). The simplest fix is to add a `clear other defaults` step inside `insert_storage_profile()` and `update_storage_profile()` when `profile.is_default` is true.
