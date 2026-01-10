//! Shared UI components

pub mod album_card;
pub mod album_detail;
pub mod helpers;
pub mod library;
pub mod playback;
pub mod utils;

pub use album_card::AlbumCard;
pub use album_detail::release_tabs_section::ReleaseTorrentInfo;
pub use album_detail::{
    AlbumArt, AlbumCoverSection, AlbumDetailView, AlbumMetadata, DeleteAlbumDialog,
    DeleteReleaseDialog, ExportErrorToast, PlayAlbumButton, ReleaseInfoModal, ReleaseTabsSection,
    TrackRow,
};
pub use helpers::{BackButton, ErrorDisplay, LoadingSpinner, PageContainer};
pub use library::LibraryView;
pub use playback::{NowPlayingBarView, QueueSidebarState, QueueSidebarView};
pub use utils::{format_duration, format_file_size};
