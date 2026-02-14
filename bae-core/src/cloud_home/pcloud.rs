//! pCloud `CloudHome` implementation.
//!
//! Uses the pCloud REST API with OAuth 2.0 tokens.
//! Files are stored flat in a single folder -- path separators are encoded as `__`.
//! pCloud access tokens are permanent (no refresh needed).

use async_trait::async_trait;

use super::{CloudHome, CloudHomeError, JoinInfo};
use crate::oauth::OAuthConfig;

/// pCloud cloud home backend.
pub struct PCloudCloudHome {
    client: reqwest::Client,
    folder_id: u64,
    api_host: String,
    access_token: String,
}

/// File metadata from a pCloud `listfolder` response.
struct PCloudFile {
    file_id: u64,
    name: String,
}

impl PCloudCloudHome {
    pub fn new(folder_id: u64, api_host: String, access_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            folder_id,
            api_host,
            access_token,
        }
    }

    pub fn oauth_config() -> OAuthConfig {
        OAuthConfig {
            // Placeholder: the actual client_id is set per-deployment.
            client_id: String::new(),
            // pCloud requires client_secret (no PKCE support). Set per-deployment.
            client_secret: Some(String::new()),
            auth_url: "https://my.pcloud.com/oauth2/authorize".to_string(),
            // Token URL needs the api_host, but at OAuth time we don't know
            // the region yet. The authorize endpoint works from either host.
            // We use the US host for token exchange; the response's locationid
            // tells us the correct host for subsequent API calls.
            token_url: "https://api.pcloud.com/oauth2_token".to_string(),
            scopes: vec![],
            redirect_port: 19284,
            extra_auth_params: vec![],
        }
    }

    /// Determine the API host from pCloud's locationid.
    /// locationid 1 = US, locationid 2 = EU.
    pub fn api_host_from_location_id(location_id: u64) -> &'static str {
        if location_id == 2 {
            "eapi.pcloud.com"
        } else {
            "api.pcloud.com"
        }
    }

    /// Encode a CloudHome key to a flat pCloud filename.
    /// `changes/dev1/42.enc` -> `changes__dev1__42.enc`
    fn encode_key(key: &str) -> String {
        key.replace('/', "__")
    }

    /// Decode a flat filename back to a CloudHome key.
    /// `changes__dev1__42.enc` -> `changes/dev1/42.enc`
    fn decode_key(filename: &str) -> String {
        filename.replace("__", "/")
    }

    /// Encode a prefix for filename matching.
    /// `changes/dev1/` -> `changes__dev1__`
    fn encode_prefix(prefix: &str) -> String {
        prefix.replace('/', "__")
    }

    fn api_url(&self, path: &str) -> String {
        format!("https://{}{}", self.api_host, path)
    }

    /// Make a GET API call and check the pCloud `result` field.
    /// Returns the parsed JSON response on success.
    async fn api_get(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<serde_json::Value, CloudHomeError> {
        let resp = self
            .client
            .get(self.api_url(path))
            .bearer_auth(&self.access_token)
            .query(params)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("request failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudHomeError::Storage(
                "pCloud access token revoked. Please sign in again.".to_string(),
            ));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read body: {e}")))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| CloudHomeError::Storage(format!("parse response: {e}")))?;

        let result = json["result"].as_u64().unwrap_or(999);
        if result != 0 {
            let error_msg = json["error"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(CloudHomeError::Storage(format!(
                "pCloud API error {result}: {error_msg}"
            )));
        }

        Ok(json)
    }

    /// List all files in the folder (single level, non-recursive).
    async fn list_folder_contents(&self) -> Result<Vec<PCloudFile>, CloudHomeError> {
        let folder_id_str = self.folder_id.to_string();
        let json = self
            .api_get(
                "/listfolder",
                &[("folderid", &folder_id_str), ("recursive", "0")],
            )
            .await?;

        let mut files = Vec::new();
        if let Some(contents) = json["metadata"]["contents"].as_array() {
            for item in contents {
                // Skip subfolders (isfolder == true)
                if item["isfolder"].as_bool().unwrap_or(false) {
                    continue;
                }
                if let (Some(file_id), Some(name)) =
                    (item["fileid"].as_u64(), item["name"].as_str())
                {
                    files.push(PCloudFile {
                        file_id,
                        name: name.to_string(),
                    });
                }
            }
        }

        Ok(files)
    }

    /// Find a file by its encoded name within the folder.
    async fn find_file(&self, encoded_name: &str) -> Result<Option<PCloudFile>, CloudHomeError> {
        let files = self.list_folder_contents().await?;
        Ok(files.into_iter().find(|f| f.name == encoded_name))
    }

    /// Get a download URL for a file by its file_id.
    /// pCloud's `getfilelink` returns hosts + path; we build a URL from them.
    async fn get_download_url(&self, file_id: u64) -> Result<String, CloudHomeError> {
        let file_id_str = file_id.to_string();
        let json = self
            .api_get("/getfilelink", &[("fileid", &file_id_str)])
            .await?;

        let hosts = json["hosts"]
            .as_array()
            .and_then(|h| h.first())
            .and_then(|h| h.as_str())
            .ok_or_else(|| CloudHomeError::Storage("no download host in response".to_string()))?;

        let path = json["path"]
            .as_str()
            .ok_or_else(|| CloudHomeError::Storage("no download path in response".to_string()))?;

        Ok(format!("https://{}{}", hosts, path))
    }
}

#[async_trait]
impl CloudHome for PCloudCloudHome {
    async fn write(&self, key: &str, data: Vec<u8>) -> Result<(), CloudHomeError> {
        let encoded = Self::encode_key(key);
        let folder_id_str = self.folder_id.to_string();

        // pCloud overwrites by default (old version saved as revision)
        let resp = self
            .client
            .put(self.api_url("/uploadfile"))
            .bearer_auth(&self.access_token)
            .query(&[
                ("folderid", folder_id_str.as_str()),
                ("filename", encoded.as_str()),
                ("nopartial", "1"),
            ])
            .body(data)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("upload failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CloudHomeError::Storage(
                "pCloud access token revoked. Please sign in again.".to_string(),
            ));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read upload response: {e}")))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| CloudHomeError::Storage(format!("parse upload response: {e}")))?;

        let result = json["result"].as_u64().unwrap_or(999);
        if result != 0 {
            let error_msg = json["error"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(CloudHomeError::Storage(format!(
                "upload {key} failed: pCloud error {result}: {error_msg}"
            )));
        }

        Ok(())
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
        let encoded = Self::encode_key(key);
        let file = self
            .find_file(&encoded)
            .await?
            .ok_or_else(|| CloudHomeError::NotFound(key.to_string()))?;

        let url = self.get_download_url(file.file_id).await?;

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("download failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(CloudHomeError::NotFound(key.to_string()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudHomeError::Storage(format!(
                "read {key} (HTTP {status}): {body}"
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read body for {key}: {e}")))?;

        Ok(bytes.to_vec())
    }

    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        let encoded = Self::encode_key(key);
        let file = self
            .find_file(&encoded)
            .await?
            .ok_or_else(|| CloudHomeError::NotFound(key.to_string()))?;

        let url = self.get_download_url(file.file_id).await?;
        let range = format!("bytes={}-{}", start, end.saturating_sub(1));

        let resp = self
            .client
            .get(&url)
            .header("Range", &range)
            .send()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("download range failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(CloudHomeError::NotFound(key.to_string()));
        }
        if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudHomeError::Storage(format!(
                "read range {key} (HTTP {status}): {body}"
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CloudHomeError::Storage(format!("read range body for {key}: {e}")))?;

        Ok(bytes.to_vec())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
        let encoded_prefix = Self::encode_prefix(prefix);
        let files = self.list_folder_contents().await?;

        let keys: Vec<String> = files
            .into_iter()
            .filter(|f| f.name.starts_with(&encoded_prefix))
            .map(|f| Self::decode_key(&f.name))
            .collect();

        Ok(keys)
    }

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        let encoded = Self::encode_key(key);

        if let Some(file) = self.find_file(&encoded).await? {
            let file_id_str = file.file_id.to_string();
            self.api_get("/deletefile", &[("fileid", &file_id_str)])
                .await?;
        }

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError> {
        let encoded = Self::encode_key(key);
        Ok(self.find_file(&encoded).await?.is_some())
    }

    async fn grant_access(&self, member_id: &str) -> Result<JoinInfo, CloudHomeError> {
        let folder_id_str = self.folder_id.to_string();
        // permissions is a bitmask: 1=create, 2=modify, 4=delete → 7 = full
        self.api_get(
            "/sharefolder",
            &[
                ("folderid", &folder_id_str),
                ("mail", member_id),
                ("permissions", "7"),
            ],
        )
        .await?;

        Ok(JoinInfo::PCloud {
            folder_id: self.folder_id,
        })
    }

    async fn revoke_access(&self, member_id: &str) -> Result<(), CloudHomeError> {
        // pCloud shares are identified by share ID, not email. List all outgoing
        // shares, find the one for this member on our folder, then cancel it.
        let json = self.api_get("/listshares", &[]).await?;

        let share_id = json["shares"]["outgoing"]
            .as_array()
            .and_then(|shares| {
                shares.iter().find_map(|s| {
                    if s["tomail"].as_str() == Some(member_id)
                        && s["folderid"].as_u64() == Some(self.folder_id)
                    {
                        s["shareid"].as_u64()
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| CloudHomeError::Storage(format!("no share found for {member_id}")))?;

        let share_id_str = share_id.to_string();
        self.api_get("/removeshare", &[("shareid", &share_id_str)])
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_replaces_slashes() {
        assert_eq!(
            PCloudCloudHome::encode_key("changes/dev1/42.enc"),
            "changes__dev1__42.enc"
        );
    }

    #[test]
    fn decode_key_restores_slashes() {
        assert_eq!(
            PCloudCloudHome::decode_key("changes__dev1__42.enc"),
            "changes/dev1/42.enc"
        );
    }

    #[test]
    fn encode_decode_roundtrip() {
        let keys = [
            "snapshot.db.enc",
            "changes/device-abc/1.enc",
            "heads/device-abc.json.enc",
            "images/cover.jpg",
        ];
        for key in keys {
            let encoded = PCloudCloudHome::encode_key(key);
            let decoded = PCloudCloudHome::decode_key(&encoded);
            assert_eq!(decoded, key);
        }
    }

    #[test]
    fn encode_prefix_for_query() {
        assert_eq!(
            PCloudCloudHome::encode_prefix("changes/dev1/"),
            "changes__dev1__"
        );
    }

    #[test]
    fn api_host_from_location_us() {
        assert_eq!(
            PCloudCloudHome::api_host_from_location_id(1),
            "api.pcloud.com"
        );
    }

    #[test]
    fn api_host_from_location_eu() {
        assert_eq!(
            PCloudCloudHome::api_host_from_location_id(2),
            "eapi.pcloud.com"
        );
    }

    #[test]
    fn api_host_from_location_unknown_defaults_to_us() {
        assert_eq!(
            PCloudCloudHome::api_host_from_location_id(99),
            "api.pcloud.com"
        );
    }

    #[test]
    fn oauth_config_urls() {
        let config = PCloudCloudHome::oauth_config();
        assert_eq!(config.auth_url, "https://my.pcloud.com/oauth2/authorize");
        assert_eq!(config.token_url, "https://api.pcloud.com/oauth2_token");
        assert!(config.scopes.is_empty());
        // pCloud requires client_secret (no PKCE support)
        assert!(config.client_secret.is_some());
    }
}
