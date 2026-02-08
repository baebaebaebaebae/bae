pub mod artist_image;
pub mod cover_art;
mod discogs_matcher;
mod discogs_parser;
mod file_validation;
mod folder_metadata_detector;
pub mod folder_scanner;
mod handle;
mod musicbrainz_parser;
mod progress;
mod service;
mod track_to_file_mapper;
mod types;
pub use discogs_matcher::{rank_discogs_matches, rank_mb_matches, MatchCandidate, MatchSource};
pub use folder_metadata_detector::{detect_folder_contents, detect_metadata, FolderMetadata};
pub use folder_scanner::{scan_for_candidates_with_callback, CategorizedFiles, DetectedCandidate};
pub use handle::{ImportServiceHandle, ScanEvent};
#[cfg(feature = "torrent")]
pub use handle::{TorrentFileMetadata, TorrentImportMetadata};
pub use progress::ImportProgressHandle;
pub use service::ImportService;
#[cfg(feature = "torrent")]
pub use types::TorrentSource;
pub use types::{CoverSelection, ImportPhase, ImportProgress, ImportRequest, PrepareStep};

/// Extract a image file extension from a URL using proper URL parsing.
///
/// Parses the URL, extracts the path component (ignoring query params),
/// and returns the extension if it's a known image type. Falls back to "jpg".
fn image_extension_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed.path().rsplit('.').next().and_then(|ext| {
                let lower = ext.to_lowercase();
                if ["jpg", "jpeg", "png", "gif", "webp"].contains(&lower.as_str()) {
                    Some(lower)
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "jpg".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_jpg_url() {
        assert_eq!(
            image_extension_from_url("https://example.com/image.jpg"),
            "jpg"
        );
    }

    #[test]
    fn test_png_with_query_params() {
        assert_eq!(
            image_extension_from_url("https://img.discogs.com/artist/12345.png?token=abc"),
            "png"
        );
    }

    #[test]
    fn test_jpeg_with_path_segments() {
        assert_eq!(
            image_extension_from_url("https://cdn.example.com/images/artists/photo.jpeg"),
            "jpeg"
        );
    }

    #[test]
    fn test_webp_extension() {
        assert_eq!(
            image_extension_from_url("https://example.com/img.webp"),
            "webp"
        );
    }

    #[test]
    fn test_unknown_extension_falls_back_to_jpg() {
        assert_eq!(
            image_extension_from_url("https://example.com/img.bmp"),
            "jpg"
        );
    }

    #[test]
    fn test_no_extension_falls_back_to_jpg() {
        assert_eq!(image_extension_from_url("https://example.com/image"), "jpg");
    }

    #[test]
    fn test_invalid_url_falls_back_to_jpg() {
        assert_eq!(image_extension_from_url("not-a-url"), "jpg");
    }
}
