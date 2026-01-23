//! Helpers for generating bae:// URLs
//!
//! The bae:// custom protocol is registered in app.rs and serves:
//! - Images from storage: bae://image/{image_id}
//! - Local files: bae://local{url_encoded_path}

use std::path::Path;

/// Convert a DbImage ID to a bae:// URL for serving from storage.
///
/// The image will be read and decrypted on demand.
pub fn image_url(image_id: &str) -> String {
    format!("bae://image/{}", image_id)
}

/// Convert a local file path to a bae://local/... URL.
///
/// Path components are URL-encoded so they can contain spaces and special characters.
pub fn local_file_url(path: &Path) -> String {
    let encoded_segments: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .map(|s| urlencoding::encode(s).into_owned())
        .collect();
    format!("bae://local/{}", encoded_segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_url() {
        assert_eq!(image_url("abc"), "bae://image/abc");
    }

    #[test]
    fn test_simple() {
        assert_eq!(
            local_file_url(Path::new("/a/b/c.jpg")),
            "bae://local/a/b/c.jpg"
        );
    }

    #[test]
    fn test_spaces() {
        assert_eq!(
            local_file_url(Path::new("/a/b b/c.jpg")),
            "bae://local/a/b%20b/c.jpg"
        );
    }

    #[test]
    fn test_special_chars() {
        assert_eq!(
            local_file_url(Path::new("/a/b's (1,2)/c.jpg")),
            "bae://local/a/b%27s%20%281%2C2%29/c.jpg"
        );
    }

    /// Regression: artwork in subfolders was losing the subfolder path.
    #[test]
    fn test_subfolder_preserved() {
        assert_eq!(
            local_file_url(Path::new("/a/sub/c.jpg")),
            "bae://local/a/sub/c.jpg"
        );
    }
}
