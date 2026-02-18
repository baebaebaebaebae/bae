//! Shared UI components

pub mod album_card;
pub mod album_detail;
pub mod app_layout;
pub mod artist_detail;
pub mod button;
pub mod dropdown;
pub mod error_banner;
pub mod error_toast;
pub mod helpers;
pub mod icons;
pub mod import;
pub mod imports;
pub mod library;
pub mod menu;
pub mod modal;
pub mod pill;
pub mod playback;
pub mod resizable_panel;
pub mod segmented_control;
pub mod select;
pub mod settings;
pub mod success_toast;
pub mod text_input;
pub mod text_link;
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
pub use artist_detail::ArtistDetailView;
pub use button::{Button, ButtonSize, ButtonVariant, ChromelessButton};
pub use dioxus_virtual_scroll::{
    GridLayout, KeyFn, RenderFn, ScrollTarget, VirtualGrid, VirtualGridConfig,
};
pub use dropdown::{Dropdown, Placement};
pub use error_banner::ErrorBanner;
pub use error_toast::ErrorToast;
pub use helpers::{
    BackButton, ConfirmDialogView, ErrorDisplay, LoadingSpinner, Tooltip, TooltipBubble,
};
pub use icons::{
    AlertTriangleIcon, ArrowDownIcon, ArrowLeftIcon, ArrowRightLeftIcon, ArrowUpIcon, CheckIcon,
    ChevronDownIcon, ChevronLeftIcon, ChevronRightIcon, CloudIcon, CloudOffIcon, DiscIcon,
    DownloadIcon, EllipsisIcon, ExternalLinkIcon, FileIcon, FileTextIcon, FolderIcon,
    HardDriveIcon, ImageIcon, InfoIcon, KeyIcon, LayersIcon, LoaderIcon, LockIcon, MenuIcon,
    MonitorIcon, PauseIcon, PencilIcon, PlayIcon, PlusIcon, RefreshIcon, RowsIcon, SearchIcon,
    SettingsIcon, SkipBackIcon, SkipForwardIcon, StarIcon, TrashIcon, UploadIcon, UserIcon, XIcon,
};
pub use import::{
    CdDriveStatus, CdSelectorView, ConfirmationView, DiscIdLookupErrorView, FileListView,
    FolderImportView, FolderImportViewProps, GalleryItem, GalleryItemContent, GalleryLightbox,
    ImportErrorDisplayView, ImportSource, ImportSourceSelectorView, ImportView,
    ManualSearchPanelView, MatchItemView, MetadataDetectionPromptView, MultipleExactMatchesView,
    ReleaseSelectorView, ReleaseSidebarView, SearchSourceSelectorView, SelectedSourceView,
    TorrentFilesDisplayView, TorrentInfoDisplayView, TorrentInputMode, TorrentInputView,
    TorrentTrackerDisplayView, TrackerConnectionStatus, TrackerStatus,
};
pub use imports::ImportsDropdownView;
pub use library::LibraryView;
pub use menu::{MenuDivider, MenuDropdown, MenuItem};
pub use modal::Modal;
pub use pill::{Pill, PillVariant};
pub use playback::{NowPlayingBarView, QueueSidebarState, QueueSidebarView};
pub use resizable_panel::{GrabBar, PanelPosition, ResizablePanel, ResizeDirection};
pub use segmented_control::{Segment, SegmentedControl};
pub use select::{Select, SelectOption};
pub use settings::{
    AboutSectionView, BaeCloudAuthMode, BitTorrentSectionView, BitTorrentSettings,
    CloudProviderOption, CloudProviderPicker, DiscogsSectionView, FollowLibraryView,
    FollowTestStatus, JoinLibraryView, JoinStatus, LibraryInfo, LibrarySectionView, SettingsCard,
    SettingsSection, SettingsTab, SettingsView, SubsonicSectionView, SyncBucketConfig,
    SyncSectionView,
};
pub use success_toast::SuccessToast;
pub use text_input::{TextInput, TextInputSize, TextInputType};
pub use text_link::TextLink;
pub use title_bar::{
    AlbumResult, ArtistResult, GroupedSearchResults, NavItem, SearchAction, TitleBarView,
    TrackResult, SEARCH_INPUT_ID,
};
pub use utils::{format_duration, format_file_size};
