//! Active imports UI state store

use dioxus::prelude::*;

/// Status of an import operation
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ImportOperationStatus {
    #[default]
    Pending,
    Preparing,
    Importing,
    Complete,
    Failed,
}

/// Preparation step during import
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrepareStep {
    ParsingMetadata,
    DownloadingCoverArt,
    DiscoveringFiles,
    ValidatingTracks,
    SavingToDatabase,
    ExtractingDurations,
}

/// Represents a single import operation being tracked in the UI
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveImport {
    pub import_id: String,
    pub album_title: String,
    pub artist_name: String,
    pub status: ImportOperationStatus,
    pub current_step: Option<PrepareStep>,
    pub progress_percent: Option<u8>,
    pub release_id: Option<String>,
    /// External cover art URL (ephemeral, shown during import)
    pub cover_art_url: Option<String>,
    /// Stored cover image ID (shown after import complete)
    pub cover_image_id: Option<String>,
}

/// UI state for active imports (shown in toolbar dropdown)
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ActiveImportsUiState {
    /// List of active import operations
    pub imports: Vec<ActiveImport>,
    /// Whether initial loading is in progress
    pub is_loading: bool,
}
