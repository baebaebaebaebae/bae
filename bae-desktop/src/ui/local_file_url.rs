//! Helpers for generating bae:// URLs
//!
//! The bae:// custom protocol is registered in app.rs and serves:
//! - Images from storage: bae://image/{image_id}
//! - Local files: bae://local{url_encoded_path}

use bae_ui::display_types::FileInfo;
use std::path::Path;

/// Convert a DbImage ID to a bae:// URL for serving from storage.
///
/// The image will be read and decrypted on demand.
pub fn image_url(image_id: &str) -> String {
    format!("bae://image/{}", image_id)
}

/// Convert a FileInfo to a (name, url) tuple for display.
///
/// Uses the full path from FileInfo (which includes subdirectories like "Scans/")
/// rather than reconstructing from folder_path + name (which loses subdirectories).
///
/// Path components are URL-encoded so they can contain spaces and special characters.
pub fn local_file_url(f: &FileInfo) -> (String, String) {
    let path = Path::new(&f.path);
    let encoded_segments: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .map(|s| urlencoding::encode(s).into_owned())
        .collect();
    let url = format!("bae://local/{}", encoded_segments.join("/"));
    (f.name.clone(), url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, path: &str) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            path: path.to_string(),
            size: 0,
            format: String::new(),
        }
    }

    #[test]
    fn test_image_url() {
        assert_eq!(image_url("abc"), "bae://image/abc");
    }

    #[test]
    fn test_simple() {
        let (name, url) = local_file_url(&file("c.jpg", "/a/b/c.jpg"));
        assert_eq!(name, "c.jpg");
        assert_eq!(url, "bae://local/a/b/c.jpg");
    }

    #[test]
    fn test_spaces() {
        let (_, url) = local_file_url(&file("c.jpg", "/a/b b/c.jpg"));
        assert_eq!(url, "bae://local/a/b%20b/c.jpg");
    }

    #[test]
    fn test_special_chars() {
        // apostrophe, parens, comma
        let (_, url) = local_file_url(&file("c.jpg", "/a/b's (1,2)/c.jpg"));
        assert_eq!(url, "bae://local/a/b%27s%20%281%2C2%29/c.jpg");
    }

    /// Regression: artwork in subfolders was losing the subfolder path.
    #[test]
    fn test_subfolder_preserved() {
        let (name, url) = local_file_url(&file("c.jpg", "/a/sub/c.jpg"));
        assert_eq!(name, "c.jpg");
        assert_eq!(url, "bae://local/a/sub/c.jpg");
    }
}
