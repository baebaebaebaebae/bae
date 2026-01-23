//! Album detail state store

use crate::display_types::{Album, Artist, File, Image, Release, Track};
use dioxus::prelude::*;

/// State for the album detail view
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct AlbumDetailState {
    /// The album being viewed
    pub album: Option<Album>,
    /// Artists for this album
    pub artists: Vec<Artist>,
    /// Tracks for this album (with per-track reactive import_state)
    pub tracks: Vec<Track>,
    /// Track count - set when tracks are loaded, avoids subscribing to track changes
    pub track_count: usize,
    /// Track IDs - set when tracks are loaded, avoids subscribing to track changes
    pub track_ids: Vec<String>,
    /// Track disc info (disc_number, track_id) - for disc headers without subscribing to tracks
    pub track_disc_info: Vec<(Option<i32>, String)>,
    /// Releases (editions) for this album
    pub releases: Vec<Release>,
    /// Files for the current release
    pub files: Vec<File>,
    /// Images for this album
    pub images: Vec<Image>,
    /// Currently selected release ID
    pub selected_release_id: Option<String>,
    /// Whether the album data is loading
    pub loading: bool,
    /// Error message if loading failed
    pub error: Option<String>,
    /// Import progress percentage (0-100) for the selected release
    pub import_progress: Option<u8>,
    /// Import error message if import failed
    pub import_error: Option<String>,
}
