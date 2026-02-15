//! Top-level application state store
//!
//! This combines all sub-states into a single Store for the entire app.
//! Components access state via lensing: `app.state.import().candidates()`

use super::active_imports::ActiveImportsUiState;
use super::album_detail::AlbumDetailState;
use super::artist_detail::ArtistDetailState;
use super::config::ConfigState;
use super::import::ImportState;
use super::library::LibraryState;
use super::playback::PlaybackUiState;
use super::sync::SyncState;
use super::ui::UiState;
use dioxus::prelude::*;

/// Top-level application state combining all sub-states
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct AppState {
    /// Import workflow state (per-candidate state machines)
    pub import: ImportState,
    /// Library view state (albums, artists)
    pub library: LibraryState,
    /// Album detail view state
    pub album_detail: AlbumDetailState,
    /// Artist detail view state
    pub artist_detail: ArtistDetailState,
    /// Active imports shown in toolbar dropdown
    pub active_imports: ActiveImportsUiState,
    /// Playback state (playing/paused, queue)
    pub playback: PlaybackUiState,
    /// General UI state (overlays, sidebar, search)
    pub ui: UiState,
    /// Application configuration
    pub config: ConfigState,
    /// Sync status
    pub sync: SyncState,
}
