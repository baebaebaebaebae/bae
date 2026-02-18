//! Settings view components
//!
//! Pure, props-based components for the settings UI.

mod about;
mod bittorrent;
mod card;
mod cloud_provider;
mod discogs;
mod follow_library;
mod join_library;
mod library;
mod subsonic;
mod sync;
mod view;

pub use about::AboutSectionView;
pub use bittorrent::{BitTorrentSectionView, BitTorrentSettings};
pub use card::{SettingsCard, SettingsSection};
pub use cloud_provider::{BaeCloudAuthMode, CloudProviderOption, CloudProviderPicker};
pub use discogs::DiscogsSectionView;
pub use follow_library::{FollowLibraryView, FollowSyncStatus};
pub use join_library::{JoinLibraryView, JoinStatus};
pub use library::{LibraryInfo, LibrarySectionView};
pub use subsonic::SubsonicSectionView;
pub use sync::{SyncBucketConfig, SyncSectionView};
pub use view::{SettingsTab, SettingsView};
