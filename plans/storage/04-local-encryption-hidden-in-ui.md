# Local encryption hidden in UI

## Problem

The "Encrypted" checkbox in the storage profile editor is only rendered when the location is Cloud. Switching to Local explicitly resets `encrypted` to `false`. But the backend fully supports encrypted local profiles.

## What happens today

In `bae-ui/src/components/settings/storage_profiles.rs`:

- Line 799: the encryption checkbox is inside `if *location.read() == StorageLocation::Cloud { ... }`
- Line 675: selecting the Local radio button runs `encrypted.set(false)`

Meanwhile:
- `ReleaseStorageImpl::write_file()` encrypts based on `profile.encrypted` regardless of `StorageLocation`
- Playback handles encrypted local files correctly (uses `CloudStorageReader` which works for local encrypted too)
- `test_storage_permutations` in `bae-core/tests/test_storage.rs` tests all four combinations including local+encrypted

## Also

The encryption description text says "AES-256 encryption" (`bae-ui/.../storage_profiles.rs:811`) but the actual algorithm is XChaCha20-Poly1305 (libsodium `crypto_secretstream`). The encryption subsection elsewhere in settings correctly identifies the algorithm.

## Fix

- Show the encryption checkbox for both Local and Cloud profiles
- Fix the description text to say "XChaCha20-Poly1305" or just "Encrypted at rest" without naming the algorithm
- Remove the `encrypted.set(false)` on location switch (let the user control it independently)
