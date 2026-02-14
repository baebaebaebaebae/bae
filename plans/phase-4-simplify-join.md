# Phase 4: Simplify join

Replace the manual 5-field exchange with a single invite code. The two-step code exchange is the same regardless of cloud backend -- what adapts is how storage access is granted.

## Depends on

- Phase 2 fixes (in Phase 3 plan) -- `grant_access(member_id)` on the CloudHome trait, `JoinInfo` with consumer cloud variants, `Serialize`/`Deserialize` on `JoinInfo`
- Phase 3 (consumer clouds) -- not strictly required, but the invite code design accounts for consumer cloud `JoinInfo` variants

Can ship after Phase 2 fixes alone (S3-only invite codes work). Consumer cloud invite codes work automatically once Phase 3 backends are added.

---

## What's built

- Full invite flow in bae-desktop Library settings: create S3 client → `accept_invitation()` → unwrap key → bootstrap from snapshot → pull changesets → save credentials
- Invite form in Sync settings: enter pubkey + role → `create_invitation()` → writes membership entry → shows bucket/region/endpoint as text for the owner to manually share
- `ShareInfo` struct in bae-ui: bucket, region, endpoint, invitee_pubkey -- displayed as plain text after invite
- 5-field "Join Shared" form in `bae-ui/src/components/settings/join_library.rs`: bucket, region, endpoint, access_key, secret_key
- Desktop join handler in `bae-desktop/src/ui/components/settings/library.rs`: constructs `S3CloudHome` from the 5 fields, calls `accept_invitation()`, bootstraps

## What's not built

- Invite code encoding/decoding
- Single-paste join -- still 5 separate fields
- `grant_access` integration in invite flow (covered by Phase 2 fixes)
- Consumer cloud join flow (joiner signs into their own account)

---

## Invite code format

### Encoding -- `bae-core/src/join_code.rs` (new)

The invite code is a base64url-encoded JSON payload containing everything the joiner needs to connect to the cloud home. The encryption key is NOT in the code -- it's wrapped to the joiner's pubkey via the membership chain (already uploaded to `keys/{pubkey}.enc`).

```rust
use serde::{Deserialize, Serialize};
use crate::cloud_home::JoinInfo;

/// Payload encoded into an invite code.
#[derive(Serialize, Deserialize)]
pub struct InviteCode {
    /// Library ID the joiner will join.
    pub library_id: String,
    /// Library name (for display during join).
    pub library_name: String,
    /// Cloud home connection info.
    pub join_info: JoinInfo,
    /// Owner's Ed25519 public key (hex). Joiner can verify the membership chain.
    pub owner_pubkey: String,
}

/// Encode an InviteCode to a base64url string.
pub fn encode(code: &InviteCode) -> String {
    let json = serde_json::to_vec(code).expect("InviteCode is always serializable");
    base64_url::encode(&json)
}

/// Decode a base64url string to an InviteCode.
pub fn decode(s: &str) -> Result<InviteCode, JoinCodeError> {
    let bytes = base64_url::decode(s).map_err(|_| JoinCodeError::InvalidBase64)?;
    serde_json::from_slice(&bytes).map_err(|e| JoinCodeError::InvalidJson(e.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum JoinCodeError {
    #[error("invalid base64url encoding")]
    InvalidBase64,
    #[error("invalid invite code payload: {0}")]
    InvalidJson(String),
}
```

Dependencies: `base64-url` crate (or use the `base64` crate with URL-safe alphabet -- check which is already in the workspace). `serde_json` already in workspace.

Register in `bae-core/src/lib.rs`: `pub mod join_code;`.

### What's in the code per backend

| Backend | JoinInfo contents | What the joiner needs to do |
|---|---|---|
| S3 | `bucket, region, endpoint, access_key, secret_key` | Use embedded credentials directly |
| Google Drive | `folder_id` | Sign in with own Google account → shared folder already accessible |
| Dropbox | `shared_folder_id` | Sign in with own Dropbox account → shared folder already accessible |
| OneDrive | `drive_id, folder_id` | Sign in with own Microsoft account → shared folder already accessible |
| pCloud | `folder_id` | Sign in with own pCloud account → shared folder already accessible |

For S3: `JoinInfo::S3` must be extended to include credentials (the current variant only has bucket/region/endpoint). Update the enum:

```rust
pub enum JoinInfo {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,   // <-- new
        secret_key: String,   // <-- new
    },
    // ... consumer cloud variants unchanged ...
}
```

For S3 with minting: `grant_access` mints scoped IAM credentials and returns them in `JoinInfo::S3`. For S3 without minting (pre-shared credentials): the owner enters the shared credentials manually; `grant_access` returns them as-is.

For consumer clouds: `grant_access` shares the folder with the joiner's account and returns the folder ID. The joiner signs into their own account after pasting the code.

### S3CloudHome::grant_access update

The current S3 `grant_access` ignores member_id and returns bucket/region/endpoint without credentials. For invite codes to work, S3 needs to include the credentials the joiner will use.

Two modes:
1. **Minting** (Backblaze B2, AWS, etc.): `grant_access` calls the provider's IAM API to create scoped credentials → includes them in JoinInfo. This is Phase 3 credential minting scope -- for now, return the owner's own credentials.
2. **Shared credentials** (simple S3, MinIO): `grant_access` returns the owner's access_key/secret_key from the keyring.

For the initial Phase 4 implementation, `S3CloudHome::grant_access` reads the owner's credentials from the keyring and returns them:

```rust
async fn grant_access(&self, _member_id: &str) -> Result<JoinInfo, CloudHomeError> {
    Ok(JoinInfo::S3 {
        bucket: self.bucket.clone(),
        region: self.region.clone(),
        endpoint: self.endpoint.clone(),
        access_key: self.access_key.clone(),
        secret_key: self.secret_key.clone(),
    })
}
```

This means S3CloudHome needs to store access_key/secret_key (it currently discards them after creating the AWS client). Add fields.

---

## Owner-side flow

### invite.rs update -- `bae-core/src/sync/invite.rs`

`create_invitation()` already returns `JoinInfo` from `grant_access()` (after Phase 2 fixes). Extend the desktop caller to encode the invite code:

```rust
// After create_invitation() succeeds:
let invite_code = join_code::InviteCode {
    library_id: config.library_id.clone(),
    library_name: config.library_name.clone().unwrap_or_default(),
    join_info,
    owner_pubkey: owner_keypair.public_key_hex(),
};
let code_string = join_code::encode(&invite_code);
// Display in UI for copying
```

### UI changes -- owner side

**`bae-ui/src/stores/sync.rs`:**

Replace `ShareInfo` with invite code:

```rust
// Before:
pub struct ShareInfo {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub invitee_pubkey: String,
}

// After:
pub struct ShareInfo {
    /// The invite code string (base64url) to share with the invitee.
    pub invite_code: String,
    /// Display name of the invitee (truncated pubkey).
    pub invitee_display: String,
}
```

**`bae-ui/src/components/settings/sync.rs` -- invite success display:**

Currently shows bucket/region/endpoint as plain text. Replace with:

```
┌─────────────────────────────────────────────────────┐
│ Invite Code                                          │
│                                                      │
│  Send this code to {invitee_display}:               │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │ eyJ0eXAiOiJKV1QiLCJhbGc... (truncated)     │    │
│  │                                    [Copy]    │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│  The code contains the cloud home connection info.  │
│  The encryption key is delivered separately via     │
│  the membership chain.                              │
│                                                      │
│                                        [Done]        │
└─────────────────────────────────────────────────────┘
```

Implementation: a read-only `TextInput` (or `<textarea>`) with the full invite code string, plus a "Copy" button that copies to clipboard. Reuse the existing copy-to-clipboard pattern from the pubkey copy button.

Update `SyncSectionView` props:
- `share_info: Option<ShareInfo>` -- no change to prop name, but the struct contents change from 4 S3 fields to `invite_code: String` + `invitee_display: String`
- Existing `on_copy_pubkey` pattern reused for copy button

**`bae-desktop/src/ui/components/settings/sync.rs`:**

After successful `create_invitation()`:
1. Encode `JoinInfo` into invite code (via `join_code::encode`)
2. Set `share_info` in store with the code string
3. UI shows the invite code card

---

## Joiner-side flow

### UI changes -- joiner side

**`bae-ui/src/components/settings/join_library.rs` -- complete rewrite:**

Replace the 5-field S3 form with a single paste input.

```rust
#[component]
pub fn JoinLibraryView(
    /// The invite code input value.
    invite_code: String,
    /// Current status of the join operation.
    status: Option<JoinStatus>,
    /// For consumer cloud joiners: whether OAuth sign-in is needed.
    needs_sign_in: bool,
    /// The cloud provider to sign into (shown when needs_sign_in is true).
    sign_in_provider: Option<CloudProvider>,
    /// Library name decoded from the invite code (shown for confirmation).
    decoded_library_name: Option<String>,
    /// Owner pubkey decoded from the invite code (shown for verification).
    decoded_owner_pubkey: Option<String>,

    // --- Callbacks ---
    on_code_change: EventHandler<String>,
    /// Called when the user clicks "Join". Desktop wrapper decodes and handles.
    on_join: EventHandler<()>,
    /// Called when the user clicks "Sign in with {provider}" (consumer clouds).
    on_sign_in: EventHandler<()>,
    /// Called when the user clicks "Cancel" to go back.
    on_cancel: EventHandler<()>,
) -> Element;
```

### Join form layout

**Step 1: Paste code**

```
┌─────────────────────────────────────────────────────┐
│ Join Shared Library                                  │
│                                                      │
│  Paste the invite code you received from the owner. │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │ Invite code                                  │    │
│  │ (paste here)                                 │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│                           [Cancel]  [Join Library]   │
└─────────────────────────────────────────────────────┘
```

The code is decoded on the fly as the user types/pastes. If valid, show the library name and owner pubkey:

```
┌─────────────────────────────────────────────────────┐
│ Join Shared Library                                  │
│                                                      │
│  Paste the invite code you received from the owner. │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │ eyJ0eXAiOiJKV1QiLCJhbGc...                  │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│  Library: "groovin-coltrane"                        │
│  Owner: a3f8...c4d2                                 │
│  Cloud home: Google Drive                           │
│                                                      │
│                           [Cancel]  [Join Library]   │
└─────────────────────────────────────────────────────┘
```

**Step 2a: S3 join (no extra sign-in needed)**

After clicking "Join Library", the status shows progress:

```
  Connecting to cloud home...
  Downloading library snapshot...
  Applying changes...
  Successfully joined! Restarting...
```

**Step 2b: Consumer cloud join (OAuth sign-in needed)**

After clicking "Join Library", the UI detects a consumer cloud `JoinInfo` and prompts sign-in:

```
┌─────────────────────────────────────────────────────┐
│ Join Shared Library                                  │
│                                                      │
│  Sign in to access the shared library on            │
│  Google Drive.                                       │
│                                                      │
│       [Sign in with Google]                          │
│                                                      │
│  The owner has already shared the cloud home        │
│  folder with your account.                          │
│                                                      │
│                                        [Cancel]      │
└─────────────────────────────────────────────────────┘
```

After OAuth completes, the join proceeds automatically (same as S3 from that point).

### Desktop handler -- `bae-desktop/src/ui/components/settings/library.rs`

The current join handler constructs an `S3CloudHome` from 5 separate fields. Replace with:

```rust
async fn handle_join(invite_code_str: &str, key_service: &KeyService) -> Result<(), JoinError> {
    // 1. Decode invite code
    let code = join_code::decode(invite_code_str)?;

    // 2. Construct CloudHome from JoinInfo
    let cloud_home: Box<dyn CloudHome> = match &code.join_info {
        JoinInfo::S3 { bucket, region, endpoint, access_key, secret_key } => {
            // Store credentials in keyring
            key_service.set_cloud_home_access_key(access_key)?;
            key_service.set_cloud_home_secret_key(secret_key)?;
            Box::new(S3CloudHome::new(
                bucket.clone(), region.clone(), endpoint.clone(),
                access_key.clone(), secret_key.clone(),
            ).await?)
        }
        JoinInfo::GoogleDrive { folder_id } => {
            // OAuth sign-in first (user clicks "Sign in with Google")
            // Tokens stored in keyring by the OAuth flow
            let tokens = key_service.get_cloud_home_oauth_token()?;
            Box::new(GoogleDriveCloudHome::new(folder_id.clone(), tokens)?)
        }
        // ... other consumer clouds
    };

    // 3. Wrap in CloudHomeSyncBucket (same as existing flow)
    let encryption = EncryptionService::new(/* ... */);
    let sync_bucket = CloudHomeSyncBucket::new(cloud_home, encryption);

    // 4. Accept invitation (existing -- unwrap key, validate chain)
    accept_invitation(&sync_bucket, user_keypair).await?;

    // 5. Bootstrap from snapshot (existing)
    bootstrap_from_snapshot(&sync_bucket, &db).await?;

    // 6. Save config (cloud_provider, folder IDs, etc.)
    config.cloud_provider = Some(provider_from_join_info(&code.join_info));
    config.save()?;

    Ok(())
}
```

### JoinStatus update -- `bae-ui/src/components/settings/join_library.rs`

Reuse existing `JoinStatus` enum but add a variant for the sign-in prompt:

```rust
pub enum JoinStatus {
    /// Decoded the invite code, waiting for user action.
    Decoded,
    /// Need OAuth sign-in before proceeding (consumer clouds).
    NeedsSignIn(String), // provider display name
    /// Currently joining.
    Joining(String), // progress message
    /// Join succeeded.
    Success,
    /// Join failed with an error.
    Error(String),
}
```

---

## Desktop library switcher update

The library switcher in `bae-ui/src/components/settings/library.rs` currently has a "Join Shared Library" button that navigates to the 5-field form. No change needed to the navigation -- only the destination view changes (from 5-field form to invite code paste).

The `LibrarySectionView` already has `on_join_shared` callback and `show_join_form` state. These stay the same.

---

## Invite form update (owner side)

The invite form in `SyncSectionView` currently shows: pubkey input → role selector → "Invite" button → on success, shows ShareInfo (bucket/region/endpoint text).

After Phase 4, the success display changes from plain text S3 coordinates to a copyable invite code. The invite form inputs (pubkey, role) stay the same. Only the success output changes.

**New props on SyncSectionView** (replacing current ShareInfo-related ones):

No new props needed. `share_info: Option<ShareInfo>` stays, but `ShareInfo` struct changes as described above.

---

## Mocks

### `bae-mocks/src/mocks/settings.rs`

Update settings mock to show:
- Invite success state with an invite code (base64url string) instead of S3 coordinates
- Join form with single paste input instead of 5 fields

### `bae-mocks/src/pages/settings.rs`

Pass updated props for the new `ShareInfo` shape and `JoinLibraryView` shape.

### `bae-mocks/src/mocks/join_library.rs` (new or updated)

Mock showing:
1. Empty paste input state
2. Valid code pasted (showing decoded library name, owner, provider)
3. Consumer cloud sign-in prompt
4. Join in progress
5. Join success

---

## Tests

### `bae-core/src/join_code.rs` -- unit tests

```rust
#[test]
fn round_trip_s3() {
    let code = InviteCode {
        library_id: "lib-123".into(),
        library_name: "My Library".into(),
        join_info: JoinInfo::S3 {
            bucket: "my-bucket".into(),
            region: "us-east-1".into(),
            endpoint: None,
            access_key: "AKIA...".into(),
            secret_key: "secret".into(),
        },
        owner_pubkey: "deadbeef".into(),
    };
    let encoded = encode(&code);
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded.library_id, "lib-123");
    assert_eq!(decoded.library_name, "My Library");
}

#[test]
fn round_trip_google_drive() {
    let code = InviteCode {
        library_id: "lib-456".into(),
        library_name: "Shared".into(),
        join_info: JoinInfo::GoogleDrive { folder_id: "abc123".into() },
        owner_pubkey: "cafebabe".into(),
    };
    let encoded = encode(&code);
    let decoded = decode(&encoded).unwrap();
    // ...
}

#[test]
fn decode_invalid_base64() {
    assert!(decode("not-valid!!!").is_err());
}

#[test]
fn decode_invalid_json() {
    let encoded = base64_url::encode(b"not json");
    assert!(decode(&encoded).is_err());
}
```

---

## Summary of file changes

| File | Change |
|---|---|
| `bae-core/src/join_code.rs` | **New.** InviteCode struct, encode/decode, tests |
| `bae-core/src/cloud_home/mod.rs` | Extend `JoinInfo::S3` with access_key/secret_key fields |
| `bae-core/src/cloud_home/s3.rs` | Store access_key/secret_key on S3CloudHome, return them from `grant_access` |
| `bae-core/src/lib.rs` | Register `join_code` module |
| `bae-ui/src/stores/sync.rs` | Change `ShareInfo` to hold invite_code + invitee_display |
| `bae-ui/src/components/settings/join_library.rs` | **Rewrite.** Single paste input, decoded preview, sign-in prompt |
| `bae-ui/src/components/settings/sync.rs` | Update invite success display to show copyable code instead of S3 coordinates |
| `bae-desktop/src/ui/components/settings/library.rs` | Decode invite code, construct CloudHome from JoinInfo, handle consumer cloud OAuth |
| `bae-desktop/src/ui/components/settings/sync.rs` | After invite, encode JoinInfo into invite code string |
| `bae-mocks/src/mocks/settings.rs` | Updated mock data |
| `bae-mocks/src/pages/settings.rs` | Updated props |

## Verification

- `cargo clippy -p bae-core -p bae-ui -p bae-desktop -p bae-mocks -- -D warnings`
- `cargo test -p bae-core` -- join_code round-trip tests pass
- Manual: owner invites via pubkey → invite code shown → owner copies
- Manual: joiner pastes invite code → decoded library name shown → clicks Join → bootstrap succeeds
- Manual: verify old 5-field form is gone, replaced by single paste input
- Manual: consumer cloud invite code → sign-in prompt → after OAuth → join proceeds
