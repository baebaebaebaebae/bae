use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Host, Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::warn;

use crate::registry::Registry;
use crate::s3::{S3Client, S3Error};

pub struct ProxyState {
    pub registry: Arc<RwLock<Registry>>,
    pub s3_clients: Arc<RwLock<HashMap<String, S3Client>>>,
}

#[derive(Deserialize)]
struct ListQuery {
    prefix: String,
}

/// Maximum allowed clock skew for timestamp verification (5 minutes).
const MAX_TIMESTAMP_SKEW_SECS: u64 = 300;

pub fn proxy_router(state: Arc<ProxyState>) -> Router {
    Router::new()
        .route("/cloud", get(list_keys))
        .route(
            "/cloud/{*key}",
            get(read_key)
                .put(write_key)
                .delete(delete_key)
                .head(head_key),
        )
        .route("/health", get(health))
        .with_state(state)
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, msg.to_string()).into_response()
}

fn s3_error_to_response(err: S3Error) -> Response {
    match err {
        S3Error::NotFound => StatusCode::NOT_FOUND.into_response(),
        S3Error::Other(msg) => {
            warn!("S3 error: {msg}");
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
    }
}

/// Extract hostname from the Host header, stripping any port.
fn extract_hostname(host: &str) -> &str {
    // Handle IPv6 addresses like [::1]:8080
    if host.starts_with('[') {
        return host;
    }
    host.split(':').next().unwrap_or(host)
}

/// Verify Ed25519 auth headers on a request.
///
/// Expects:
/// - `X-Bae-Pubkey`: hex-encoded 32-byte Ed25519 public key
/// - `X-Bae-Timestamp`: Unix timestamp (seconds)
/// - `X-Bae-Signature`: hex-encoded 64-byte Ed25519 signature
///
/// Signature is over `"METHOD\nPATH\nTIMESTAMP"`.
#[allow(clippy::result_large_err)]
pub fn verify_auth(
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    expected_pubkey_hex: &str,
) -> Result<(), Response> {
    let pubkey_hex = headers
        .get("x-bae-pubkey")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("missing X-Bae-Pubkey header"))?;

    let timestamp_str = headers
        .get("x-bae-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("missing X-Bae-Timestamp header"))?;

    let signature_hex = headers
        .get("x-bae-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("missing X-Bae-Signature header"))?;

    // 1. Verify timestamp within 5 minutes of server time.
    let timestamp: u64 = timestamp_str
        .parse()
        .map_err(|_| unauthorized("invalid timestamp format"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now.abs_diff(timestamp) > MAX_TIMESTAMP_SKEW_SECS {
        return Err(unauthorized("timestamp too far from server time"));
    }

    // 2. Check that the presented pubkey matches the expected one from the registry.
    if pubkey_hex != expected_pubkey_hex {
        return Err(unauthorized("pubkey does not match registered key"));
    }

    // 3. Decode pubkey and signature.
    let pk_bytes: [u8; 32] = hex::decode(pubkey_hex)
        .map_err(|_| unauthorized("invalid pubkey hex"))?
        .try_into()
        .map_err(|_| unauthorized("pubkey wrong length"))?;

    let sig_bytes: [u8; 64] = hex::decode(signature_hex)
        .map_err(|_| unauthorized("invalid signature hex"))?
        .try_into()
        .map_err(|_| unauthorized("signature wrong length"))?;

    let verifying_key =
        VerifyingKey::from_bytes(&pk_bytes).map_err(|_| unauthorized("invalid pubkey"))?;

    let signature = Signature::from_bytes(&sig_bytes);

    // 4. Verify signature over "METHOD\nPATH\nTIMESTAMP".
    let message = format!("{}\n{}\n{}", method.as_str(), path, timestamp_str);

    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| unauthorized("invalid signature"))?;

    Ok(())
}

/// Get or create an S3Client for a library.
async fn get_s3_client(
    s3_clients: &RwLock<HashMap<String, S3Client>>,
    library_id: &str,
    entry: &crate::registry::LibraryEntry,
) -> Result<(), Response> {
    // Check if we already have a client.
    {
        let clients = s3_clients.read().await;
        if clients.contains_key(library_id) {
            return Ok(());
        }
    }

    // Create a new client.
    let client = S3Client::new(entry).await.map_err(|e| {
        warn!("failed to create S3 client for {library_id}: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
    })?;

    let mut clients = s3_clients.write().await;
    clients.entry(library_id.to_string()).or_insert(client);
    Ok(())
}

async fn health(State(state): State<Arc<ProxyState>>) -> Response {
    let registry = state.registry.read().await;
    let count = registry.libraries.len();

    let body = serde_json::json!({
        "status": "ok",
        "libraries": count,
    });

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

async fn list_keys(
    State(state): State<Arc<ProxyState>>,
    Host(raw_host): Host,
    headers: HeaderMap,
    method: Method,
    Query(query): Query<ListQuery>,
) -> Response {
    let hostname = extract_hostname(&raw_host);

    let registry = state.registry.read().await;
    let entry = match registry.find_by_hostname(hostname) {
        Some(e) => e.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    drop(registry);

    if let Some(ref pubkey) = entry.ed25519_pubkey {
        if let Err(resp) = verify_auth(&headers, &method, "/cloud", pubkey) {
            return resp;
        }
    }

    if let Err(resp) = get_s3_client(&state.s3_clients, &entry.library_id, &entry).await {
        return resp;
    }

    let clients = state.s3_clients.read().await;
    let client = clients.get(&entry.library_id).unwrap();

    match client.list_objects(&query.prefix).await {
        Ok(keys) => {
            let json = serde_json::to_string(&keys).unwrap();
            (StatusCode::OK, [("content-type", "application/json")], json).into_response()
        }
        Err(err) => s3_error_to_response(err),
    }
}

async fn read_key(
    State(state): State<Arc<ProxyState>>,
    Host(raw_host): Host,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let hostname = extract_hostname(&raw_host);

    let registry = state.registry.read().await;
    let entry = match registry.find_by_hostname(hostname) {
        Some(e) => e.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    drop(registry);

    let request_path = format!("/cloud/{key}");

    if let Some(ref pubkey) = entry.ed25519_pubkey {
        if let Err(resp) = verify_auth(&headers, &method, &request_path, pubkey) {
            return resp;
        }
    }

    if let Err(resp) = get_s3_client(&state.s3_clients, &entry.library_id, &entry).await {
        return resp;
    }

    let clients = state.s3_clients.read().await;
    let client = clients.get(&entry.library_id).unwrap();

    // Check for Range header.
    if let Some(range_header) = headers.get("range").and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range_header(range_header) {
            return match client.get_object_range(&key, start, end).await {
                Ok(data) => (StatusCode::PARTIAL_CONTENT, data).into_response(),
                Err(err) => s3_error_to_response(err),
            };
        }
    }

    match client.get_object(&key).await {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(err) => s3_error_to_response(err),
    }
}

async fn write_key(
    State(state): State<Arc<ProxyState>>,
    Host(raw_host): Host,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
    body: Bytes,
) -> Response {
    let hostname = extract_hostname(&raw_host);

    let registry = state.registry.read().await;
    let entry = match registry.find_by_hostname(hostname) {
        Some(e) => e.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    drop(registry);

    let pubkey = match &entry.ed25519_pubkey {
        Some(pk) => pk.clone(),
        None => {
            return (StatusCode::FORBIDDEN, "library not provisioned").into_response();
        }
    };

    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &pubkey) {
        return resp;
    }

    if let Err(resp) = get_s3_client(&state.s3_clients, &entry.library_id, &entry).await {
        return resp;
    }

    let clients = state.s3_clients.read().await;
    let client = clients.get(&entry.library_id).unwrap();

    match client.put_object(&key, body.to_vec()).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(err) => s3_error_to_response(err),
    }
}

async fn delete_key(
    State(state): State<Arc<ProxyState>>,
    Host(raw_host): Host,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let hostname = extract_hostname(&raw_host);

    let registry = state.registry.read().await;
    let entry = match registry.find_by_hostname(hostname) {
        Some(e) => e.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    drop(registry);

    let pubkey = match &entry.ed25519_pubkey {
        Some(pk) => pk.clone(),
        None => {
            return (StatusCode::FORBIDDEN, "library not provisioned").into_response();
        }
    };

    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &pubkey) {
        return resp;
    }

    if let Err(resp) = get_s3_client(&state.s3_clients, &entry.library_id, &entry).await {
        return resp;
    }

    let clients = state.s3_clients.read().await;
    let client = clients.get(&entry.library_id).unwrap();

    match client.delete_object(&key).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(err) => s3_error_to_response(err),
    }
}

async fn head_key(
    State(state): State<Arc<ProxyState>>,
    Host(raw_host): Host,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let hostname = extract_hostname(&raw_host);

    let registry = state.registry.read().await;
    let entry = match registry.find_by_hostname(hostname) {
        Some(e) => e.clone(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    drop(registry);

    let request_path = format!("/cloud/{key}");

    if let Some(ref pubkey) = entry.ed25519_pubkey {
        if let Err(resp) = verify_auth(&headers, &method, &request_path, pubkey) {
            return resp;
        }
    }

    if let Err(resp) = get_s3_client(&state.s3_clients, &entry.library_id, &entry).await {
        return resp;
    }

    let clients = state.s3_clients.read().await;
    let client = clients.get(&entry.library_id).unwrap();

    match client.head_object(&key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => s3_error_to_response(err),
    }
}

/// Parse a `Range: bytes=START-END` header.
/// Returns (start, end) where both are inclusive, or None if unparseable.
fn parse_range_header(header: &str) -> Option<(u64, u64)> {
    let range_spec = header.strip_prefix("bytes=")?;
    let (start_str, end_str) = range_spec.split_once('-')?;

    let start: u64 = start_str.parse().ok()?;
    let end: u64 = end_str.parse().ok()?;

    if start > end {
        return None;
    }

    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use ed25519_dalek::Signer;

    fn make_auth_headers(pubkey_hex: &str, timestamp: &str, signature_hex: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-bae-pubkey", pubkey_hex.parse().unwrap());
        headers.insert("x-bae-timestamp", timestamp.parse().unwrap());
        headers.insert("x-bae-signature", signature_hex.parse().unwrap());
        headers
    }

    fn current_timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    #[test]
    fn verify_auth_valid_signature() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.as_bytes());

        let timestamp = current_timestamp();
        let method = Method::PUT;
        let path = "/cloud/changes/dev1/42.enc";
        let message = format!("{}\n{}\n{}", method.as_str(), path, timestamp);
        let signature = signing_key.sign(message.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let headers = make_auth_headers(&pubkey_hex, &timestamp, &sig_hex);
        let result = verify_auth(&headers, &method, path, &pubkey_hex);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_auth_expired_timestamp() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.as_bytes());

        // Timestamp 10 minutes in the past.
        let old_timestamp = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 600)
            .to_string();

        let method = Method::PUT;
        let path = "/cloud/test.enc";
        let message = format!("{}\n{}\n{}", method.as_str(), path, old_timestamp);
        let signature = signing_key.sign(message.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let headers = make_auth_headers(&pubkey_hex, &old_timestamp, &sig_hex);
        let result = verify_auth(&headers, &method, path, &pubkey_hex);
        assert!(result.is_err());
    }

    #[test]
    fn verify_auth_wrong_pubkey() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.as_bytes());

        // Generate a different keypair for the "expected" pubkey.
        let other_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let other_pubkey_hex = hex::encode(other_key.verifying_key().as_bytes());

        let timestamp = current_timestamp();
        let method = Method::PUT;
        let path = "/cloud/test.enc";
        let message = format!("{}\n{}\n{}", method.as_str(), path, timestamp);
        let signature = signing_key.sign(message.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let headers = make_auth_headers(&pubkey_hex, &timestamp, &sig_hex);
        // Expected pubkey differs from the one in the header.
        let result = verify_auth(&headers, &method, path, &other_pubkey_hex);
        assert!(result.is_err());
    }

    #[test]
    fn verify_auth_invalid_signature() {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_hex = hex::encode(verifying_key.as_bytes());

        let timestamp = current_timestamp();
        let method = Method::PUT;
        let path = "/cloud/test.enc";

        // Sign a different message than what verify_auth expects.
        let wrong_message = "wrong message";
        let signature = signing_key.sign(wrong_message.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let headers = make_auth_headers(&pubkey_hex, &timestamp, &sig_hex);
        let result = verify_auth(&headers, &method, path, &pubkey_hex);
        assert!(result.is_err());
    }

    #[test]
    fn verify_auth_missing_headers() {
        let method = Method::PUT;
        let path = "/cloud/test.enc";
        let pubkey = "aa".repeat(32);

        // No headers at all.
        let empty = HeaderMap::new();
        assert!(verify_auth(&empty, &method, path, &pubkey).is_err());

        // Only pubkey header.
        let mut partial = HeaderMap::new();
        partial.insert("x-bae-pubkey", pubkey.parse().unwrap());
        assert!(verify_auth(&partial, &method, path, &pubkey).is_err());

        // Pubkey + timestamp but no signature.
        partial.insert("x-bae-timestamp", "1700000000".parse().unwrap());
        assert!(verify_auth(&partial, &method, path, &pubkey).is_err());
    }

    #[test]
    fn parse_range_header_valid() {
        assert_eq!(parse_range_header("bytes=0-499"), Some((0, 499)));
        assert_eq!(parse_range_header("bytes=100-200"), Some((100, 200)));
        assert_eq!(parse_range_header("bytes=0-0"), Some((0, 0)));
    }

    #[test]
    fn parse_range_header_invalid() {
        assert_eq!(parse_range_header("bytes=500-100"), None);
        assert_eq!(parse_range_header("bytes=abc-def"), None);
        assert_eq!(parse_range_header("invalid"), None);
        assert_eq!(parse_range_header("bytes="), None);
        assert_eq!(parse_range_header("bytes=100-"), None);
    }
}
