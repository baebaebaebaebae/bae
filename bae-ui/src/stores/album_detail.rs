//! Album detail state store

use crate::display_types::{Album, Artist, File, Image, Release, RemoteCoverOption, Track};
use dioxus::prelude::*;

/// Transfer progress state
#[derive(Clone, Debug, PartialEq)]
pub struct TransferProgressState {
    pub file_index: usize,
    pub total_files: usize,
    pub filename: String,
    pub percent: u8,
}

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
    /// Whether the current release's files are managed locally
    pub managed_locally: bool,
    /// Whether the current release's files are managed in the cloud
    pub managed_in_cloud: bool,
    /// Whether the release is unmanaged (files at original location)
    pub is_unmanaged: bool,
    /// Transfer progress (Some when a transfer is active)
    pub transfer_progress: Option<TransferProgressState>,
    /// Transfer error message
    pub transfer_error: Option<String>,
    /// Remote cover options fetched from MusicBrainz/Discogs
    pub remote_covers: Vec<RemoteCoverOption>,
    /// Whether remote covers are currently loading
    pub loading_remote_covers: bool,
    /// Share error message (e.g., share link creation failure)
    pub share_error: Option<String>,
    /// Set to true when a share link has been copied to clipboard
    pub share_link_copied: bool,
}
