//! API client for bae-cloud (signup, login, provision, logout).

use serde::Deserialize;

const DEFAULT_API_URL: &str = "https://cloud.bae.fm";

/// Base URL for the bae-cloud API. Override with `BAE_CLOUD_API_URL` env var for dev/testing.
pub fn api_url() -> String {
    std::env::var("BAE_CLOUD_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

#[derive(Debug, Deserialize)]
pub struct SignupResponse {
    pub session_token: String,
    pub library_id: String,
    pub library_url: String,
    pub provisioned: bool,
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub session_token: String,
    pub library_id: String,
    pub library_url: String,
    pub provisioned: bool,
}

/// Create a new bae-cloud account.
pub async fn signup(email: &str, username: &str, password: &str) -> Result<SignupResponse, String> {
    let url = format!("{}/api/signup", api_url());
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({
            "email": email,
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if resp.status().is_success() {
        resp.json().await.map_err(|e| format!("parse error: {e}"))
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(body)
    }
}

/// Log in to an existing bae-cloud account.
pub async fn login(email: &str, password: &str) -> Result<LoginResponse, String> {
    let url = format!("{}/api/login", api_url());
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
        }))
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if resp.status().is_success() {
        resp.json().await.map_err(|e| format!("parse error: {e}"))
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(body)
    }
}

/// Provision the library with the user's Ed25519 public key.
///
/// The signature is over `"provision:{library_id}:{timestamp}"` where timestamp is Unix seconds.
pub async fn provision(
    session_token: &str,
    ed25519_pubkey: &str,
    signature: &str,
    timestamp: &str,
) -> Result<(), String> {
    let url = format!("{}/api/provision", api_url());
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(session_token)
        .json(&serde_json::json!({
            "ed25519_pubkey": ed25519_pubkey,
            "signature": signature,
            "timestamp": timestamp,
        }))
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(body)
    }
}

/// Invalidate the current session.
pub async fn logout(session_token: &str) -> Result<(), String> {
    let url = format!("{}/api/logout", api_url());
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(session_token)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_signup_response() {
        let json = r#"{
            "session_token": "tok-abc",
            "library_id": "lib-456",
            "library_url": "https://alice.bae.fm",
            "provisioned": false
        }"#;
        let resp: SignupResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.session_token, "tok-abc");
        assert_eq!(resp.library_id, "lib-456");
        assert_eq!(resp.library_url, "https://alice.bae.fm");
        assert!(!resp.provisioned);
    }

    #[test]
    fn parse_login_response() {
        let json = r#"{
            "session_token": "tok-xyz",
            "library_id": "lib-000",
            "library_url": "https://bob.bae.fm",
            "provisioned": true
        }"#;
        let resp: LoginResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.session_token, "tok-xyz");
        assert_eq!(resp.library_url, "https://bob.bae.fm");
        assert!(resp.provisioned);
    }

    #[test]
    fn parse_login_response_not_provisioned() {
        let json = r#"{
            "session_token": "tok-new",
            "library_id": "lib-new",
            "library_url": "https://new.bae.fm",
            "provisioned": false
        }"#;
        let resp: LoginResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.provisioned);
    }
}
