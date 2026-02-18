use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_types::region::Region;

use crate::registry::LibraryEntry;

pub struct S3Client {
    client: Client,
    bucket: String,
    key_prefix: String,
}

pub enum S3Error {
    NotFound,
    Other(String),
}

impl S3Client {
    pub async fn new(entry: &LibraryEntry) -> Result<Self, String> {
        let credentials = Credentials::new(
            &entry.s3_access_key,
            &entry.s3_secret_key,
            None,
            None,
            "bae-proxy-registry",
        );

        let config = aws_sdk_s3::config::Builder::new()
            .region(Region::new(entry.s3_region.clone()))
            .endpoint_url(&entry.s3_endpoint)
            .credentials_provider(credentials)
            .force_path_style(true)
            .behavior_version_latest()
            .build();

        let client = Client::from_conf(config);

        Ok(Self {
            client,
            bucket: entry.s3_bucket.clone(),
            key_prefix: entry.s3_key_prefix.clone(),
        })
    }

    fn prefixed_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }

    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>, S3Error> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.prefixed_key(key))
            .send()
            .await
            .map_err(|e| {
                if is_not_found(&e) {
                    S3Error::NotFound
                } else {
                    S3Error::Other(format!("get_object {key}: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| S3Error::Other(format!("read body {key}: {e}")))?;

        Ok(bytes.into_bytes().to_vec())
    }

    pub async fn get_object_range(
        &self,
        key: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, S3Error> {
        let range = format!("bytes={start}-{end}");
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.prefixed_key(key))
            .range(range)
            .send()
            .await
            .map_err(|e| {
                if is_not_found(&e) {
                    S3Error::NotFound
                } else {
                    S3Error::Other(format!("get_object_range {key}: {e}"))
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| S3Error::Other(format!("read body {key}: {e}")))?;

        Ok(bytes.into_bytes().to_vec())
    }

    pub async fn put_object(&self, key: &str, data: Vec<u8>) -> Result<(), S3Error> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.prefixed_key(key))
            .body(data.into())
            .send()
            .await
            .map_err(|e| S3Error::Other(format!("put_object {key}: {e}")))?;

        Ok(())
    }

    pub async fn delete_object(&self, key: &str) -> Result<(), S3Error> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.prefixed_key(key))
            .send()
            .await
            .map_err(|e| S3Error::Other(format!("delete_object {key}: {e}")))?;

        Ok(())
    }

    pub async fn head_object(&self, key: &str) -> Result<bool, S3Error> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.prefixed_key(key))
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) if is_not_found(&e) => Ok(false),
            Err(e) => Err(S3Error::Other(format!("head_object {key}: {e}"))),
        }
    }

    pub async fn list_objects(&self, prefix: &str) -> Result<Vec<String>, S3Error> {
        let full_prefix = self.prefixed_key(prefix);
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);

            if let Some(token) = &continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| S3Error::Other(format!("list_objects {prefix}: {e}")))?;

            for obj in resp.contents() {
                if let Some(key) = obj.key() {
                    // Strip the key_prefix so callers see relative paths.
                    if let Some(relative) = key.strip_prefix(&self.key_prefix) {
                        keys.push(relative.to_string());
                    }
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
}

fn is_not_found<E: std::fmt::Display>(err: &aws_sdk_s3::error::SdkError<E>) -> bool {
    matches!(err, aws_sdk_s3::error::SdkError::ServiceError(e) if e.raw().status().as_u16() == 404)
}
