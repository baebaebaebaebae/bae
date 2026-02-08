//! Album detail view components

mod album_art;
mod album_cover_section;
mod album_metadata;
mod cover_picker;
mod delete_album_dialog;
mod delete_release_dialog;
mod export_error_toast;
mod play_album_button;
mod release_info_modal;
pub mod release_tabs_section;
mod storage_modal;
mod track_row;
mod view;

pub use album_art::AlbumArt;
pub use album_cover_section::AlbumCoverSection;
pub use album_metadata::AlbumMetadata;
pub use delete_album_dialog::DeleteAlbumDialog;
pub use delete_release_dialog::DeleteReleaseDialog;
pub use export_error_toast::ExportErrorToast;
pub use play_album_button::PlayAlbumButton;
pub use release_info_modal::ReleaseInfoModal;
pub use release_tabs_section::ReleaseTabsSection;
pub use storage_modal::StorageModal;
pub use track_row::TrackRow;
pub use view::AlbumDetailView;
