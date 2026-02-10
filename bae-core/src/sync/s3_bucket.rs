/// S3-backed implementation of `SyncBucketClient`.
///
/// Uses `aws-sdk-s3` to talk to an S3-compatible bucket. Handles
/// encryption/decryption for changesets, heads, and images.
/// Snapshot blobs are stored/returned encrypted (the caller handles that).
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use serde::{Deserialize, Serialize};

use super::bucket::{BucketError, DeviceHead, SyncBucketClient};
use crate::encryption::EncryptionService;

/// Serialized form of a device head stored in `heads/{device_id}.json.enc`.
#[derive(Serialize, Deserialize)]
struct HeadJson {
    seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_seq: Option<u64>,
    /// RFC 3339 timestamp of when this head was last written.
    #[serde(skip_serializing_if = "Option::is_none")]
    last_sync: Option<String>,
}

/// Serialized form of `min_schema_version.json.enc`.
#[derive(Serialize, Deserialize)]
struct MinSchemaVersionJson {
    min_schema_version: u32,
}

/// S3-backed sync bucket client.
///
/// Encryption semantics match the `SyncBucketClient` trait:
/// - `get_changeset` / `download_image`: returns decrypted bytes
/// - `put_changeset` / `upload_image`: caller passes already-encrypted bytes
/// - `get_snapshot` / `put_snapshot`: raw encrypted bytes (caller handles crypto)
/// - `list_heads` / `put_head`: head JSON is encrypted/decrypted internally
pub struct S3SyncBucketClient {
    client: Client,
    bucket: String,
    encryption: EncryptionService,
}

impl S3SyncBucketClient {
    pub async fn new(
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
        encryption: EncryptionService,
    ) -> Result<Self, BucketError> {
        let credentials = Credentials::new(access_key, secret_key, None, None, "bae-sync-bucket");

        let mut builder = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region))
            .credentials_provider(credentials);

        if let Some(ref ep) = endpoint {
            builder = builder.endpoint_url(ep.trim_end_matches('/'));
        }

        let aws_config = builder.load().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);

        Ok(S3SyncBucketClient {
            client,
            bucket,
            encryption,
        })
    }

    /// Download an object by key. Returns raw bytes.
    async fn get_object(&self, key: &str) -> Result<Vec<u8>, BucketError> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let msg = format!("{e}");
                if msg.contains("NoSuchKey") || msg.contains("not found") || msg.contains("404") {
                    BucketError::NotFound(key.to_string())
                } else {
                    BucketError::S3(format!("get {key}: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| BucketError::S3(format!("read body for {key}: {e}")))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    /// Upload bytes to a key.
    async fn put_object(&self, key: &str, data: Vec<u8>) -> Result<(), BucketError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .send()
            .await
            .map_err(|e| BucketError::S3(format!("put {key}: {e}")))?;
        Ok(())
    }

    /// Delete an object by key.
    async fn delete_object(&self, key: &str) -> Result<(), BucketError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| BucketError::S3(format!("delete {key}: {e}")))?;
        Ok(())
    }

    /// List all keys under a prefix.
    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>, BucketError> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);

            if let Some(token) = continuation_token.take() {
                req = req.continuation_token(token);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| BucketError::S3(format!("list {prefix}: {e}")))?;

            for obj in resp.contents() {
                if let Some(key) = obj.key() {
                    keys.push(key.to_string());
                }
            }

            if resp.is_truncated() == Some(true) {
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(keys)
    }

    /// Image key from ID: `images/{ab}/{cd}/{id}`.
    fn image_key(id: &str) -> String {
        let hex = id.replace('-', "");
        format!("images/{}/{}/{id}", &hex[..2], &hex[2..4])
    }
}

#[async_trait]
impl SyncBucketClient for S3SyncBucketClient {
    async fn list_heads(&self) -> Result<Vec<DeviceHead>, BucketError> {
        let keys = self.list_keys("heads/").await?;
        let mut heads = Vec::new();

        for key in &keys {
            // key = "heads/{device_id}.json.enc"
            let device_id = key
                .strip_prefix("heads/")
                .and_then(|s| s.strip_suffix(".json.enc"))
                .ok_or_else(|| BucketError::S3(format!("unexpected head key format: {key}")))?;

            let encrypted = self.get_object(key).await?;
            let decrypted = self
                .encryption
                .decrypt(&encrypted)
                .map_err(|e| BucketError::Decryption(format!("head {device_id}: {e}")))?;

            let head_json: HeadJson = serde_json::from_slice(&decrypted)
                .map_err(|e| BucketError::S3(format!("parse head {device_id}: {e}")))?;

            heads.push(DeviceHead {
                device_id: device_id.to_string(),
                seq: head_json.seq,
                snapshot_seq: head_json.snapshot_seq,
                last_sync: head_json.last_sync,
            });
        }

        Ok(heads)
    }

    async fn get_changeset(&self, device_id: &str, seq: u64) -> Result<Vec<u8>, BucketError> {
        let key = format!("changes/{device_id}/{seq}.enc");
        let encrypted = self.get_object(&key).await?;
        self.encryption
            .decrypt(&encrypted)
            .map_err(|e| BucketError::Decryption(format!("changeset {device_id}/{seq}: {e}")))
    }

    async fn put_changeset(
        &self,
        device_id: &str,
        seq: u64,
        data: Vec<u8>,
    ) -> Result<(), BucketError> {
        let key = format!("changes/{device_id}/{seq}.enc");
        self.put_object(&key, data).await
    }

    async fn put_head(
        &self,
        device_id: &str,
        seq: u64,
        snapshot_seq: Option<u64>,
        timestamp: &str,
    ) -> Result<(), BucketError> {
        let head = HeadJson {
            seq,
            snapshot_seq,
            last_sync: Some(timestamp.to_string()),
        };
        let json = serde_json::to_vec(&head)
            .map_err(|e| BucketError::S3(format!("serialize head: {e}")))?;
        let encrypted = self.encryption.encrypt(&json);
        let key = format!("heads/{device_id}.json.enc");
        self.put_object(&key, encrypted).await
    }

    async fn upload_image(&self, id: &str, data: Vec<u8>) -> Result<(), BucketError> {
        let key = Self::image_key(id);
        self.put_object(&key, data).await
    }

    async fn download_image(&self, id: &str) -> Result<Vec<u8>, BucketError> {
        let key = Self::image_key(id);
        let encrypted = self.get_object(&key).await?;
        self.encryption
            .decrypt(&encrypted)
            .map_err(|e| BucketError::Decryption(format!("image {id}: {e}")))
    }

    async fn put_snapshot(&self, data: Vec<u8>) -> Result<(), BucketError> {
        self.put_object("snapshot.db.enc", data).await
    }

    async fn get_snapshot(&self) -> Result<Vec<u8>, BucketError> {
        self.get_object("snapshot.db.enc").await
    }

    async fn delete_changeset(&self, device_id: &str, seq: u64) -> Result<(), BucketError> {
        let key = format!("changes/{device_id}/{seq}.enc");
        self.delete_object(&key).await
    }

    async fn list_changesets(&self, device_id: &str) -> Result<Vec<u64>, BucketError> {
        let prefix = format!("changes/{device_id}/");
        let keys = self.list_keys(&prefix).await?;

        let mut seqs: Vec<u64> = keys
            .iter()
            .filter_map(|k| {
                k.strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".enc"))
                    .and_then(|s| s.parse().ok())
            })
            .collect();
        seqs.sort();
        Ok(seqs)
    }

    async fn get_min_schema_version(&self) -> Result<Option<u32>, BucketError> {
        let key = "min_schema_version.json.enc";
        let encrypted = match self.get_object(key).await {
            Ok(data) => data,
            Err(BucketError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        let decrypted = self
            .encryption
            .decrypt(&encrypted)
            .map_err(|e| BucketError::Decryption(format!("min_schema_version: {e}")))?;

        let parsed: MinSchemaVersionJson = serde_json::from_slice(&decrypted)
            .map_err(|e| BucketError::S3(format!("parse min_schema_version: {e}")))?;

        Ok(Some(parsed.min_schema_version))
    }

    async fn set_min_schema_version(&self, version: u32) -> Result<(), BucketError> {
        let payload = MinSchemaVersionJson {
            min_schema_version: version,
        };
        let json = serde_json::to_vec(&payload)
            .map_err(|e| BucketError::S3(format!("serialize min_schema_version: {e}")))?;
        let encrypted = self.encryption.encrypt(&json);
        self.put_object("min_schema_version.json.enc", encrypted)
            .await
    }
}

/// List all image keys in the sync bucket.
///
/// Separate from `SyncBucketClient` because only bae-server needs to
/// enumerate all images for bulk download. Returns keys like
/// `images/ab/cd/{id}`.
impl S3SyncBucketClient {
    pub async fn list_image_keys(&self) -> Result<Vec<String>, BucketError> {
        self.list_keys("images/").await
    }
}
