//! S3-backed `CloudHome` implementation.
//!
//! Wraps `aws-sdk-s3` to provide raw storage operations against any
//! S3-compatible endpoint.

use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;

use super::{CloudHome, CloudHomeError, JoinInfo};

/// S3-backed cloud home.
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
        let credentials = Credentials::new(access_key, secret_key, None, None, "bae-cloud-home");

        let mut builder = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.clone()))
            .credentials_provider(credentials);

        if let Some(ref ep) = endpoint {
            builder = builder.endpoint_url(ep.trim_end_matches('/'));
        }

        let aws_config = builder.load().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);

        Ok(S3CloudHome {
            client,
            bucket,
            region,
            endpoint,
        })
    }
}

#[async_trait]
impl CloudHome for S3CloudHome {
    async fn write(&self, key: &str, data: Vec<u8>) -> Result<(), CloudHomeError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("put {key}: {e}")))?;
        Ok(())
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
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
                    CloudHomeError::NotFound(key.to_string())
                } else {
                    CloudHomeError::Storage(format!("get {key}: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read body for {key}: {e}")))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        let range = format!("bytes={start}-{}", end.saturating_sub(1));
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .range(range)
            .send()
            .await
            .map_err(|e| {
                let msg = format!("{e}");
                if msg.contains("NoSuchKey") || msg.contains("not found") || msg.contains("404") {
                    CloudHomeError::NotFound(key.to_string())
                } else {
                    CloudHomeError::Storage(format!("get range {key}: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read range body for {key}: {e}")))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
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
                .map_err(|e| CloudHomeError::Storage(format!("list {prefix}: {e}")))?;

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

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("delete {key}: {e}")))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("NotFound")
                    || msg.contains("not found")
                    || msg.contains("404")
                    || msg.contains("NoSuchKey")
                {
                    Ok(false)
                } else {
                    Err(CloudHomeError::Storage(format!("head {key}: {e}")))
                }
            }
        }
    }

    fn join_info(&self) -> JoinInfo {
        JoinInfo::S3 {
            bucket: self.bucket.clone(),
            region: self.region.clone(),
            endpoint: self.endpoint.clone(),
        }
    }

    async fn revoke_access(&self) -> Result<(), CloudHomeError> {
        // S3 access is managed externally (IAM/pre-shared credentials).
        Ok(())
    }
}
