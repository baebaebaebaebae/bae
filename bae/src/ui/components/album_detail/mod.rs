mod album_art;
mod album_cover_section;
mod album_metadata;
mod back_button;
mod delete_album_dialog;
mod delete_release_dialog;
mod error;
mod export_error_toast;
mod loading;
mod page;
mod play_album_button;
mod release_action_menu;
mod release_info_modal;
mod release_tabs_section;
mod track_row;
pub mod utils;
mod view;

pub use page::AlbumDetail;
pub use release_info_modal::ReleaseInfoModal;

// Re-exports for demo app
#[cfg(feature = "demo")]
pub use back_button::BackButton;
#[cfg(feature = "demo")]
pub use error::AlbumDetailError;
#[cfg(feature = "demo")]
pub use page::PageContainer;
#[cfg(feature = "demo")]
pub use view::AlbumDetailView;
