use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tracing::warn;

use crate::cloud_home::{CloudHome, CloudHomeError};

pub struct CloudRouteState {
    pub cloud_home: Arc<dyn CloudHome>,
}

#[derive(Deserialize)]
struct ListQuery {
    prefix: String,
}

#[derive(Deserialize)]
struct ShareManifest {
    files: Vec<String>,
}

pub fn create_cloud_router(state: Arc<CloudRouteState>) -> Router {
    Router::new()
        .route("/cloud", get(list_keys))
        .route("/cloud/*key", get(read_key).head(head_key))
        .route("/share/:share_id/meta", get(share_meta))
        .route("/share/:share_id/manifest", get(share_manifest))
        .route("/share/:share_id/file/*key", get(share_file))
        .with_state(state)
}

fn cloud_error_to_response(err: CloudHomeError) -> Response {
    match err {
        CloudHomeError::NotFound(_) => StatusCode::NOT_FOUND.into_response(),
        CloudHomeError::Storage(msg) => {
            warn!("Cloud home storage error: {msg}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        CloudHomeError::Io(err) => {
            warn!("Cloud home I/O error: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
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

async fn list_keys(
    State(state): State<Arc<CloudRouteState>>,
    Query(query): Query<ListQuery>,
) -> Response {
    match state.cloud_home.list(&query.prefix).await {
        Ok(keys) => {
            let json = serde_json::to_string(&keys).unwrap();
            (StatusCode::OK, [("content-type", "application/json")], json).into_response()
        }
        Err(err) => cloud_error_to_response(err),
    }
}

async fn read_key(
    State(state): State<Arc<CloudRouteState>>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    if let Some(range_header) = headers.get("range").and_then(|v| v.to_str().ok()) {
        if let Some((start, end_inclusive)) = parse_range_header(range_header) {
            // CloudHome::read_range takes start inclusive, end exclusive
            let end_exclusive = end_inclusive + 1;
            return match state
                .cloud_home
                .read_range(&key, start, end_exclusive)
                .await
            {
                Ok(data) => (StatusCode::PARTIAL_CONTENT, data).into_response(),
                Err(err) => cloud_error_to_response(err),
            };
        }
    }

    match state.cloud_home.read(&key).await {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

async fn head_key(State(state): State<Arc<CloudRouteState>>, Path(key): Path<String>) -> Response {
    match state.cloud_home.exists(&key).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

fn cors_headers() -> [(&'static str, &'static str); 1] {
    [("access-control-allow-origin", "*")]
}

async fn share_meta(
    State(state): State<Arc<CloudRouteState>>,
    Path(share_id): Path<String>,
) -> Response {
    let key = format!("shares/{share_id}/meta.enc");
    match state.cloud_home.read(&key).await {
        Ok(data) => (
            StatusCode::OK,
            [
                ("content-type", "application/octet-stream"),
                ("access-control-allow-origin", "*"),
            ],
            data,
        )
            .into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

async fn share_manifest(
    State(state): State<Arc<CloudRouteState>>,
    Path(share_id): Path<String>,
) -> Response {
    let key = format!("shares/{share_id}/manifest.json");
    match state.cloud_home.read(&key).await {
        Ok(data) => (
            StatusCode::OK,
            [
                ("content-type", "application/json"),
                ("access-control-allow-origin", "*"),
            ],
            data,
        )
            .into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

async fn share_file(
    State(state): State<Arc<CloudRouteState>>,
    headers: HeaderMap,
    Path((share_id, key)): Path<(String, String)>,
) -> Response {
    // Read manifest to validate the requested key
    let manifest_key = format!("shares/{share_id}/manifest.json");
    let manifest_data = match state.cloud_home.read(&manifest_key).await {
        Ok(data) => data,
        Err(CloudHomeError::NotFound(_)) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => return cloud_error_to_response(err),
    };

    let manifest: ShareManifest = match serde_json::from_slice(&manifest_data) {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to parse manifest for share {share_id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !manifest.files.contains(&key) {
        return (StatusCode::FORBIDDEN, "file not in share manifest").into_response();
    }

    // Serve the file with range support and CORS
    if let Some(range_header) = headers.get("range").and_then(|v| v.to_str().ok()) {
        if let Some((start, end_inclusive)) = parse_range_header(range_header) {
            let end_exclusive = end_inclusive + 1;
            return match state
                .cloud_home
                .read_range(&key, start, end_exclusive)
                .await
            {
                Ok(data) => (StatusCode::PARTIAL_CONTENT, cors_headers(), data).into_response(),
                Err(err) => cloud_error_to_response(err),
            };
        }
    }

    match state.cloud_home.read(&key).await {
        Ok(data) => (
            StatusCode::OK,
            [
                ("content-type", "application/octet-stream"),
                ("access-control-allow-origin", "*"),
            ],
            data,
        )
            .into_response(),
        Err(err) => cloud_error_to_response(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_valid() {
        assert_eq!(parse_range_header("bytes=0-499"), Some((0, 499)));
        assert_eq!(parse_range_header("bytes=100-200"), Some((100, 200)));
        assert_eq!(parse_range_header("bytes=0-0"), Some((0, 0)));
    }

    #[test]
    fn parse_range_invalid() {
        assert_eq!(parse_range_header("bytes=500-100"), None);
        assert_eq!(parse_range_header("bytes=abc-def"), None);
        assert_eq!(parse_range_header("invalid"), None);
        assert_eq!(parse_range_header("bytes="), None);
        assert_eq!(parse_range_header("bytes=100-"), None);
    }
}
