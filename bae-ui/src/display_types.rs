//! Display types for UI components
//!
//! These types are lightweight versions of database models, containing
//! only the fields needed for display. They enable props-based components
//! that can work with either real or demo data.

/// Album display info
#[derive(Clone, Debug, PartialEq)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub cover_url: Option<String>,
    pub is_compilation: bool,
}

/// Artist display info
#[derive(Clone, Debug, PartialEq)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

/// Track import state for UI display
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TrackImportState {
    /// Track import not started or not applicable
    #[default]
    None,
    /// Track is being imported with progress percentage
    Importing(u8),
    /// Track import completed
    Complete,
}

/// Track display info
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration_ms: Option<i64>,
    pub is_available: bool,
    /// Import state for reactive UI updates during import
    pub import_state: TrackImportState,
}

/// Playback display state
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PlaybackDisplay {
    #[default]
    Stopped,
    Loading {
        track_id: String,
    },
    Playing {
        track_id: String,
        position_ms: u64,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        position_ms: u64,
        duration_ms: u64,
    },
}

/// Queue item for display
#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub track: Track,
    pub album_title: String,
    pub cover_url: Option<String>,
}

/// Release display info
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Release {
    pub id: String,
    pub album_id: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub discogs_release_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
}

/// File display info
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub id: String,
    pub filename: String,
    pub file_size: i64,
    pub format: String,
}

/// Image display info
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub id: String,
    pub filename: String,
    pub is_cover: bool,
    pub source: String,
}
