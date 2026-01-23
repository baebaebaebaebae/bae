//! Demo app pages

mod album_detail;
mod import;
mod layout;
mod library;
mod mock_dropdown;
mod mock_index;
mod settings;

pub use album_detail::AlbumDetail;
pub use import::Import;
pub use layout::DemoLayout;
pub use library::Library;
pub use mock_dropdown::MockDropdownTest;
pub use mock_index::{MockAlbumDetail, MockFolderImport, MockIndex, MockLibrary, MockTitleBar};
pub use settings::Settings;
