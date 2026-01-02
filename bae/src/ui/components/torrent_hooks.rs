use crate::torrent::TorrentManagerHandle;
use crate::AppContext;
use dioxus::prelude::*;

/// Hook to access the torrent manager service.
/// This initializes the torrent manager on first access, which triggers
/// the local network permission prompt.
pub fn use_torrent_manager() -> TorrentManagerHandle {
    let context = use_context::<AppContext>();
    context.torrent_manager.get().clone()
}
