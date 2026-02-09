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
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::Path as StdPath;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;

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
}

impl ImageServerHandle {
    pub fn cover_url(&self, release_id: &str) -> String {
        let path = format!("/cover/{}", release_id);
        let sig = sign(&self.secret, &path);
        format!("http://{}:{}{path}?sig={sig}", self.host, self.port)
    }

    pub fn artist_image_url(&self, artist_id: &str) -> String {
        let path = format!("/artist-image/{}", artist_id);
        let sig = sign(&self.secret, &path);
        format!("http://{}:{}{path}?sig={sig}", self.host, self.port)
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
        library_dir,
        secret,
    };

    let app = Router::new()
        .route("/cover/:release_id", get(handle_cover))
        .route("/artist-image/:artist_id", get(handle_artist_image))
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
    }
}

// =============================================================================
// HMAC signing / verification
// =============================================================================

fn sign(secret: &[u8; 32], path: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(path.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn verify(secret: &[u8; 32], path: &str, sig: &str) -> bool {
    let Ok(sig_bytes) = hex::decode(sig) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(path.as_bytes());
    mac.verify_slice(&sig_bytes).is_ok()
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

async fn handle_cover(
    State(state): State<ImageServerState>,
    Path(release_id): Path<String>,
) -> impl IntoResponse {
    let cover_path = state.library_dir.cover_path(&release_id);

    let content_type = match state
        .library_manager
        .get()
        .get_library_image(&release_id, &crate::db::LibraryImageType::Cover)
        .await
    {
        Ok(Some(img)) => img.content_type.to_string(),
        Ok(None) => {
            warn!("No library_image row for cover {}", release_id);
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("DB error looking up cover {}: {}", release_id, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match tokio::fs::read(&cover_path).await {
        Ok(data) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, content_type)],
            data,
        )
            .into_response(),
        Err(_) => {
            warn!("Cover not found for release {}", release_id);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn handle_artist_image(
    State(state): State<ImageServerState>,
    Path(artist_id): Path<String>,
) -> impl IntoResponse {
    let image_path = state.library_dir.artist_image_path(&artist_id);

    let content_type = match state
        .library_manager
        .get()
        .get_library_image(&artist_id, &crate::db::LibraryImageType::Artist)
        .await
    {
        Ok(Some(img)) => img.content_type.to_string(),
        Ok(None) => {
            warn!("No library_image row for artist {}", artist_id);
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("DB error looking up artist image {}: {}", artist_id, e);
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
            warn!("Artist image not found for {}", artist_id);
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
        }
    }

    #[test]
    fn cover_url_has_sig() {
        let h = test_handle();
        let url = h.cover_url("abc");
        assert!(url.starts_with("http://127.0.0.1:8080/cover/abc?sig="));
        assert_eq!(url.split("sig=").count(), 2);
    }

    #[test]
    fn artist_image_url_has_sig() {
        let h = test_handle();
        let url = h.artist_image_url("xyz");
        assert!(url.starts_with("http://127.0.0.1:8080/artist-image/xyz?sig="));
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
        let path = "/cover/abc";
        let sig = sign(&secret, path);
        assert!(verify(&secret, path, &sig));
    }

    #[test]
    fn verify_rejects_wrong_sig() {
        let secret = [0x42; 32];
        assert!(!verify(&secret, "/cover/abc", "deadbeef"));
    }

    #[test]
    fn verify_rejects_wrong_path() {
        let secret = [0x42; 32];
        let sig = sign(&secret, "/cover/abc");
        assert!(!verify(&secret, "/cover/xyz", &sig));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let secret_a = [0x42; 32];
        let secret_b = [0x99; 32];
        let sig = sign(&secret_a, "/cover/abc");
        assert!(!verify(&secret_b, "/cover/abc", &sig));
    }
}
