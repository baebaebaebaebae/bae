use crate::discogs::DiscogsClient;
use crate::library::LibraryManager;
use std::path::Path;
use tracing::{debug, info, warn};

/// Fetch and save an artist image from Discogs.
///
/// Skips if the artist already has an image on disk.
/// Downloads the primary image from Discogs and saves to `{artists_dir}/{artist_id}.{ext}`.
/// Best-effort: logs warnings on failure, never fails the import.
///
/// Returns the relative image path (e.g. "artists/{artist_id}.jpg") if saved successfully.
pub async fn fetch_and_save_artist_image(
    artist_id: &str,
    discogs_artist_id: &str,
    discogs_client: &DiscogsClient,
    artists_dir: &Path,
    library_manager: &LibraryManager,
) -> Option<String> {
    // Check if image already exists on disk
    for ext in &["jpg", "jpeg", "png", "webp"] {
        let path = artists_dir.join(format!("{}.{}", artist_id, ext));
        if path.exists() {
            debug!("Artist image already exists: {}", path.display());
            return None;
        }
    }

    let image_url = match discogs_client.get_artist_image(discogs_artist_id).await {
        Ok(Some(url)) => url,
        Ok(None) => {
            debug!("No image found for Discogs artist {}", discogs_artist_id);
            return None;
        }
        Err(e) => {
            warn!("Failed to fetch artist image URL from Discogs: {}", e);
            return None;
        }
    };

    // Download the image
    let client = match reqwest::Client::builder()
        .user_agent("bae/1.0 +https://github.com/hideselfview/bae")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create HTTP client for artist image: {}", e);
            return None;
        }
    };

    let response = match client.get(&image_url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to download artist image: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        warn!(
            "Artist image download returned status {}",
            response.status()
        );
        return None;
    }

    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read artist image bytes: {}", e);
            return None;
        }
    };

    if bytes.len() < 100 {
        warn!("Downloaded artist image too small ({} bytes)", bytes.len());
        return None;
    }

    let ext = super::image_extension_from_url(&image_url);

    if let Err(e) = std::fs::create_dir_all(artists_dir) {
        warn!("Failed to create artists directory: {}", e);
        return None;
    }

    let filename = format!("{}.{}", artist_id, ext);
    let save_path = artists_dir.join(&filename);

    if let Err(e) = std::fs::write(&save_path, &bytes) {
        warn!("Failed to write artist image: {}", e);
        return None;
    }

    let relative_path = format!("artists/{}", filename);

    info!(
        "Saved artist image ({} bytes) to {}",
        bytes.len(),
        save_path.display()
    );

    // Update DB with image path
    if let Err(e) = library_manager
        .update_artist_image(artist_id, &relative_path)
        .await
    {
        warn!("Failed to update artist image_path in DB: {}", e);
    }

    Some(relative_path)
}
