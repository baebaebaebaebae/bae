use std::borrow::Cow;
use std::path::PathBuf;

use bae_core::library::SharedLibraryManager;
use dioxus::desktop::wry::http::Response as HttpResponse;
use tracing::{debug, warn};

type ProtocolResponse = HttpResponse<Cow<'static, [u8]>>;

#[derive(Clone)]
pub struct ImageServices {
    pub library_manager: SharedLibraryManager,
    pub library_path: PathBuf,
}

pub fn handle_protocol_request(uri: &str, services: &ImageServices) -> ProtocolResponse {
    tracing::trace!("bae:// protocol request: {:?}", uri);

    if let Some(encoded_path) = uri.strip_prefix("bae://local") {
        handle_local_file(encoded_path)
    } else if let Some(release_id) = uri.strip_prefix("bae://cover/") {
        handle_cover(release_id, services)
    } else if let Some(artist_id) = uri.strip_prefix("bae://artist-image/") {
        handle_artist_image(artist_id, services)
    } else if let Some(image_id) = uri.strip_prefix("bae://image/") {
        handle_image(image_id, services)
    } else {
        warn!("Invalid bae:// URL: {}", uri);
        HttpResponse::builder()
            .status(400)
            .body(Cow::Borrowed(b"Invalid URL" as &[u8]))
            .unwrap()
    }
}

fn handle_cover(release_id: &str, services: &ImageServices) -> ProtocolResponse {
    // Strip query params (e.g. ?t=123 for cache busting)
    let release_id = release_id.split('?').next().unwrap_or(release_id);
    let covers_dir = services.library_path.join("covers");

    // Try common image extensions
    for ext in &["jpg", "jpeg", "png", "webp", "gif"] {
        let path = covers_dir.join(format!("{}.{}", release_id, ext));
        if let Ok(data) = std::fs::read(&path) {
            let mime = mime_type_for_extension(ext);
            return HttpResponse::builder()
                .status(200)
                .header("Content-Type", mime)
                .body(Cow::Owned(data))
                .unwrap();
        }
    }

    warn!("Cover not found for album {}", release_id);
    HttpResponse::builder()
        .status(404)
        .body(Cow::Borrowed(b"Cover not found" as &[u8]))
        .unwrap()
}

fn handle_artist_image(artist_id: &str, services: &ImageServices) -> ProtocolResponse {
    let artists_dir = services.library_path.join("artists");

    for ext in &["jpg", "jpeg", "png", "webp", "gif"] {
        let path = artists_dir.join(format!("{}.{}", artist_id, ext));
        if let Ok(data) = std::fs::read(&path) {
            let mime = mime_type_for_extension(ext);
            return HttpResponse::builder()
                .status(200)
                .header("Content-Type", mime)
                .body(Cow::Owned(data))
                .unwrap();
        }
    }

    warn!("Artist image not found for {}", artist_id);
    HttpResponse::builder()
        .status(404)
        .body(Cow::Borrowed(b"Artist image not found" as &[u8]))
        .unwrap()
}

fn handle_local_file(encoded_path: &str) -> ProtocolResponse {
    let path: String = encoded_path
        .split('/')
        .map(|segment| {
            urlencoding::decode(segment)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| segment.to_string())
        })
        .collect::<Vec<_>>()
        .join("/");

    match std::fs::read(&path) {
        Ok(data) => {
            let mime_type = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map(mime_type_for_extension)
                .unwrap_or("application/octet-stream");

            HttpResponse::builder()
                .status(200)
                .header("Content-Type", mime_type)
                .body(Cow::Owned(data))
                .unwrap()
        }
        Err(e) => {
            warn!("Failed to read file {}: {}", path, e);
            HttpResponse::builder()
                .status(404)
                .body(Cow::Borrowed(b"File not found" as &[u8]))
                .unwrap()
        }
    }
}

fn handle_image(image_id: &str, services: &ImageServices) -> ProtocolResponse {
    if image_id.is_empty() {
        return HttpResponse::builder()
            .status(400)
            .body(Cow::Borrowed(b"Missing image ID" as &[u8]))
            .unwrap();
    }

    let services_clone = services.clone();
    let image_id_owned = image_id.to_string();

    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(serve_image(&image_id_owned, &services_clone))
    })
    .join()
    .unwrap_or_else(|_| Err("Thread panicked".to_string()));

    match result {
        Ok((data, mime_type)) => HttpResponse::builder()
            .status(200)
            .header("Content-Type", mime_type)
            .body(Cow::Owned(data))
            .unwrap(),
        Err(e) => {
            warn!("Failed to serve image {}: {}", image_id, e);
            HttpResponse::builder()
                .status(404)
                .body(Cow::Owned(format!("Image not found: {}", e).into_bytes()))
                .unwrap()
        }
    }
}

async fn serve_image(
    image_id: &str,
    services: &ImageServices,
) -> Result<(Vec<u8>, &'static str), String> {
    debug!("Serving image: {}", image_id);

    let image = services
        .library_manager
        .get()
        .get_image_by_id(image_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Image not found: {}", image_id))?;

    let data = services
        .library_manager
        .get()
        .fetch_image_bytes(image_id)
        .await
        .map_err(|e| format!("Failed to fetch image: {}", e))?;

    let mime_type = match image.filename.rsplit('.').next() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };

    Ok((data, mime_type))
}

fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "tiff" | "tif" => "image/tiff",
        _ => "application/octet-stream",
    }
}
