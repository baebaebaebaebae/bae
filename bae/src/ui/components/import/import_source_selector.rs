pub use bae_ui::ImportSource;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TorrentInputMode {
    File,
    Magnet,
}
