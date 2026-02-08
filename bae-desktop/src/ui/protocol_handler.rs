use std::borrow::Cow;

use bae_core::library::SharedLibraryManager;
use bae_core::library_dir::LibraryDir;
use dioxus::desktop::wry::http::Response as HttpResponse;
use tracing::{debug, warn};

type ProtocolResponse = HttpResponse<Cow<'static, [u8]>>;

#[derive(Clone)]
pub struct ImageServices {
    pub library_manager: SharedLibraryManager,
    pub library_dir: LibraryDir,
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
    let release_id = release_id.split('?').next().unwrap_or(release_id);
    let cover_path = services.library_dir.cover_path(release_id);

    match std::fs::read(&cover_path) {
        Ok(data) => {
            // Get content type from DB, fall back to jpeg
            let content_type = std::thread::spawn({
                let lm = services.library_manager.clone();
                let rid = release_id.to_string();
                move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        lm.get()
                            .get_library_image(&rid, &bae_core::db::LibraryImageType::Cover)
                            .await
                            .ok()
                            .flatten()
                            .map(|img| img.content_type)
                            .unwrap_or_else(|| "image/jpeg".to_string())
                    })
                }
            })
            .join()
            .unwrap_or_else(|_| "image/jpeg".to_string());

            HttpResponse::builder()
                .status(200)
                .header("Content-Type", content_type)
                .body(Cow::Owned(data))
                .unwrap()
        }
        Err(_) => {
            warn!("Cover not found for release {}", release_id);

            HttpResponse::builder()
                .status(404)
                .body(Cow::Borrowed(b"Cover not found" as &[u8]))
                .unwrap()
        }
    }
}

fn handle_artist_image(artist_id: &str, services: &ImageServices) -> ProtocolResponse {
    let artist_id = artist_id.split('?').next().unwrap_or(artist_id);
    let image_path = services.library_dir.artist_image_path(artist_id);

    match std::fs::read(&image_path) {
        Ok(data) => {
            let content_type = std::thread::spawn({
                let lm = services.library_manager.clone();
                let aid = artist_id.to_string();
                move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        lm.get()
                            .get_library_image(&aid, &bae_core::db::LibraryImageType::Artist)
                            .await
                            .ok()
                            .flatten()
                            .map(|img| img.content_type)
                            .unwrap_or_else(|| "image/jpeg".to_string())
                    })
                }
            })
            .join()
            .unwrap_or_else(|_| "image/jpeg".to_string());

            HttpResponse::builder()
                .status(200)
                .header("Content-Type", content_type)
                .body(Cow::Owned(data))
                .unwrap()
        }
        Err(_) => {
            warn!("Artist image not found for {}", artist_id);

            HttpResponse::builder()
                .status(404)
                .body(Cow::Borrowed(b"Artist image not found" as &[u8]))
                .unwrap()
        }
    }
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
                .map(bae_core::util::content_type_for_extension)
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

    let file = services
        .library_manager
        .get()
        .get_file_by_id(image_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("File not found: {}", image_id))?;

    let source_path = file
        .source_path
        .as_ref()
        .ok_or_else(|| "File has no source_path".to_string())?;

    let data = std::fs::read(source_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let mime_type = bae_core::util::content_type_for_extension(&file.format);

    Ok((data, mime_type))
}
