//! Storage abstraction layer
//!
//! Provides flexible storage options for releases. Storage is configured via
//! StorageProfile (location + encrypted) and implemented by a single
//! ReleaseStorageImpl that applies transforms based on the profile.
pub mod cleanup;
mod reader;
mod traits;
pub mod transfer;

pub use reader::create_storage_reader;
pub use traits::{ReleaseStorage, ReleaseStorageImpl};

/// Hash-based storage path for a file: `storage/{ab}/{cd}/{file_id}`
///
/// Deterministic from the file_id alone. Used for both local profiles
/// (relative to `location_path`) and cloud profiles (S3 key).
pub fn storage_path(file_id: &str) -> String {
    let hex = file_id.replace('-', "");
    format!("storage/{}/{}/{}", &hex[..2], &hex[2..4], file_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_path_uses_first_four_hex_chars() {
        let id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let path = storage_path(id);
        assert_eq!(path, format!("storage/a1/b2/{}", id));
    }

    #[test]
    fn storage_path_preserves_original_id_with_dashes() {
        let id = "12345678-aaaa-bbbb-cccc-ddddeeeeaaaa";
        let path = storage_path(id);
        // prefix from dashless hex: "12" and "34"
        assert_eq!(path, format!("storage/12/34/{}", id));
        // The full id with dashes is the filename
        assert!(path.ends_with(id));
    }
}
