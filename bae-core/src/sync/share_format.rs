use serde::{Deserialize, Serialize};

/// Metadata for a shared album, encrypted with per-share key and stored as `shares/{share_id}/meta.enc`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShareMeta {
    pub album_name: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover_image_key: Option<String>,
    pub tracks: Vec<ShareMetaTrack>,
    /// Base64-encoded 32-byte per-release encryption key.
    pub release_key_b64: String,
}

/// A track within shared album metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShareMetaTrack {
    pub number: Option<i32>,
    pub title: String,
    pub duration_secs: Option<i64>,
    pub file_key: String,
    /// Audio format: "flac", "mp3", "ogg", "wav", "aac", "m4a"
    pub format: String,
}

/// Manifest listing S3 keys that bae-proxy serves publicly for a share.
/// Stored unencrypted as `shares/{share_id}/manifest.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShareManifest {
    pub files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_meta_serde_roundtrip() {
        let meta = ShareMeta {
            album_name: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            year: Some(2024),
            cover_image_key: Some("images/ab/cd/img-id".to_string()),
            tracks: vec![ShareMetaTrack {
                number: Some(1),
                title: "Track One".to_string(),
                duration_secs: Some(240),
                file_key: "storage/ab/cd/file-id".to_string(),
                format: "flac".to_string(),
            }],
            release_key_b64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                [0u8; 32],
            ),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: ShareMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.album_name, "Test Album");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].format, "flac");
    }

    #[test]
    fn share_manifest_serde_roundtrip() {
        let manifest = ShareManifest {
            files: vec![
                "storage/ab/cd/file-1".to_string(),
                "storage/ab/cd/file-2".to_string(),
                "images/ab/cd/img-1".to_string(),
            ],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ShareManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.files.len(), 3);
    }
}
