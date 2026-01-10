//! Settings view components
//!
//! Pure, props-based components for the settings UI.

mod about;
mod api_keys;
mod bittorrent;
mod encryption;
mod storage_profiles;
mod subsonic;
mod view;

pub use about::AboutSectionView;
pub use api_keys::ApiKeysSectionView;
pub use bittorrent::{BitTorrentSectionView, BitTorrentSettings};
pub use encryption::EncryptionSectionView;
pub use storage_profiles::{
    StorageLocation, StorageProfile, StorageProfileEditorView, StorageProfilesSectionView,
};
pub use subsonic::SubsonicSectionView;
pub use view::{SettingsTab, SettingsView};
