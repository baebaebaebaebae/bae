//! Write proxy endpoints for CloudHome operations.
//!
//! Proxies raw bytes to/from the backing CloudHome (S3, etc.) with
//! Ed25519 signature-based auth. No encryption/decryption -- that
//! happens client-side.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tracing::warn;

use bae_core::cloud_home::{CloudHome, CloudHomeError};
use bae_core::keys::verify_signature;
use bae_core::sodium_ffi;
use bae_core::sync::membership::MembershipChain;

/// The loaded state of the membership chain at server startup.
#[derive(Clone)]
pub enum ChainState {
    /// No membership entries exist (new library). Any valid signature is accepted.
    None,
    /// Entries exist and form a valid chain.
    Valid(MembershipChain),
    /// Entries exist but are corrupt or invalid. All write proxy requests are rejected.
    Invalid,
}

/// Shared state for the cloud proxy routes.
#[derive(Clone)]
pub struct CloudProxyState {
    pub cloud_home: Arc<dyn CloudHome>,
    pub chain_state: ChainState,
}

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    prefix: String,
}

/// Maximum allowed clock skew for timestamp verification (5 minutes).
const MAX_TIMESTAMP_SKEW_SECS: u64 = 300;

pub fn cloud_proxy_router(state: CloudProxyState) -> Router {
    Router::new()
        .route("/cloud", get(list_keys))
        .route(
            "/cloud/*key",
            get(read_key)
                .put(write_key)
                .delete(delete_key)
                .head(head_key),
        )
        .with_state(state)
}

/// Build a 401 Unauthorized response.
fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, msg.to_string()).into_response()
}

/// Build a 503 Service Unavailable response.
fn service_unavailable(msg: &str) -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, msg.to_string()).into_response()
}

/// Verify the Ed25519 auth headers on a request.
///
/// Returns Ok on success, or an error response on failure.
#[allow(clippy::result_large_err)]
fn verify_auth(
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    chain_state: &ChainState,
) -> Result<(), Response> {
    // Reject immediately if the membership chain is corrupt.
    if matches!(chain_state, ChainState::Invalid) {
        return Err(service_unavailable(
            "membership chain is corrupt, write proxy disabled",
        ));
    }

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

    // 1. Verify timestamp is within 5 minutes of server time.
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

    // 2. Decode the public key and signature.
    let pk_bytes: [u8; sodium_ffi::SIGN_PUBLICKEYBYTES] = hex::decode(pubkey_hex)
        .map_err(|_| unauthorized("invalid pubkey hex"))?
        .try_into()
        .map_err(|_| unauthorized("pubkey wrong length"))?;

    let sig_bytes: [u8; sodium_ffi::SIGN_BYTES] = hex::decode(signature_hex)
        .map_err(|_| unauthorized("invalid signature hex"))?
        .try_into()
        .map_err(|_| unauthorized("signature wrong length"))?;

    // 3. Verify the signature over "METHOD\nPATH\nTIMESTAMP".
    let message = format!("{}\n{}\n{}", method.as_str(), path, timestamp_str);

    if !verify_signature(&sig_bytes, message.as_bytes(), &pk_bytes) {
        return Err(unauthorized("invalid signature"));
    }

    // 4. Check membership chain (if one exists).
    if let ChainState::Valid(chain) = chain_state {
        let is_member = chain
            .current_members()
            .iter()
            .any(|(pk, _)| pk == pubkey_hex);

        if !is_member {
            return Err(unauthorized("not a member of this library"));
        }
    }
    // ChainState::None -- any valid signature is accepted.

    Ok(())
}

/// Map CloudHomeError to HTTP status code.
fn cloud_error_to_response(err: CloudHomeError) -> Response {
    match err {
        CloudHomeError::NotFound(msg) => (StatusCode::NOT_FOUND, msg).into_response(),
        CloudHomeError::Storage(msg) => {
            warn!("cloud storage error: {msg}");
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
        CloudHomeError::Io(err) => {
            warn!("cloud I/O error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

/// GET /cloud?prefix={p} -- list keys under a prefix.
async fn list_keys(
    State(state): State<CloudProxyState>,
    headers: HeaderMap,
    method: Method,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(resp) = verify_auth(&headers, &method, "/cloud", &state.chain_state) {
        return resp;
    }

    match state.cloud_home.list(&query.prefix).await {
        Ok(keys) => {
            let json = serde_json::to_string(&keys).unwrap();
            (StatusCode::OK, [("content-type", "application/json")], json).into_response()
        }
        Err(err) => cloud_error_to_response(err),
    }
}

/// GET /cloud/*key -- read a key (with optional Range header).
async fn read_key(
    State(state): State<CloudProxyState>,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &state.chain_state) {
        return resp;
    }

    // Check for Range header.
    if let Some(range_header) = headers.get("range").and_then(|v| v.to_str().ok()) {
        if let Some(range) = parse_range_header(range_header) {
            // CloudHome uses exclusive end.
            match state
                .cloud_home
                .read_range(&key, range.0, range.1 + 1)
                .await
            {
                Ok(data) => {
                    return (StatusCode::PARTIAL_CONTENT, data).into_response();
                }
                Err(err) => return cloud_error_to_response(err),
            }
        }
    }

    match state.cloud_home.read(&key).await {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

/// PUT /cloud/*key -- write a key.
async fn write_key(
    State(state): State<CloudProxyState>,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
    body: Bytes,
) -> Response {
    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &state.chain_state) {
        return resp;
    }

    match state.cloud_home.write(&key, body.to_vec()).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

/// DELETE /cloud/*key -- delete a key.
async fn delete_key(
    State(state): State<CloudProxyState>,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &state.chain_state) {
        return resp;
    }

    match state.cloud_home.delete(&key).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

/// HEAD /cloud/*key -- check if a key exists.
async fn head_key(
    State(state): State<CloudProxyState>,
    headers: HeaderMap,
    method: Method,
    Path(key): Path<String>,
) -> Response {
    let request_path = format!("/cloud/{key}");
    if let Err(resp) = verify_auth(&headers, &method, &request_path, &state.chain_state) {
        return resp;
    }

    match state.cloud_home.exists(&key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => cloud_error_to_response(err),
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

/// Load the membership chain from the sync bucket.
///
/// Returns:
/// - `Ok(None)` if no membership entries exist (new library).
/// - `Ok(Some(chain))` if entries exist and form a valid chain.
/// - `Err(reason)` if entries exist but are corrupt or invalid.
pub async fn load_membership_chain(
    bucket: &dyn bae_core::sync::bucket::SyncBucketClient,
) -> Result<Option<MembershipChain>, String> {
    let entry_keys = bucket
        .list_membership_entries()
        .await
        .map_err(|e| format!("failed to list membership entries: {e}"))?;

    if entry_keys.is_empty() {
        return Ok(None);
    }

    let mut raw_entries = Vec::new();
    for (author, seq) in &entry_keys {
        let data = bucket
            .get_membership_entry(author, *seq)
            .await
            .map_err(|e| format!("failed to get membership entry {author}/{seq}: {e}"))?;

        let entry: bae_core::sync::membership::MembershipEntry = serde_json::from_slice(&data)
            .map_err(|e| format!("failed to parse membership entry {author}/{seq}: {e}"))?;

        raw_entries.push(entry);
    }

    let chain = MembershipChain::from_entries(raw_entries)
        .map_err(|e| format!("invalid membership chain: {e}"))?;

    Ok(Some(chain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_header_valid() {
        assert_eq!(parse_range_header("bytes=0-499"), Some((0, 499)));
        assert_eq!(parse_range_header("bytes=100-200"), Some((100, 200)));
        assert_eq!(parse_range_header("bytes=0-0"), Some((0, 0)));
    }

    #[test]
    fn parse_range_header_invalid() {
        assert_eq!(parse_range_header("bytes=500-100"), None); // start > end
        assert_eq!(parse_range_header("bytes=abc-def"), None);
        assert_eq!(parse_range_header("invalid"), None);
        assert_eq!(parse_range_header("bytes="), None);
        assert_eq!(parse_range_header("bytes=100-"), None);
    }
}
