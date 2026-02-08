//! Torrent import workflow view
//!
//! A 2-step flow for importing music from a torrent:
//!
//! ## Step 1: Identify
//! User provides a .torrent file or magnet link, then the system identifies the music
//! via metadata. If ambiguous, user picks from candidates. If no match, user searches manually.
//!
//! ## Step 2: Confirm
//! User reviews the match, selects cover art and storage profile, then imports.
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down to children. Use lenses where possible.

use super::{
    ConfirmationView, DiscIdLookupErrorView, ImportErrorDisplayView, ManualSearchPanelView,
    MetadataDetectionPromptView, MultipleExactMatchesView, SelectedSourceView,
    TorrentFilesDisplayView, TorrentInfoDisplayView, TorrentTrackerDisplayView, TrackerStatus,
};
use crate::components::StorageProfile;
use crate::display_types::{
    IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab, SelectedCover,
    TorrentFileInfo, TorrentInfo,
};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState, ImportStateStoreExt};
use crate::{TorrentInputMode, TorrentInputView};
use dioxus::prelude::*;

/// Props for torrent import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct TorrentImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === Torrent-specific state (not in ImportState) ===
    /// Torrent info (parsed from .torrent file)
    pub torrent_info: Option<TorrentInfo>,
    /// Tracker connection statuses
    pub tracker_statuses: Vec<TrackerStatus>,
    /// Files in the torrent
    pub torrent_files: Vec<TorrentFileInfo>,
    /// Current input mode (file or magnet)
    pub input_mode: TorrentInputMode,
    /// True if dragging over drop zone
    pub is_dragging: bool,
    /// Callback when input mode changes
    pub on_mode_change: EventHandler<TorrentInputMode>,
    /// Callback when user clicks to select file
    pub on_file_select: EventHandler<()>,
    /// Callback when user submits magnet link
    pub on_magnet_submit: EventHandler<String>,

    // === External data ===
    /// Storage profiles (from app context)
    pub storage_profiles: ReadSignal<Vec<StorageProfile>>,

    // === Callbacks ===
    pub on_exact_match_select: EventHandler<usize>,
    pub on_confirm_exact_match: EventHandler<MatchCandidate>,
    pub on_switch_to_manual_search: EventHandler<()>,
    pub on_switch_to_exact_matches: EventHandler<String>,
    pub on_search_source_change: EventHandler<SearchSource>,
    pub on_search_tab_change: EventHandler<SearchTab>,
    pub on_artist_change: EventHandler<String>,
    pub on_album_change: EventHandler<String>,
    pub on_catalog_number_change: EventHandler<String>,
    pub on_barcode_change: EventHandler<String>,
    pub on_manual_match_select: EventHandler<usize>,
    pub on_search: EventHandler<()>,
    pub on_cancel_search: EventHandler<()>,
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    pub on_retry_cover: EventHandler<usize>,
    pub on_retry_discid_lookup: EventHandler<()>,
    pub on_detect_metadata: EventHandler<()>,
    pub on_select_cover: EventHandler<SelectedCover>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    pub on_clear: EventHandler<()>,
    pub on_view_in_library: EventHandler<String>,
}

/// Torrent import workflow view
///
/// Only reads `current_candidate_key` via lens. Step routing is pushed
/// into `TorrentWorkflowContent` so this parent stays narrowly subscribed.
#[component]
pub fn TorrentImportView(props: TorrentImportViewProps) -> Element {
    let state = props.state;
    let candidate_key = state.current_candidate_key().read().clone();

    rsx! {
        div {
            if candidate_key.is_none() {
                TorrentInputView {
                    input_mode: props.input_mode,
                    is_dragging: props.is_dragging,
                    on_mode_change: props.on_mode_change,
                    on_select_click: props.on_file_select,
                    on_magnet_submit: props.on_magnet_submit,
                }
            } else if let Some(ref key) = candidate_key {
                TorrentWorkflowContent {
                    key: "{key}",
                    state,
                    torrent_info: props.torrent_info.clone(),
                    tracker_statuses: props.tracker_statuses.clone(),
                    torrent_files: props.torrent_files.clone(),
                    storage_profiles: props.storage_profiles,
                    on_clear: props.on_clear,
                    on_exact_match_select: props.on_exact_match_select,
                    on_confirm_exact_match: props.on_confirm_exact_match,
                    on_switch_to_manual_search: props.on_switch_to_manual_search,
                    on_switch_to_exact_matches: props.on_switch_to_exact_matches,
                    on_search_source_change: props.on_search_source_change,
                    on_search_tab_change: props.on_search_tab_change,
                    on_artist_change: props.on_artist_change,
                    on_album_change: props.on_album_change,
                    on_catalog_number_change: props.on_catalog_number_change,
                    on_barcode_change: props.on_barcode_change,
                    on_manual_match_select: props.on_manual_match_select,
                    on_search: props.on_search,
                    on_cancel_search: props.on_cancel_search,
                    on_manual_confirm: props.on_manual_confirm,
                    on_retry_cover: props.on_retry_cover,
                    on_retry_discid_lookup: props.on_retry_discid_lookup,
                    on_detect_metadata: props.on_detect_metadata,
                    on_select_cover: props.on_select_cover,
                    on_storage_profile_change: props.on_storage_profile_change,
                    on_edit: props.on_edit,
                    on_confirm: props.on_confirm,
                    on_configure_storage: props.on_configure_storage,
                    on_view_in_library: props.on_view_in_library,
                }
            }
        }
    }
}

/// Step routing for torrent workflow — reads candidate_states to determine step
#[component]
fn TorrentWorkflowContent(
    state: ReadStore<ImportState>,
    torrent_info: Option<TorrentInfo>,
    tracker_statuses: Vec<TrackerStatus>,
    torrent_files: Vec<TorrentFileInfo>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_clear: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
    on_confirm_exact_match: EventHandler<MatchCandidate>,
    on_switch_to_manual_search: EventHandler<()>,
    on_switch_to_exact_matches: EventHandler<String>,
    on_search_source_change: EventHandler<SearchSource>,
    on_search_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_manual_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_cancel_search: EventHandler<()>,
    on_manual_confirm: EventHandler<MatchCandidate>,
    on_retry_cover: EventHandler<usize>,
    on_retry_discid_lookup: EventHandler<()>,
    on_detect_metadata: EventHandler<()>,
    on_select_cover: EventHandler<SelectedCover>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    let torrent_path = state
        .current_candidate_key()
        .read()
        .clone()
        .unwrap_or_default();
    let step = state
        .candidate_states()
        .read()
        .get(&torrent_path)
        .map(|s| match s {
            CandidateState::Identifying(_) => ImportStep::Identify,
            CandidateState::Confirming(_) => ImportStep::Confirm,
        })
        .unwrap_or(ImportStep::Identify);

    match step {
        ImportStep::Identify => rsx! {
            TorrentIdentifyContent {
                state,
                torrent_path,
                torrent_info,
                tracker_statuses,
                torrent_files,
                on_clear,
                on_exact_match_select,
                on_confirm_exact_match,
                on_switch_to_manual_search,
                on_switch_to_exact_matches,
                on_search_source_change,
                on_search_tab_change,
                on_artist_change,
                on_album_change,
                on_catalog_number_change,
                on_barcode_change,
                on_manual_match_select,
                on_search,
                on_cancel_search,
                on_manual_confirm,
                on_retry_cover,
                on_retry_discid_lookup,
                on_detect_metadata,
                on_view_in_library,
            }
        },
        ImportStep::Confirm => rsx! {
            TorrentConfirmContent {
                state,
                torrent_path,
                torrent_info,
                tracker_statuses,
                torrent_files,
                storage_profiles,
                on_clear,
                on_select_cover,
                on_storage_profile_change,
                on_edit,
                on_confirm,
                on_configure_storage,
                on_view_in_library,
            }
        },
    }
}

/// Torrent Identify content - reads state at leaf level
#[component]
fn TorrentIdentifyContent(
    state: ReadStore<ImportState>,
    torrent_path: String,
    torrent_info: Option<TorrentInfo>,
    tracker_statuses: Vec<TrackerStatus>,
    torrent_files: Vec<TorrentFileInfo>,
    on_clear: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
    on_confirm_exact_match: EventHandler<MatchCandidate>,
    on_switch_to_manual_search: EventHandler<()>,
    on_switch_to_exact_matches: EventHandler<String>,
    on_search_source_change: EventHandler<SearchSource>,
    on_search_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_manual_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_cancel_search: EventHandler<()>,
    on_manual_confirm: EventHandler<MatchCandidate>,
    on_retry_cover: EventHandler<usize>,
    on_retry_discid_lookup: EventHandler<()>,
    on_detect_metadata: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    let current_key = state.current_candidate_key().read().clone();
    let candidate_states = state.candidate_states().read().clone();
    let cs = current_key.as_ref().and_then(|k| candidate_states.get(k));

    let (identify_mode, discid_lookup_error, detected_metadata, has_searched) = match cs {
        Some(CandidateState::Identifying(is)) => (
            is.mode.clone(),
            is.discid_lookup_error.clone(),
            Some(is.metadata.clone()),
            is.search_state.any_tab_searched(),
        ),
        Some(CandidateState::Confirming(cs)) => (
            IdentifyMode::Created,
            None,
            Some(cs.metadata.clone()),
            cs.search_state.any_tab_searched(),
        ),
        None => (IdentifyMode::Created, None, None, false),
    };

    let show_metadata_detection_prompt =
        identify_mode == IdentifyMode::ManualSearch && detected_metadata.is_none() && !has_searched;

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected Torrent".to_string(),
                path: torrent_path,
                on_clear,
                on_reveal: |_| {},
                TorrentTrackerDisplayView { trackers: tracker_statuses }
                if let Some(ref info) = torrent_info {
                    TorrentInfoDisplayView { info: info.clone() }
                }
                TorrentFilesDisplayView { files: torrent_files }
            }
            match identify_mode {
                IdentifyMode::Created | IdentifyMode::DiscIdLookup(_) => rsx! {},
                IdentifyMode::MultipleExactMatches(_) => rsx! {
                    MultipleExactMatchesView {
                        state,
                        on_select: on_exact_match_select,
                        on_confirm: on_confirm_exact_match,
                        on_switch_to_manual_search,
                        on_view_in_library,
                    }
                },
                IdentifyMode::ManualSearch => rsx! {
                    if discid_lookup_error.is_some() {
                        DiscIdLookupErrorView { error_message: discid_lookup_error, on_retry: on_retry_discid_lookup }
                    }
                    if show_metadata_detection_prompt {
                        MetadataDetectionPromptView { on_detect: on_detect_metadata }
                    }
                    ManualSearchPanelView {
                        state,
                        on_search_source_change,
                        on_tab_change: on_search_tab_change,
                        on_artist_change,
                        on_album_change,
                        on_catalog_number_change,
                        on_barcode_change,
                        on_match_select: on_manual_match_select,
                        on_search,
                        on_cancel_search,
                        on_confirm: on_manual_confirm,
                        on_retry_cover,
                        on_view_in_library,
                        on_switch_to_exact_matches,
                    }
                },
            }
        }
    }
}

/// Torrent Confirm content - reads state at leaf level
#[component]
fn TorrentConfirmContent(
    state: ReadStore<ImportState>,
    torrent_path: String,
    torrent_info: Option<TorrentInfo>,
    tracker_statuses: Vec<TrackerStatus>,
    torrent_files: Vec<TorrentFileInfo>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_clear: EventHandler<()>,
    on_select_cover: EventHandler<SelectedCover>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    let current_key = state.current_candidate_key().read().clone();
    let candidate_states = state.candidate_states().read().clone();
    let cs = current_key.as_ref().and_then(|k| candidate_states.get(k));

    let (
        confirmed_candidate,
        selected_cover,
        display_cover_url,
        artwork_files,
        selected_profile_id,
        is_importing,
        is_completed,
        completed_album_id,
        preparing_step_text,
        import_error,
    ) = match cs {
        Some(CandidateState::Confirming(cs)) => {
            let cover_url = cs.selected_cover.as_ref().and_then(|sel| match sel {
                SelectedCover::Remote { url, .. } => Some(url.clone()),
                SelectedCover::Local { filename } => cs
                    .files
                    .artwork
                    .iter()
                    .find(|f| &f.name == filename)
                    .map(|f| f.display_url.clone())
                    .filter(|url| !url.is_empty()),
            });
            let (importing, completed, album_id, preparing, error) = match &cs.phase {
                ConfirmPhase::Ready => (false, false, None, None, None),
                ConfirmPhase::Preparing(msg) => (true, false, None, Some(msg.clone()), None),
                ConfirmPhase::Importing => (true, false, None, None, None),
                ConfirmPhase::Failed(err) => (false, false, None, None, Some(err.clone())),
                ConfirmPhase::Completed(id) => (false, true, Some(id.clone()), None, None),
            };
            (
                Some(cs.confirmed_candidate.clone()),
                cs.selected_cover.clone(),
                cover_url,
                cs.files.artwork.clone(),
                cs.selected_profile_id.clone(),
                importing,
                completed,
                album_id,
                preparing,
                error,
            )
        }
        _ => (
            None,
            None,
            None,
            vec![],
            None,
            false,
            false,
            None,
            None,
            None,
        ),
    };

    let Some(candidate) = confirmed_candidate else {
        return rsx! {};
    };

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected Torrent".to_string(),
                path: torrent_path,
                on_clear,
                on_reveal: |_| {},
                TorrentTrackerDisplayView { trackers: tracker_statuses }
                if let Some(ref info) = torrent_info {
                    TorrentInfoDisplayView { info: info.clone() }
                }
                TorrentFilesDisplayView { files: torrent_files }
            }
            ConfirmationView {
                candidate: candidate.clone(),
                selected_cover,
                display_cover_url,
                artwork_files,
                remote_cover_url: candidate.cover_url.clone(),
                storage_profiles,
                selected_profile_id,
                is_importing,
                is_completed,
                completed_album_id,
                preparing_step_text,
                on_select_cover,
                on_storage_profile_change,
                on_edit,
                on_confirm,
                on_configure_storage,
                on_view_in_library,
            }
            ImportErrorDisplayView { error_message: import_error }
        }
    }
}
