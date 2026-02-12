use crate::hmac_utils::{hmac_sign, hmac_verify};
use crate::library::SharedLibraryManager;
use crate::library_dir::LibraryDir;
use axum::{
    extract::{Path, Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::path::Path as StdPath;
use tracing::{debug, warn};

#[derive(Clone)]
struct ImageServerState {
    library_manager: SharedLibraryManager,
    library_dir: LibraryDir,
    secret: [u8; 32],
}

/// Connection details for the running image server.
#[derive(Clone)]
pub struct ImageServerHandle {
    pub host: String,
    pub port: u16,
    secret: [u8; 32],
    library_dir: LibraryDir,
}

impl ImageServerHandle {
    /// URL for a library image (cover or artist photo) by its id.
    pub fn image_url(&self, id: &str) -> String {
        let path = format!("/image/{}", id);
        let sig = sign(&self.secret, &path);
        format!("http://{}:{}{path}?sig={sig}", self.host, self.port)
    }

    /// URL for a library image, but only if the image file actually exists on disk.
    pub fn image_url_if_exists(&self, id: &str) -> Option<String> {
        if self.library_dir.image_path(id).exists() {
            Some(self.image_url(id))
        } else {
            None
        }
    }

    pub fn file_url(&self, file_id: &str) -> String {
        let path = format!("/file/{}", file_id);
        let sig = sign(&self.secret, &path);
        format!("http://{}:{}{path}?sig={sig}", self.host, self.port)
    }

    pub fn local_file_url(&self, path: &StdPath) -> String {
        let encoded_segments: Vec<String> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .map(|s| urlencoding::encode(s).into_owned())
            .collect();
        let route_path = format!("/local/{}", encoded_segments.join("/"));
        let sig = sign(&self.secret, &route_path);
        format!("http://{}:{}{route_path}?sig={sig}", self.host, self.port)
    }
}

/// Start the image server on a random port.
/// Returns a handle with host, port, and signing secret.
pub async fn start_image_server(
    library_manager: SharedLibraryManager,
    library_dir: LibraryDir,
    host: &str,
) -> ImageServerHandle {
    let mut secret = [0u8; 32];
    secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());

    let state = ImageServerState {
        library_manager,
        library_dir: library_dir.clone(),
        secret,
    };

    let app = Router::new()
        .route("/image/:id", get(handle_image))
        .route("/file/:file_id", get(handle_file))
        .route("/local/*path", get(handle_local_file))
        .layer(middleware::from_fn_with_state(state.clone(), verify_sig))
        .with_state(state);

    let bind_addr = format!("{}:0", host);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind image server");
    let port = listener.local_addr().unwrap().port();

    tracing::info!("Image server listening on http://{}:{}", host, port);

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    ImageServerHandle {
        host: host.to_string(),
        port,
        secret,
        library_dir,
    }
}

// =============================================================================
// HMAC signing / verification
// =============================================================================

fn sign(secret: &[u8; 32], path: &str) -> String {
    hex::encode(hmac_sign(secret, path.as_bytes()))
}

fn verify(secret: &[u8; 32], path: &str, sig: &str) -> bool {
    let Ok(sig_bytes) = hex::decode(sig) else {
        return false;
    };
    hmac_verify(secret, path.as_bytes(), &sig_bytes)
}

async fn verify_sig(
    State(state): State<ImageServerState>,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let path = request.uri().path();
    match params.get("sig") {
        Some(sig) if verify(&state.secret, path, sig) => next.run(request).await,
        _ => StatusCode::FORBIDDEN.into_response(),
    }
}

// =============================================================================
// Handlers
// =============================================================================

/// Unified handler for all library images (covers and artist photos).
/// Looks up the `library_images` row by id and serves from `images/ab/cd/{id}`.
async fn handle_image(
    State(state): State<ImageServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let image_path = state.library_dir.image_path(&id);

    let content_type = match state
        .library_manager
        .get()
        .get_library_image_by_id(&id)
        .await
    {
        Ok(Some(img)) => img.content_type.to_string(),
        Ok(None) => {
            warn!("No library_image row for id {}", id);
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("DB error looking up image {}: {}", id, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match tokio::fs::read(&image_path).await {
        Ok(data) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, content_type)],
            data,
        )
            .into_response(),
        Err(_) => {
            warn!("Image not found for id {}", id);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn handle_file(
    State(state): State<ImageServerState>,
    Path(file_id): Path<String>,
) -> impl IntoResponse {
    if file_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing file ID").into_response();
    }

    debug!("Serving file: {}", file_id);

    let file = match state.library_manager.get().get_file_by_id(&file_id).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            warn!("File not found: {}", file_id);
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("Database error looking up file {}: {}", file_id, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let source_path = match &file.source_path {
        Some(p) => p.clone(),
        None => {
            warn!("File {} has no source_path", file_id);
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    match tokio::fs::read(&source_path).await {
        Ok(data) => {
            let content_type = file.content_type.to_string();
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, content_type)],
                data,
            )
                .into_response()
        }
        Err(e) => {
            warn!("Failed to read file {}: {}", source_path, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn handle_local_file(Path(encoded_path): Path<String>) -> impl IntoResponse {
    let path: String = encoded_path
        .split('/')
        .map(|segment| {
            urlencoding::decode(segment)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| segment.to_string())
        })
        .collect::<Vec<_>>()
        .join("/");
    let path = format!("/{}", path);

    match tokio::fs::read(&path).await {
        Ok(data) => {
            let content_type = StdPath::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map(crate::content_type::ContentType::from_extension)
                .unwrap_or(crate::content_type::ContentType::OctetStream);

            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    content_type.as_str().to_string(),
                )],
                data,
            )
                .into_response()
        }
        Err(e) => {
            warn!("Failed to read local file {}: {}", path, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handle() -> ImageServerHandle {
        ImageServerHandle {
            host: "127.0.0.1".to_string(),
            port: 8080,
            secret: [0xAB; 32],
            library_dir: LibraryDir::new(std::path::PathBuf::from("/tmp/test")),
        }
    }

    #[test]
    fn image_url_has_sig() {
        let h = test_handle();
        let url = h.image_url("abc");
        assert!(url.starts_with("http://127.0.0.1:8080/image/abc?sig="));
        assert_eq!(url.split("sig=").count(), 2);
    }

    #[test]
    fn file_url_has_sig() {
        let h = test_handle();
        let url = h.file_url("f1");
        assert!(url.starts_with("http://127.0.0.1:8080/file/f1?sig="));
    }

    #[test]
    fn local_file_url_has_sig() {
        let h = test_handle();
        let url = h.local_file_url(StdPath::new("/a/b/c.jpg"));
        assert!(url.starts_with("http://127.0.0.1:8080/local/a/b/c.jpg?sig="));
    }

    #[test]
    fn local_file_url_encodes_spaces() {
        let h = test_handle();
        let url = h.local_file_url(StdPath::new("/a/b b/c.jpg"));
        assert!(url.contains("/local/a/b%20b/c.jpg?sig="));
    }

    #[test]
    fn local_file_url_encodes_special_chars() {
        let h = test_handle();
        let url = h.local_file_url(StdPath::new("/a/b's (1,2)/c.jpg"));
        assert!(url.contains("/local/a/b%27s%20%281%2C2%29/c.jpg?sig="));
    }

    #[test]
    fn sign_verify_roundtrip() {
        let secret = [0x42; 32];
        let path = "/image/abc";
        let sig = sign(&secret, path);
        assert!(verify(&secret, path, &sig));
    }

    #[test]
    fn verify_rejects_wrong_sig() {
        let secret = [0x42; 32];
        assert!(!verify(&secret, "/image/abc", "deadbeef"));
    }

    #[test]
    fn verify_rejects_wrong_path() {
        let secret = [0x42; 32];
        let sig = sign(&secret, "/image/abc");
        assert!(!verify(&secret, "/image/xyz", &sig));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let secret_a = [0x42; 32];
        let secret_b = [0x99; 32];
        let sig = sign(&secret_a, "/image/abc");
        assert!(!verify(&secret_b, "/image/abc", &sig));
    }
}
