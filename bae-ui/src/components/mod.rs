//! Shared UI components

pub mod album_card;
pub mod album_detail;
pub mod app_layout;
pub mod helpers;
pub mod import;
pub mod library;
pub mod playback;
pub mod settings;
pub mod title_bar;
pub mod utils;

pub use album_card::AlbumCard;
pub use album_detail::release_tabs_section::ReleaseTorrentInfo;
pub use album_detail::{
    AlbumArt, AlbumCoverSection, AlbumDetailView, AlbumMetadata, DeleteAlbumDialog,
    DeleteReleaseDialog, ExportErrorToast, PlayAlbumButton, ReleaseInfoModal, ReleaseTabsSection,
    TrackRow,
};
pub use app_layout::AppLayoutView;
pub use helpers::{BackButton, ErrorDisplay, LoadingSpinner, PageContainer};
pub use import::{
    CdDriveStatus, CdSelectorView, FolderSelectorView, ImportSource, ImportSourceSelectorView,
    ImportView, TorrentInputMode, TorrentInputView,
};
pub use library::LibraryView;
pub use playback::{NowPlayingBarView, QueueSidebarState, QueueSidebarView};
pub use settings::{
    AboutSectionView, ApiKeysSectionView, BitTorrentSectionView, BitTorrentSettings,
    EncryptionSectionView, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfileEditorView, StorageProfilesSectionView, SubsonicSectionView,
};
pub use title_bar::{NavItem, SearchResult, TitleBarView};
pub use utils::{format_duration, format_file_size};
