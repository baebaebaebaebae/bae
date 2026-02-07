//! Settings view components
//!
//! Pure, props-based components for the settings UI.

mod about;
mod bittorrent;
mod cloud;
mod discogs;
mod storage_profiles;
mod subsonic;
mod view;

pub use about::AboutSectionView;
pub use bittorrent::{BitTorrentSectionView, BitTorrentSettings};
pub use cloud::CloudSectionView;
pub use discogs::DiscogsSectionView;
pub use storage_profiles::{
    StorageLocation, StorageProfile, StorageProfileEditorView, StorageProfilesSectionView,
};
pub use subsonic::SubsonicSectionView;
pub use view::{SettingsTab, SettingsView};
