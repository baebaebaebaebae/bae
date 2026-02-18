use serde::Deserialize;

/// Manifest listing S3 keys that are publicly readable for a share.
#[derive(Deserialize)]
pub struct ShareManifest {
    pub files: Vec<String>,
}
