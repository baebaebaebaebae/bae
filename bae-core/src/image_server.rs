use crate::library::SharedLibraryManager;
use crate::library_dir::LibraryDir;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use std::path::Path as StdPath;
use tracing::{debug, warn};

#[derive(Clone)]
struct ImageServerState {
    library_manager: SharedLibraryManager,
    library_dir: LibraryDir,
}

/// Start the image server on a random port.
/// Returns the port number the server is listening on.
pub async fn start_image_server(
    library_manager: SharedLibraryManager,
    library_dir: LibraryDir,
) -> u16 {
    let state = ImageServerState {
        library_manager,
        library_dir,
    };

    let app = Router::new()
        .route("/cover/:release_id", get(handle_cover))
        .route("/artist-image/:artist_id", get(handle_artist_image))
        .route("/image/:file_id", get(handle_image))
        .route("/local/*path", get(handle_local_file))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind image server");
    let port = listener.local_addr().unwrap().port();

    tracing::info!("Image server listening on http://127.0.0.1:{}", port);

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    port
}

// =============================================================================
// URL helpers
// =============================================================================

pub fn cover_url(port: u16, release_id: &str) -> String {
    format!("http://127.0.0.1:{}/cover/{}", port, release_id)
}

pub fn artist_image_url(port: u16, artist_id: &str) -> String {
    format!("http://127.0.0.1:{}/artist-image/{}", port, artist_id)
}

pub fn image_url(port: u16, file_id: &str) -> String {
    format!("http://127.0.0.1:{}/image/{}", port, file_id)
}

pub fn local_file_url(port: u16, path: &StdPath) -> String {
    let encoded_segments: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .map(|s| urlencoding::encode(s).into_owned())
        .collect();
    format!(
        "http://127.0.0.1:{}/local/{}",
        port,
        encoded_segments.join("/")
    )
}

// =============================================================================
// Handlers
// =============================================================================

async fn handle_cover(
    State(state): State<ImageServerState>,
    Path(release_id): Path<String>,
) -> impl IntoResponse {
    let release_id = release_id.split('?').next().unwrap_or(&release_id);
    let cover_path = state.library_dir.cover_path(release_id);

    match tokio::fs::read(&cover_path).await {
        Ok(data) => {
            let content_type = state
                .library_manager
                .get()
                .get_library_image(release_id, &crate::db::LibraryImageType::Cover)
                .await
                .ok()
                .flatten()
                .map(|img| img.content_type.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());

            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, content_type)],
                data,
            )
                .into_response()
        }
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
    let artist_id = artist_id.split('?').next().unwrap_or(&artist_id);
    let image_path = state.library_dir.artist_image_path(artist_id);

    match tokio::fs::read(&image_path).await {
        Ok(data) => {
            let content_type = state
                .library_manager
                .get()
                .get_library_image(artist_id, &crate::db::LibraryImageType::Artist)
                .await
                .ok()
                .flatten()
                .map(|img| img.content_type.to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());

            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, content_type)],
                data,
            )
                .into_response()
        }
        Err(_) => {
            warn!("Artist image not found for {}", artist_id);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn handle_image(
    State(state): State<ImageServerState>,
    Path(file_id): Path<String>,
) -> impl IntoResponse {
    if file_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing image ID").into_response();
    }

    debug!("Serving image: {}", file_id);

    let file = match state.library_manager.get().get_file_by_id(&file_id).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            warn!("Image file not found: {}", file_id);
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("Database error looking up image {}: {}", file_id, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let source_path = match &file.source_path {
        Some(p) => p.clone(),
        None => {
            warn!("Image {} has no source_path", file_id);
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
            warn!("Failed to read image file {}: {}", source_path, e);
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

    #[test]
    fn test_cover_url() {
        assert_eq!(cover_url(8080, "abc"), "http://127.0.0.1:8080/cover/abc");
    }

    #[test]
    fn test_artist_image_url() {
        assert_eq!(
            artist_image_url(8080, "xyz"),
            "http://127.0.0.1:8080/artist-image/xyz"
        );
    }

    #[test]
    fn test_image_url() {
        assert_eq!(image_url(8080, "f1"), "http://127.0.0.1:8080/image/f1");
    }

    #[test]
    fn test_local_file_url_simple() {
        assert_eq!(
            local_file_url(8080, StdPath::new("/a/b/c.jpg")),
            "http://127.0.0.1:8080/local/a/b/c.jpg"
        );
    }

    #[test]
    fn test_local_file_url_spaces() {
        assert_eq!(
            local_file_url(8080, StdPath::new("/a/b b/c.jpg")),
            "http://127.0.0.1:8080/local/a/b%20b/c.jpg"
        );
    }

    #[test]
    fn test_local_file_url_special_chars() {
        assert_eq!(
            local_file_url(8080, StdPath::new("/a/b's (1,2)/c.jpg")),
            "http://127.0.0.1:8080/local/a/b%27s%20%281%2C2%29/c.jpg"
        );
    }

    #[test]
    fn test_local_file_url_subfolder_preserved() {
        assert_eq!(
            local_file_url(8080, StdPath::new("/a/sub/c.jpg")),
            "http://127.0.0.1:8080/local/a/sub/c.jpg"
        );
    }
}
