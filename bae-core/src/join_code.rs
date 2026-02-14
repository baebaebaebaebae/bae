use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::cloud_home::JoinInfo;

#[derive(Serialize, Deserialize)]
pub struct InviteCode {
    pub library_id: String,
    pub library_name: String,
    pub join_info: JoinInfo,
    pub owner_pubkey: String,
}

pub fn encode(code: &InviteCode) -> String {
    let json = serde_json::to_vec(code).expect("InviteCode is always serializable");
    URL_SAFE_NO_PAD.encode(&json)
}

pub fn decode(s: &str) -> Result<InviteCode, JoinCodeError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s.trim())
        .map_err(|_| JoinCodeError::InvalidBase64)?;
    serde_json::from_slice(&bytes).map_err(|e| JoinCodeError::InvalidJson(e.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum JoinCodeError {
    #[error("invalid base64url encoding")]
    InvalidBase64,
    #[error("invalid invite code payload: {0}")]
    InvalidJson(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_s3() {
        let code = InviteCode {
            library_id: "lib-123".into(),
            library_name: "My Library".into(),
            join_info: JoinInfo::S3 {
                bucket: "my-bucket".into(),
                region: "us-east-1".into(),
                endpoint: None,
                access_key: "AKIAEXAMPLE".into(),
                secret_key: "secret123".into(),
            },
            owner_pubkey: "deadbeef".into(),
        };
        let encoded = encode(&code);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.library_id, "lib-123");
        assert_eq!(decoded.library_name, "My Library");
        assert_eq!(decoded.owner_pubkey, "deadbeef");
        match decoded.join_info {
            JoinInfo::S3 {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(region, "us-east-1");
                assert_eq!(endpoint, None);
                assert_eq!(access_key, "AKIAEXAMPLE");
                assert_eq!(secret_key, "secret123");
            }
            _ => panic!("expected S3 variant"),
        }
    }

    #[test]
    fn round_trip_s3_with_endpoint() {
        let code = InviteCode {
            library_id: "lib-456".into(),
            library_name: "Shared".into(),
            join_info: JoinInfo::S3 {
                bucket: "bucket".into(),
                region: "eu-west-1".into(),
                endpoint: Some("https://s3.example.com".into()),
                access_key: "ak".into(),
                secret_key: "sk".into(),
            },
            owner_pubkey: "cafebabe".into(),
        };
        let encoded = encode(&code);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.library_id, "lib-456");
        match decoded.join_info {
            JoinInfo::S3 { endpoint, .. } => {
                assert_eq!(endpoint, Some("https://s3.example.com".to_string()));
            }
            _ => panic!("expected S3 variant"),
        }
    }

    #[test]
    fn round_trip_google_drive() {
        let code = InviteCode {
            library_id: "lib-789".into(),
            library_name: "Cloud Shared".into(),
            join_info: JoinInfo::GoogleDrive {
                folder_id: "abc123".into(),
            },
            owner_pubkey: "cafebabe".into(),
        };
        let encoded = encode(&code);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.library_id, "lib-789");
        match decoded.join_info {
            JoinInfo::GoogleDrive { folder_id } => assert_eq!(folder_id, "abc123"),
            _ => panic!("expected GoogleDrive variant"),
        }
    }

    #[test]
    fn decode_invalid_base64() {
        assert!(matches!(
            decode("not-valid!!!"),
            Err(JoinCodeError::InvalidBase64)
        ));
    }

    #[test]
    fn decode_invalid_json() {
        let encoded = URL_SAFE_NO_PAD.encode(b"not json");
        assert!(matches!(
            decode(&encoded),
            Err(JoinCodeError::InvalidJson(_))
        ));
    }

    #[test]
    fn decode_trims_whitespace() {
        let code = InviteCode {
            library_id: "lib-ws".into(),
            library_name: "Trimmed".into(),
            join_info: JoinInfo::Dropbox {
                shared_folder_id: "sf1".into(),
            },
            owner_pubkey: "aabb".into(),
        };
        let encoded = format!("  {} \n", encode(&code));
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.library_id, "lib-ws");
    }
}
