//! Import workflow state store
//!
//! State machine types for the import workflow. These are used by both
//! bae-desktop (real import) and bae-mocks (design tool).

use crate::display_types::{
    CandidateTrack, CategorizedFileInfo, DetectedCandidate, FolderMetadata, IdentifyMode,
    MatchCandidate, SearchSource, SearchTab, SelectedCover,
};
use dioxus::prelude::*;

// ============================================================================
// State Machine Types
// ============================================================================

/// Per-candidate state. Only constructed after file scan + metadata detection complete.
// We keep this unboxed for store lensing and accept size overhead.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Store)]
pub enum CandidateState {
    /// User picking from auto matches or searching manually (ImportStep::Identify)
    Identifying(IdentifyingState),
    /// User confirming selection before import (ImportStep::Confirm)
    Confirming(Box<ConfirmingState>),
}

/// State for the Identify step
#[derive(Clone, Debug, PartialEq, Store)]
pub struct IdentifyingState {
    /// Files from the candidate folder (required)
    pub files: CategorizedFileInfo,
    /// Detected metadata from tags/filenames (required)
    pub metadata: FolderMetadata,
    /// Current mode within Identify step
    pub mode: IdentifyMode,
    /// Cached auto-match results from DiscID lookup
    pub auto_matches: Vec<MatchCandidate>,
    /// Index of selected match in auto_matches (for MultipleExactMatches mode)
    pub selected_match_index: Option<usize>,
    /// Prefetch validation state for the selected exact match
    pub exact_match_prefetch: Option<PrefetchState>,
    /// True when user clicked "Select" on exact match while prefetch was in-flight
    pub exact_match_confirm_pending: bool,
    /// Manual search state (persisted even when in MultipleExactMatches)
    pub search_state: ManualSearchState,
    /// Error from DiscID lookup (network/server error - retryable)
    pub discid_lookup_error: Option<String>,
    /// Disc ID that was searched but found no results (informational, not retryable)
    pub disc_id_not_found: Option<String>,
    /// Source disc ID for auto_matches (preserved when switching to ManualSearch)
    pub source_disc_id: Option<String>,
}

/// Prefetch validation state for a selected search result
#[derive(Clone, Debug, PartialEq)]
pub enum PrefetchState {
    /// Full release fetch in progress
    Fetching,
    /// Track count matches local files
    Valid { tracks: Vec<CandidateTrack> },
    /// Track count mismatch
    TrackCountMismatch {
        release_tracks: usize,
        local_files: usize,
    },
    /// Fetch failed with error message
    FetchFailed(String),
}

/// Per-tab search results and status
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct TabSearchState {
    pub has_searched: bool,
    pub search_results: Vec<MatchCandidate>,
    pub selected_result_index: Option<usize>,
    pub error_message: Option<String>,
    /// Prefetch validation state for the selected result
    pub prefetch_state: Option<PrefetchState>,
    /// True when user clicked "Select" while prefetch was still in-flight
    pub confirm_pending: bool,
}

/// State for manual search within Identify step
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ManualSearchState {
    pub search_source: SearchSource,
    pub search_artist: String,
    pub search_album: String,
    pub search_year: String,
    pub search_label: String,
    pub search_catalog_number: String,
    pub search_barcode: String,
    pub search_tab: SearchTab,
    pub is_searching: bool,
    pub general_mb: TabSearchState,
    pub general_discogs: TabSearchState,
    pub catalog_number_mb: TabSearchState,
    pub catalog_number_discogs: TabSearchState,
    pub barcode_mb: TabSearchState,
    pub barcode_discogs: TabSearchState,
}

impl ManualSearchState {
    pub fn any_tab_searched(&self) -> bool {
        self.general_mb.has_searched
            || self.general_discogs.has_searched
            || self.catalog_number_mb.has_searched
            || self.catalog_number_discogs.has_searched
            || self.barcode_mb.has_searched
            || self.barcode_discogs.has_searched
    }

    pub fn current_tab_state(&self) -> &TabSearchState {
        match (self.search_tab, self.search_source) {
            (SearchTab::General, SearchSource::MusicBrainz) => &self.general_mb,
            (SearchTab::General, SearchSource::Discogs) => &self.general_discogs,
            (SearchTab::CatalogNumber, SearchSource::MusicBrainz) => &self.catalog_number_mb,
            (SearchTab::CatalogNumber, SearchSource::Discogs) => &self.catalog_number_discogs,
            (SearchTab::Barcode, SearchSource::MusicBrainz) => &self.barcode_mb,
            (SearchTab::Barcode, SearchSource::Discogs) => &self.barcode_discogs,
        }
    }

    pub fn current_tab_state_mut(&mut self) -> &mut TabSearchState {
        match (self.search_tab, self.search_source) {
            (SearchTab::General, SearchSource::MusicBrainz) => &mut self.general_mb,
            (SearchTab::General, SearchSource::Discogs) => &mut self.general_discogs,
            (SearchTab::CatalogNumber, SearchSource::MusicBrainz) => &mut self.catalog_number_mb,
            (SearchTab::CatalogNumber, SearchSource::Discogs) => &mut self.catalog_number_discogs,
            (SearchTab::Barcode, SearchSource::MusicBrainz) => &mut self.barcode_mb,
            (SearchTab::Barcode, SearchSource::Discogs) => &mut self.barcode_discogs,
        }
    }
}

/// State for the Confirm step
#[derive(Clone, Debug, PartialEq, Store)]
pub struct ConfirmingState {
    /// Files from the candidate folder (required)
    pub files: CategorizedFileInfo,
    /// Detected metadata (required)
    pub metadata: FolderMetadata,
    /// The confirmed release candidate (required)
    pub confirmed_candidate: MatchCandidate,
    /// Selected cover art
    pub selected_cover: Option<SelectedCover>,
    /// Whether to copy files into managed local storage
    pub managed: bool,
    /// Current phase within Confirm step
    pub phase: ConfirmPhase,
    /// Cached auto-match results (for returning to Identify)
    pub auto_matches: Vec<MatchCandidate>,
    /// Manual search state (for returning to Identify)
    pub search_state: ManualSearchState,
    /// Disc ID that led to this confirmation (for returning to MultipleExactMatches)
    pub source_disc_id: Option<String>,
}

/// Pick a default cover: remote (MB/Discogs) > release artwork > none
fn default_cover(candidate: &MatchCandidate, files: &CategorizedFileInfo) -> Option<SelectedCover> {
    if let Some(url) = &candidate.cover_url {
        return Some(SelectedCover::Remote {
            url: url.clone(),
            source: String::new(),
        });
    }
    if let Some(img) = files.artwork.first() {
        return Some(SelectedCover::Local {
            filename: img.name.clone(),
        });
    }
    None
}

/// Phase within the Confirm step
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub enum ConfirmPhase {
    /// User can edit cover/profile and click Confirm
    #[default]
    Ready,
    /// Fetching/preparing after clicking Confirm
    Preparing(String),
    /// Import command sent, controls disabled
    Importing,
    /// Error occurred
    Failed(String),
    /// Import finished successfully (carries album_id for navigation)
    Completed(String),
}

// ============================================================================
// Event Types
// ============================================================================

/// Events that can be dispatched to the state machine
#[derive(Clone, Debug)]
pub enum CandidateEvent {
    // --- Identify step events ---
    /// User selects a match from the auto/exact match list (just updates selection, doesn't confirm)
    SelectExactMatch(usize),
    /// User confirms the selected exact match and transitions to Confirming
    ConfirmExactMatch,
    /// Prefetch started for an exact match result (sets Fetching state)
    ExactMatchPrefetchStarted(usize),
    /// Prefetch completed for an exact match result
    ExactMatchPrefetchComplete { index: usize, result: PrefetchState },
    /// Set confirm_pending for exact match (user clicked Select while prefetch in-flight)
    SetExactMatchConfirmPending,
    /// User switches from MultipleExactMatches to ManualSearch
    SwitchToManualSearch,
    /// User switches from ManualSearch back to MultipleExactMatches (carries the disc_id)
    SwitchToMultipleExactMatches(String),
    /// Start or retry DiscID lookup with the given disc ID
    StartDiscIdLookup(String),
    /// DiscID lookup completed (from async operation)
    DiscIdLookupComplete {
        matches: Vec<MatchCandidate>,
        error: Option<String>,
    },

    // --- Manual search events ---
    /// User updates a search field
    UpdateSearchField { field: SearchField, value: String },
    /// User changes the active search tab
    SetSearchTab(SearchTab),
    /// User changes the search source (MusicBrainz/Discogs)
    SetSearchSource(SearchSource),
    /// User initiates a search
    StartSearch,
    /// User cancels an in-progress search
    CancelSearch,
    /// Search completed (from async operation)
    SearchComplete {
        results: Vec<MatchCandidate>,
        error: Option<String>,
    },
    /// User selects a result from manual search
    SelectSearchResult(usize),
    /// User confirms the selected search result
    ConfirmSearchResult,
    /// Prefetch started for a search result (sets Fetching state)
    PrefetchStarted(usize),
    /// Prefetch completed for a search result
    PrefetchComplete { index: usize, result: PrefetchState },
    /// Set confirm_pending flag (user clicked Select while prefetch in-flight)
    SetConfirmPending,
    /// Update cover art for a search result (after retry or initial check)
    UpdateSearchResultCover {
        index: usize,
        cover_url: Option<String>,
        cover_fetch_failed: bool,
    },

    // --- Confirm step events ---
    /// User clicks "Edit" to go back to Identify
    GoBackToIdentify,
    /// User selects cover art
    SelectCover(Option<SelectedCover>),
    /// User selects storage profile
    SetManaged(bool),
    /// User clicks "Import" button
    StartImport,
    /// Import is preparing (from async operation)
    ImportPreparing(String),
    /// Import command sent successfully
    ImportStarted,
    /// Import failed (from async operation)
    ImportFailed(String),
    /// Import completed successfully (carries album_id)
    ImportCompleted(String),
}

/// Which search field is being updated
#[derive(Clone, Debug, PartialEq)]
pub enum SearchField {
    Artist,
    Album,
    Year,
    Label,
    CatalogNumber,
    Barcode,
}

// ============================================================================
// State Machine Implementation
// ============================================================================

impl CandidateState {
    /// Get the files from any state
    pub fn files(&self) -> &CategorizedFileInfo {
        match self {
            CandidateState::Identifying(s) => &s.files,
            CandidateState::Confirming(s) => &s.files,
        }
    }

    /// Get the metadata from any state
    pub fn metadata(&self) -> &FolderMetadata {
        match self {
            CandidateState::Identifying(s) => &s.metadata,
            CandidateState::Confirming(s) => &s.metadata,
        }
    }

    /// Check if this candidate is currently importing
    pub fn is_importing(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Importing)
        )
    }

    /// Check if import is in progress (preparing or importing)
    pub fn is_import_in_progress(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Importing | ConfirmPhase::Preparing(_))
        )
    }

    /// Check if this candidate has been imported (completed)
    pub fn is_imported(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Completed(_))
        )
    }

    /// Apply an event and return the new state.
    /// This is the core state machine transition function.
    pub fn transition(self, event: CandidateEvent) -> CandidateState {
        match self {
            CandidateState::Identifying(s) => s.on_event(event),
            CandidateState::Confirming(s) => s.on_event(event),
        }
    }
}

impl IdentifyingState {
    fn on_event(self, event: CandidateEvent) -> CandidateState {
        match event {
            CandidateEvent::SelectExactMatch(idx) => {
                // Just update selection, don't transition
                let mut state = self;
                if idx < state.auto_matches.len() {
                    state.selected_match_index = Some(idx);
                    state.exact_match_prefetch = None;
                    state.exact_match_confirm_pending = false;
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::ConfirmExactMatch => {
                // Only transition if prefetch validated successfully
                let tracks = match &self.exact_match_prefetch {
                    Some(PrefetchState::Valid { tracks }) => tracks.clone(),
                    _ => return CandidateState::Identifying(self),
                };
                if let Some(idx) = self.selected_match_index {
                    if let Some(mut candidate) = self.auto_matches.get(idx).cloned() {
                        candidate.tracks = tracks;
                        let source_disc_id = match &self.mode {
                            IdentifyMode::MultipleExactMatches(id) => Some(id.clone()),
                            _ => None,
                        };
                        let selected_cover = default_cover(&candidate, &self.files);
                        return CandidateState::Confirming(Box::new(ConfirmingState {
                            files: self.files,
                            metadata: self.metadata,
                            confirmed_candidate: candidate,
                            selected_cover,
                            managed: true,
                            phase: ConfirmPhase::Ready,
                            auto_matches: self.auto_matches,
                            search_state: self.search_state,
                            source_disc_id,
                        }));
                    }
                }
                CandidateState::Identifying(self)
            }
            CandidateEvent::ExactMatchPrefetchStarted(index) => {
                let mut state = self;
                if state.selected_match_index == Some(index) {
                    state.exact_match_prefetch = Some(PrefetchState::Fetching);
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::ExactMatchPrefetchComplete { index, result } => {
                let mut state = self;
                if state.selected_match_index == Some(index) {
                    let should_auto_confirm = state.exact_match_confirm_pending
                        && matches!(result, PrefetchState::Valid { .. });
                    state.exact_match_prefetch = Some(result);
                    state.exact_match_confirm_pending = false;
                    if should_auto_confirm {
                        if let Some(mut candidate) = state.auto_matches.get(index).cloned() {
                            if let Some(PrefetchState::Valid { tracks }) =
                                &state.exact_match_prefetch
                            {
                                candidate.tracks = tracks.clone();
                            }
                            let source_disc_id = match &state.mode {
                                IdentifyMode::MultipleExactMatches(id) => Some(id.clone()),
                                _ => None,
                            };
                            let selected_cover = default_cover(&candidate, &state.files);
                            return CandidateState::Confirming(Box::new(ConfirmingState {
                                files: state.files,
                                metadata: state.metadata,
                                confirmed_candidate: candidate,
                                selected_cover,
                                managed: true,
                                phase: ConfirmPhase::Ready,
                                auto_matches: state.auto_matches,
                                search_state: state.search_state,
                                source_disc_id,
                            }));
                        }
                    }
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetExactMatchConfirmPending => {
                let mut state = self;
                state.exact_match_confirm_pending = true;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SwitchToManualSearch => {
                let mut state = self;
                state.mode = IdentifyMode::ManualSearch;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SwitchToMultipleExactMatches(disc_id) => {
                let mut state = self;
                if !state.auto_matches.is_empty() {
                    state.mode = IdentifyMode::MultipleExactMatches(disc_id);
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::StartDiscIdLookup(disc_id) => {
                let mut state = self;
                state.mode = IdentifyMode::DiscIdLookup(disc_id);
                state.discid_lookup_error = None;
                CandidateState::Identifying(state)
            }
            CandidateEvent::DiscIdLookupComplete { matches, error } => {
                let mut state = self;

                // Extract disc ID from current mode (before we potentially change it)
                let disc_id = match &state.mode {
                    IdentifyMode::DiscIdLookup(id) => Some(id.clone()),
                    _ => None,
                };

                state.auto_matches = matches.clone();

                // Handle error vs no results vs found matches
                if let Some(err) = error {
                    // Network/server error - stay in DiscIdLookup mode so user can retry
                    state.discid_lookup_error = Some(err);
                    state.disc_id_not_found = None;
                } else if matches.is_empty() {
                    // Lookup succeeded but no releases found - go to manual search
                    state.discid_lookup_error = None;
                    state.disc_id_not_found = disc_id;
                    state.mode = IdentifyMode::ManualSearch;
                } else if matches.len() == 1 {
                    // Single match — auto-confirm, but keep in auto_matches
                    // so "view exact matches" works if user goes back.
                    // Tracks are pre-populated by the desktop layer before dispatch.
                    let candidate = matches[0].clone();
                    let selected_cover = default_cover(&candidate, &state.files);
                    return CandidateState::Confirming(Box::new(ConfirmingState {
                        files: state.files,
                        metadata: state.metadata,
                        confirmed_candidate: candidate,
                        selected_cover,
                        managed: true,
                        phase: ConfirmPhase::Ready,
                        auto_matches: matches,
                        search_state: state.search_state,
                        source_disc_id: disc_id,
                    }));
                } else {
                    // Multiple matches - let user choose
                    state.discid_lookup_error = None;
                    state.disc_id_not_found = None;
                    if let Some(id) = disc_id.clone() {
                        state.mode = IdentifyMode::MultipleExactMatches(id);
                    }
                    state.source_disc_id = disc_id;
                };

                CandidateState::Identifying(state)
            }
            CandidateEvent::UpdateSearchField { field, value } => {
                let mut state = self;
                state.search_state.current_tab_state_mut().error_message = None;
                match field {
                    SearchField::Artist => state.search_state.search_artist = value,
                    SearchField::Album => state.search_state.search_album = value,
                    SearchField::Year => state.search_state.search_year = value,
                    SearchField::Label => state.search_state.search_label = value,
                    SearchField::CatalogNumber => state.search_state.search_catalog_number = value,
                    SearchField::Barcode => state.search_state.search_barcode = value,
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetSearchTab(tab) => {
                let mut state = self;
                state.search_state.search_tab = tab;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetSearchSource(source) => {
                let mut state = self;
                state.search_state.search_source = source;
                CandidateState::Identifying(state)
            }
            CandidateEvent::StartSearch => {
                let mut state = self;
                state.search_state.is_searching = true;
                state.search_state.current_tab_state_mut().error_message = None;
                CandidateState::Identifying(state)
            }
            CandidateEvent::CancelSearch => {
                let mut state = self;
                state.search_state.is_searching = false;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SearchComplete { results, error } => {
                let mut state = self;
                state.search_state.is_searching = false;
                let tab = state.search_state.current_tab_state_mut();
                tab.has_searched = true;
                tab.search_results = results;
                tab.error_message = error;
                tab.selected_result_index = None;
                tab.prefetch_state = None;
                tab.confirm_pending = false;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SelectSearchResult(idx) => {
                let mut state = self;
                let tab = state.search_state.current_tab_state_mut();
                if idx < tab.search_results.len() {
                    tab.selected_result_index = Some(idx);
                    // Clear previous prefetch when selecting a different result
                    tab.prefetch_state = None;
                    tab.confirm_pending = false;
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::UpdateSearchResultCover {
                index,
                cover_url,
                cover_fetch_failed,
            } => {
                let mut state = self;
                let tab = state.search_state.current_tab_state_mut();
                if let Some(result) = tab.search_results.get_mut(index) {
                    result.cover_url = cover_url;
                    result.cover_fetch_failed = cover_fetch_failed;
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::PrefetchStarted(index) => {
                let mut state = self;
                let tab = state.search_state.current_tab_state_mut();
                if tab.selected_result_index == Some(index) {
                    tab.prefetch_state = Some(PrefetchState::Fetching);
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::PrefetchComplete { index, result } => {
                let mut state = self;
                let tab = state.search_state.current_tab_state_mut();
                // Only apply if the index still matches the selected result
                if tab.selected_result_index == Some(index) {
                    let should_auto_confirm =
                        tab.confirm_pending && matches!(result, PrefetchState::Valid { .. });
                    tab.prefetch_state = Some(result);
                    tab.confirm_pending = false;
                    if should_auto_confirm {
                        // Auto-transition to Confirming
                        let tab = state.search_state.current_tab_state();
                        if let Some(mut candidate) = tab.search_results.get(index).cloned() {
                            if let Some(PrefetchState::Valid { tracks }) = &tab.prefetch_state {
                                candidate.tracks = tracks.clone();
                            }
                            let selected_cover = default_cover(&candidate, &state.files);
                            return CandidateState::Confirming(Box::new(ConfirmingState {
                                files: state.files,
                                metadata: state.metadata,
                                confirmed_candidate: candidate,
                                selected_cover,
                                managed: true,
                                phase: ConfirmPhase::Ready,
                                auto_matches: state.auto_matches,
                                search_state: state.search_state,
                                source_disc_id: None,
                            }));
                        }
                    }
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetConfirmPending => {
                let mut state = self;
                let tab = state.search_state.current_tab_state_mut();
                tab.confirm_pending = true;
                CandidateState::Identifying(state)
            }
            CandidateEvent::ConfirmSearchResult => {
                let state = self;
                let tab = state.search_state.current_tab_state();
                // Only transition if prefetch validated successfully
                let tracks = match &tab.prefetch_state {
                    Some(PrefetchState::Valid { tracks }) => tracks.clone(),
                    _ => return CandidateState::Identifying(state),
                };
                if let Some(idx) = tab.selected_result_index {
                    if let Some(mut candidate) = tab.search_results.get(idx).cloned() {
                        candidate.tracks = tracks;
                        let selected_cover = default_cover(&candidate, &state.files);
                        return CandidateState::Confirming(Box::new(ConfirmingState {
                            files: state.files,
                            metadata: state.metadata,
                            confirmed_candidate: candidate,
                            selected_cover,
                            managed: true,
                            phase: ConfirmPhase::Ready,
                            auto_matches: state.auto_matches,
                            search_state: state.search_state,
                            source_disc_id: None, // Coming from manual search
                        }));
                    }
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::GoBackToIdentify
            | CandidateEvent::SelectCover(_)
            | CandidateEvent::SetManaged(_)
            | CandidateEvent::StartImport
            | CandidateEvent::ImportPreparing(_)
            | CandidateEvent::ImportStarted
            | CandidateEvent::ImportFailed(_)
            | CandidateEvent::ImportCompleted(_) => CandidateState::Identifying(self),
        }
    }
}

impl ConfirmingState {
    fn on_event(self, event: CandidateEvent) -> CandidateState {
        match event {
            CandidateEvent::GoBackToIdentify => {
                let mode = match (&self.source_disc_id, self.auto_matches.is_empty()) {
                    (Some(disc_id), false) => IdentifyMode::MultipleExactMatches(disc_id.clone()),
                    _ => IdentifyMode::ManualSearch,
                };
                CandidateState::Identifying(IdentifyingState {
                    files: self.files,
                    metadata: self.metadata,
                    mode,
                    auto_matches: self.auto_matches,
                    selected_match_index: None,
                    exact_match_prefetch: None,
                    exact_match_confirm_pending: false,
                    search_state: self.search_state,
                    discid_lookup_error: None,
                    disc_id_not_found: None,
                    source_disc_id: self.source_disc_id,
                })
            }
            CandidateEvent::SelectCover(cover) => {
                let mut state = self;
                state.selected_cover = cover;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::SetManaged(managed) => {
                let mut state = self;
                state.managed = managed;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::StartImport => {
                let mut state = self;
                state.phase = ConfirmPhase::Preparing("Starting...".to_string());
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportPreparing(step) => {
                let mut state = self;
                state.phase = ConfirmPhase::Preparing(step);
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportStarted => {
                let mut state = self;
                state.phase = ConfirmPhase::Importing;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportFailed(error) => {
                let mut state = self;
                state.phase = ConfirmPhase::Failed(error);
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportCompleted(album_id) => {
                let mut state = self;
                state.phase = ConfirmPhase::Completed(album_id);
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::SelectExactMatch(_)
            | CandidateEvent::ConfirmExactMatch
            | CandidateEvent::ExactMatchPrefetchStarted(_)
            | CandidateEvent::ExactMatchPrefetchComplete { .. }
            | CandidateEvent::SetExactMatchConfirmPending
            | CandidateEvent::SwitchToManualSearch
            | CandidateEvent::SwitchToMultipleExactMatches(_)
            | CandidateEvent::StartDiscIdLookup(_)
            | CandidateEvent::DiscIdLookupComplete { .. }
            | CandidateEvent::UpdateSearchField { .. }
            | CandidateEvent::SetSearchTab(_)
            | CandidateEvent::SetSearchSource(_)
            | CandidateEvent::StartSearch
            | CandidateEvent::CancelSearch
            | CandidateEvent::SearchComplete { .. }
            | CandidateEvent::SelectSearchResult(_)
            | CandidateEvent::ConfirmSearchResult
            | CandidateEvent::PrefetchStarted(_)
            | CandidateEvent::PrefetchComplete { .. }
            | CandidateEvent::SetConfirmPending
            | CandidateEvent::UpdateSearchResultCover { .. } => {
                CandidateState::Confirming(Box::new(self))
            }
        }
    }
}

// ============================================================================
// Global Import State
// ============================================================================

/// Global import workflow state (not per-candidate)
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ImportState {
    /// List of all detected candidates from the scan
    pub detected_candidates: Vec<DetectedCandidate>,
    /// Key of the currently selected candidate (release path)
    pub current_candidate_key: Option<String>,
    /// Per-candidate state machines
    pub candidate_states: std::collections::HashMap<String, CandidateState>,
    /// Loading state for candidates that haven't completed detection yet
    pub loading_candidates: std::collections::HashMap<String, bool>,
    /// Files in current folder (for UI reactivity)
    pub folder_files: CategorizedFileInfo,
    /// True while scanning a folder for release candidates
    pub is_scanning_candidates: bool,
    /// Track which candidates have already attempted DiscID lookup
    pub discid_lookup_attempted: std::collections::HashSet<String>,
    /// Which releases are selected for batch import
    pub selected_release_indices: Vec<usize>,
    /// Which release in the batch we're currently on
    pub current_release_index: usize,
    /// Which import source type (Folder, Torrent, CD)
    pub selected_import_source: crate::ImportSource,
    /// CD TOC info: (disc_id, first_track, last_track)
    pub cd_toc_info: Option<(String, u8, u8)>,
}

impl ImportState {
    /// Reset the import state to initial values
    pub fn reset(&mut self) {
        self.detected_candidates = Vec::new();
        self.current_candidate_key = None;
        self.candidate_states.clear();
        self.loading_candidates.clear();
        self.folder_files = CategorizedFileInfo::default();
        self.is_scanning_candidates = false;
        self.discid_lookup_attempted.clear();
        self.selected_release_indices = Vec::new();
        self.current_release_index = 0;
    }

    /// Get the current candidate's state (if any)
    pub fn current_candidate_state(&self) -> Option<&CandidateState> {
        let key = self.current_candidate_key.as_ref()?;
        self.candidate_states.get(key)
    }

    /// Switch to a different candidate by key
    pub fn switch_candidate(&mut self, new_key: Option<String>) {
        self.current_candidate_key = new_key.clone();
        if let Some(key) = new_key {
            if let Some(state) = self.candidate_states.get(&key) {
                self.folder_files = state.files().clone();
            }
        } else {
            self.folder_files = CategorizedFileInfo::default();
        }
    }

    /// Dispatch an event to the current candidate's state machine
    pub fn dispatch(&mut self, event: CandidateEvent) {
        let Some(key) = self.current_candidate_key.clone() else {
            return;
        };
        self.dispatch_to_candidate(&key, event);
    }

    /// Dispatch an event to a specific candidate by key
    pub fn dispatch_to_candidate(&mut self, key: &str, event: CandidateEvent) {
        if let Some(current_state) = self.candidate_states.remove(key) {
            let new_state = current_state.transition(event);
            self.candidate_states.insert(key.to_string(), new_state);
        }
    }

    /// Initialize state machine for a candidate after detection completes
    pub fn init_state_machine(
        &mut self,
        key: &str,
        files: CategorizedFileInfo,
        metadata: FolderMetadata,
    ) {
        // Sync folder_files only when this candidate is selected
        if self.current_candidate_key.as_deref() == Some(key) {
            self.folder_files = files.clone();
        }

        let initial_state = CandidateState::Identifying(IdentifyingState {
            files,
            metadata,
            mode: IdentifyMode::Created,
            auto_matches: vec![],
            selected_match_index: None,
            exact_match_prefetch: None,
            exact_match_confirm_pending: false,
            search_state: ManualSearchState::default(),
            discid_lookup_error: None,
            disc_id_not_found: None,
            source_disc_id: None,
        });
        self.candidate_states.insert(key.to_string(), initial_state);
        self.loading_candidates.remove(key);
    }

    /// Find the next release index that is not importing or imported
    pub fn find_next_available_release(&self) -> Option<usize> {
        for batch_idx in (self.current_release_index + 1)..self.selected_release_indices.len() {
            if let Some(&release_idx) = self.selected_release_indices.get(batch_idx) {
                if let Some(candidate) = self.detected_candidates.get(release_idx) {
                    let key = candidate.path.clone();
                    let skip = self
                        .candidate_states
                        .get(&key)
                        .map(|s| s.is_importing() || s.is_imported())
                        .unwrap_or(false);
                    if !skip {
                        return Some(batch_idx);
                    }
                }
            }
        }
        None
    }

    /// Check if there are more releases to import in the current batch
    pub fn has_more_releases(&self) -> bool {
        self.find_next_available_release().is_some()
    }

    /// Advance to the next release in the batch
    pub fn advance_to_next_release(&mut self) {
        if let Some(next_idx) = self.find_next_available_release() {
            self.current_release_index = next_idx;
        }
    }

    /// Get import step from current candidate state
    pub fn get_import_step(&self) -> crate::display_types::ImportStep {
        self.current_candidate_state()
            .map(|s| match s {
                CandidateState::Identifying(_) => crate::display_types::ImportStep::Identify,
                CandidateState::Confirming(_) => crate::display_types::ImportStep::Confirm,
            })
            .unwrap_or(crate::display_types::ImportStep::Identify)
    }

    /// Get identify mode from current candidate state
    pub fn get_identify_mode(&self) -> IdentifyMode {
        self.current_candidate_state()
            .and_then(|s| match s {
                CandidateState::Identifying(is) => Some(is.mode.clone()),
                _ => None,
            })
            .unwrap_or(IdentifyMode::Created)
    }

    /// Get metadata from current candidate state
    pub fn get_metadata(&self) -> Option<FolderMetadata> {
        self.current_candidate_state().map(|s| s.metadata().clone())
    }

    /// Get exact match candidates from current candidate state
    pub fn get_exact_match_candidates(&self) -> Vec<MatchCandidate> {
        self.current_candidate_state()
            .map(|s| match s {
                CandidateState::Identifying(is) => is.auto_matches.clone(),
                CandidateState::Confirming(cs) => cs.auto_matches.clone(),
            })
            .unwrap_or_default()
    }

    /// Get source disc ID from current candidate state
    pub fn get_source_disc_id(&self) -> Option<String> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.source_disc_id.clone(),
            CandidateState::Confirming(cs) => cs.source_disc_id.clone(),
        })
    }

    /// Get confirmed candidate from current candidate state
    pub fn get_confirmed_candidate(&self) -> Option<MatchCandidate> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Confirming(cs) => Some(cs.confirmed_candidate.clone()),
            _ => None,
        })
    }

    /// Get discid lookup error from current candidate state
    pub fn get_discid_lookup_error(&self) -> Option<String> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.discid_lookup_error.clone(),
            _ => None,
        })
    }

    /// Get disc ID that was searched but found no results
    pub fn get_disc_id_not_found(&self) -> Option<String> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.disc_id_not_found.clone(),
            _ => None,
        })
    }

    /// Get selected match index from current candidate state
    pub fn get_selected_match_index(&self) -> Option<usize> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.selected_match_index,
            _ => None,
        })
    }

    /// Get prefetch state for current tab's selected search result
    pub fn get_current_prefetch_state(&self) -> Option<PrefetchState> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => {
                is.search_state.current_tab_state().prefetch_state.clone()
            }
            _ => None,
        })
    }

    /// Get prefetch state for the selected exact match
    pub fn get_exact_match_prefetch_state(&self) -> Option<PrefetchState> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.exact_match_prefetch.clone(),
            _ => None,
        })
    }

    /// Get manual search state from current candidate
    pub fn get_search_state(&self) -> Option<ManualSearchState> {
        self.current_candidate_state().map(|s| match s {
            CandidateState::Identifying(is) => is.search_state.clone(),
            CandidateState::Confirming(cs) => cs.search_state.clone(),
        })
    }

    /// Get selected cover from current candidate state
    pub fn get_selected_cover(&self) -> Option<SelectedCover> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Confirming(cs) => cs.selected_cover.clone(),
            _ => None,
        })
    }

    /// Get display URL for the selected cover
    ///
    /// For remote covers, returns the URL directly.
    /// For local covers, looks up the display_url from artwork files.
    pub fn get_display_cover_url(&self) -> Option<String> {
        let selected = self.get_selected_cover()?;
        match selected {
            SelectedCover::Remote { url, .. } => Some(url),
            SelectedCover::Local { filename } => self
                .current_candidate_state()
                .and_then(|s| {
                    s.files()
                        .artwork
                        .iter()
                        .find(|f| f.name == filename)
                        .map(|f| f.display_url.clone())
                })
                .filter(|url| !url.is_empty()),
        }
    }

    /// Get whether files should be managed (copied to library storage)
    pub fn get_managed(&self) -> bool {
        self.current_candidate_state()
            .map(|s| match s {
                CandidateState::Confirming(cs) => cs.managed,
                _ => true,
            })
            .unwrap_or(true)
    }

    /// Get selected candidate index from current candidate key
    pub fn get_selected_candidate_index(&self) -> Option<usize> {
        self.current_candidate_key
            .as_ref()
            .and_then(|key| self.detected_candidates.iter().position(|c| &c.path == key))
    }

    /// Get the display name of the currently selected candidate
    pub fn get_current_candidate_name(&self) -> Option<String> {
        self.current_candidate_key.as_ref().and_then(|key| {
            self.detected_candidates
                .iter()
                .find(|c| &c.path == key)
                .map(|c| c.name.clone())
        })
    }

    /// Remove all incomplete candidates (those with corrupt/bad files)
    pub fn clear_incomplete_candidates(&mut self) {
        let incomplete_paths: Vec<String> = self
            .detected_candidates
            .iter()
            .filter(|c| {
                self.candidate_states
                    .get(&c.path)
                    .map(|s| {
                        let files = s.files();
                        files.bad_audio_count > 0 || files.bad_image_count > 0
                    })
                    .unwrap_or(false)
            })
            .map(|c| c.path.clone())
            .collect();

        if incomplete_paths.is_empty() {
            return;
        }

        for path in &incomplete_paths {
            self.detected_candidates.retain(|c| &c.path != path);
            self.candidate_states.remove(path);
            self.loading_candidates.remove(path);
            self.discid_lookup_attempted.remove(path);
        }

        if let Some(key) = &self.current_candidate_key {
            if incomplete_paths.contains(key) {
                if let Some(first) = self.detected_candidates.first() {
                    let new_key = first.path.clone();
                    self.switch_candidate(Some(new_key));
                } else {
                    self.switch_candidate(None);
                }
            }
        }
    }

    /// Remove a detected release by index
    pub fn remove_detected_release(&mut self, index: usize) {
        if index < self.detected_candidates.len() {
            let release_path = self.detected_candidates[index].path.clone();
            self.detected_candidates.remove(index);
            self.candidate_states.remove(&release_path);
            self.loading_candidates.remove(&release_path);
            self.discid_lookup_attempted.remove(&release_path);

            if self.current_candidate_key.as_deref() == Some(&release_path) {
                if let Some(first) = self.detected_candidates.first() {
                    let new_key = first.path.clone();
                    self.switch_candidate(Some(new_key));
                } else {
                    self.switch_candidate(None);
                }
            }
        }
    }
}
