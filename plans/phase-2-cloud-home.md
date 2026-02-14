# Phase 2: CloudHome trait

Introduce a low-level storage abstraction. Refactor existing S3 sync to use it. Each future backend implements only 8 methods.

## Design

Two layers:

1. **CloudHome trait** (8 methods) — raw storage. No encryption, no path layout knowledge, no sync semantics. Just bytes in, bytes out.
2. **CloudHomeSyncBucket** (concrete struct) — wraps any `dyn CloudHome` + `EncryptionService`. Handles the cloud home path layout (`changes/`, `heads/`, `images/`, `membership/`, `keys/`, `snapshot.db.enc`) and encryption. Implements the existing `SyncBucketClient` trait.

This means:
- Adding a new backend = implement 8 methods on CloudHome. Sync works automatically.
- `SyncBucketClient` trait stays unchanged. Existing tests (MockBucket) still work.
- SyncService, pull, snapshot, invite — all untouched. They receive `&dyn SyncBucketClient` as before.

## What exists

- `SyncBucketClient` trait (18 methods) — `bae-core/src/sync/bucket.rs`
- `S3SyncBucketClient` — `bae-core/src/sync/s3_bucket.rs`, implements `SyncBucketClient` using `aws-sdk-s3` directly
- `CloudStorage` trait (4 methods) — `bae-core/src/cloud_storage.rs`, used for release file storage
- `S3CloudStorage` — implements `CloudStorage`, separate S3 client
- `MockBucket` — `bae-core/src/sync/test_helpers.rs`, in-memory mock of `SyncBucketClient`

## Changes

### 1. CloudHome trait — `bae-core/src/cloud_home/mod.rs` (new)

```rust
#[async_trait]
pub trait CloudHome: Send + Sync {
    /// Write bytes to a path.
    async fn write(&self, path: &str, data: &[u8]) -> Result<(), CloudHomeError>;

    /// Read all bytes from a path.
    async fn read(&self, path: &str) -> Result<Vec<u8>, CloudHomeError>;

    /// Read a byte range [start, end) from a path.
    async fn read_range(&self, path: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError>;

    /// List all keys under a prefix.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError>;

    /// Delete an object at path.
    async fn delete(&self, path: &str) -> Result<(), CloudHomeError>;

    /// Check if an object exists at path.
    async fn exists(&self, path: &str) -> Result<bool, CloudHomeError>;

    /// Grant a member access to this cloud home. Returns the info the joiner needs to connect.
    /// S3: returns bucket/region/endpoint (credentials shared out-of-band).
    /// Consumer clouds: shares the folder with the member's account.
    async fn grant_access(&self, member_id: &str) -> Result<JoinInfo, CloudHomeError>;

    /// Revoke a member's access.
    async fn revoke_access(&self, member_id: &str) -> Result<(), CloudHomeError>;
}
```

Error type:
```rust
#[derive(Debug, thiserror::Error)]
pub enum CloudHomeError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

JoinInfo:
```rust
pub enum JoinInfo {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
    },
    // Future variants: GoogleDrive { folder_id }, ICloud { container_id }, etc.
}
```

Register module in `bae-core/src/lib.rs`: add `pub mod cloud_home;`.

### 2. S3CloudHome — `bae-core/src/cloud_home/s3.rs` (new)

Implement `CloudHome` for S3. Extract the raw S3 operations from `S3SyncBucketClient` (the 4 private methods: `get_object`, `put_object`, `delete_object`, `list_keys`).

```rust
pub struct S3CloudHome {
    client: Client,
    bucket: String,
    region: String,
    endpoint: Option<String>,
}

impl S3CloudHome {
    pub async fn new(
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    ) -> Result<Self, CloudHomeError> {
        // Same S3 client construction as S3SyncBucketClient::new()
    }
}

#[async_trait]
impl CloudHome for S3CloudHome {
    async fn write(&self, path: &str, data: &[u8]) -> Result<(), CloudHomeError> {
        // put_object
    }
    async fn read(&self, path: &str) -> Result<Vec<u8>, CloudHomeError> {
        // get_object, map NoSuchKey to NotFound
    }
    async fn read_range(&self, path: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        // get_object with Range header
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
        // list_objects_v2 with pagination
    }
    async fn delete(&self, path: &str) -> Result<(), CloudHomeError> {
        // delete_object
    }
    async fn exists(&self, path: &str) -> Result<bool, CloudHomeError> {
        // head_object, map NotFound to false
    }
    async fn grant_access(&self, _member_id: &str) -> Result<JoinInfo, CloudHomeError> {
        // S3: access is out-of-band, just return connection info
        Ok(JoinInfo::S3 { bucket, region, endpoint })
    }
    async fn revoke_access(&self, _member_id: &str) -> Result<(), CloudHomeError> {
        // S3: no-op (credentials revoked out-of-band)
        Ok(())
    }
}
```

### 3. CloudHomeSyncBucket — `bae-core/src/sync/cloud_home_bucket.rs` (new)

Generic `SyncBucketClient` implementation that wraps any `dyn CloudHome`. Moves all path layout logic and encryption from `S3SyncBucketClient` into this generic struct.

```rust
pub struct CloudHomeSyncBucket {
    home: Box<dyn CloudHome>,
    encryption: Arc<RwLock<EncryptionService>>,
}

impl CloudHomeSyncBucket {
    pub fn new(home: Box<dyn CloudHome>, encryption: EncryptionService) -> Self {
        Self {
            home,
            encryption: Arc::new(RwLock::new(encryption)),
        }
    }

    pub fn shared_encryption(&self) -> Arc<RwLock<EncryptionService>> {
        self.encryption.clone()
    }

    fn enc(&self) -> std::sync::RwLockReadGuard<'_, EncryptionService> {
        self.encryption.read().unwrap()
    }

    fn image_key(id: &str) -> String {
        let hex = id.replace('-', "");
        format!("images/{}/{}/{id}", &hex[..2], &hex[2..4])
    }
}

#[async_trait]
impl SyncBucketClient for CloudHomeSyncBucket {
    // All 18 methods — same logic as S3SyncBucketClient, but using self.home
    // instead of self.client (S3 Client).
    //
    // Example:
    async fn get_changeset(&self, device_id: &str, seq: u64) -> Result<Vec<u8>, BucketError> {
        let key = format!("changes/{device_id}/{seq}.enc");
        let encrypted = self.home.read(&key).await
            .map_err(|e| match e {
                CloudHomeError::NotFound(k) => BucketError::NotFound(k),
                e => BucketError::S3(e.to_string()),
            })?;
        self.enc().decrypt(&encrypted)
            .map_err(|e| BucketError::Decryption(format!("changeset {device_id}/{seq}: {e}")))
    }

    // ... 17 more methods with the same pattern:
    //   construct path -> call self.home.read/write/list/delete -> map errors -> encrypt/decrypt
}
```

Also add the `list_image_keys` method that `S3SyncBucketClient` currently has as an inherent method.

### 4. Remove S3SyncBucketClient — `bae-core/src/sync/s3_bucket.rs`

Delete this file entirely. All its logic is now split between:
- Raw S3 operations → `S3CloudHome`
- Path layout + encryption → `CloudHomeSyncBucket`

Remove `pub mod s3_bucket;` from `bae-core/src/sync/mod.rs`. Add `pub mod cloud_home_bucket;`.

### 5. Update bae-desktop — `bae-desktop/src/main.rs`

In `create_sync_handle()` (around line 446):

Before:
```rust
let bucket = S3SyncBucketClient::new(bucket, region, endpoint, access_key, secret_key, encryption).await?;
```

After:
```rust
let cloud_home = S3CloudHome::new(bucket, region, endpoint, access_key, secret_key).await?;
let bucket = CloudHomeSyncBucket::new(Box::new(cloud_home), encryption);
```

Same type flows through (`bucket` still implements `SyncBucketClient`).

### 6. Update bae-server — `bae-server/src/main.rs`

Same pattern as bae-desktop: create `S3CloudHome`, wrap in `CloudHomeSyncBucket`.

### 7. Update snapshot — `bae-core/src/sync/snapshot.rs`

`create_and_upload_snapshot()` receives `&dyn SyncBucketClient` — no change needed. But check if it also calls `list_image_keys()` (which was on `S3SyncBucketClient` directly). If so, add `list_image_keys` to CloudHomeSyncBucket as an inherent method.

### 8. BucketError conversion

Add `From<CloudHomeError> for BucketError`:
```rust
impl From<CloudHomeError> for BucketError {
    fn from(e: CloudHomeError) -> Self {
        match e {
            CloudHomeError::NotFound(k) => BucketError::NotFound(k),
            e => BucketError::S3(e.to_string()),
        }
    }
}
```

### 9. MockCloudHome for future tests — `bae-core/src/cloud_home/mock.rs` (new, test-only)

Simple in-memory CloudHome for testing the CloudHomeSyncBucket path layout logic. Optional but useful — `HashMap<String, Vec<u8>>` backing store. Feature-gated behind `#[cfg(test)]`.

## What doesn't change

- `SyncBucketClient` trait — stays as-is, all 18 methods unchanged
- `MockBucket` — stays as-is, all existing pull/sync tests unchanged
- `SyncService`, `pull.rs`, `snapshot.rs`, `invite.rs` — receive `&dyn SyncBucketClient`, no changes
- `CloudStorage` trait + `S3CloudStorage` — left for a future refactor (release file storage is separate scope)
- `HeadJson`, `MinSchemaVersionJson` serialization structs — move to `cloud_home_bucket.rs`

## Verification

- `cargo clippy -p bae-core -p bae-desktop -p bae-server -- -D warnings`
- `cargo test -p bae-core` — all existing sync tests pass (MockBucket unchanged)
- Manually verify bae-desktop compiles with `S3CloudHome` + `CloudHomeSyncBucket`
- Manually verify bae-server compiles
