//! CD import workflow view
//!
//! A 2-step flow for importing music from a CD:
//!
//! ## Step 1: Identify
//! User selects a CD drive, then the system identifies the CD via DiscID.
//! If ambiguous, user picks from candidates. If no match, user searches manually.
//!
//! ## Step 2: Confirm
//! User reviews the match, selects cover art and storage profile, then imports.
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down to children. Use lenses where possible.

use super::{
    CdRipperView, CdTocDisplayView, ConfirmationView, DiscIdLookupErrorView,
    ImportErrorDisplayView, ManualSearchPanelView, MultipleExactMatchesView, SelectedSourceView,
};
use crate::components::StorageProfile;
use crate::display_types::{
    CdDriveInfo, IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab, SelectedCover,
};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

/// Props for CD import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct CdImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === CD-specific state (not in ImportState) ===
    /// True while scanning for CD drives
    pub is_scanning: bool,
    /// Available CD drives
    pub drives: Vec<CdDriveInfo>,
    /// Currently selected drive path
    pub selected_drive: Option<String>,
    /// Callback when user selects a drive
    pub on_drive_select: EventHandler<String>,

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
    pub on_select_cover: EventHandler<SelectedCover>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    pub on_clear: EventHandler<()>,
    pub on_view_in_library: EventHandler<String>,
}

/// CD import workflow view
///
/// Only reads `current_candidate_key` via lens. Step routing is pushed
/// into `CdWorkflowContent` so this parent stays narrowly subscribed.
#[component]
pub fn CdImportView(props: CdImportViewProps) -> Element {
    let state = props.state;
    let candidate_key = state.current_candidate_key().read().clone();

    rsx! {
        div { class: "space-y-6",
            if candidate_key.is_none() {
                CdRipperView {
                    is_scanning: props.is_scanning,
                    drives: props.drives.clone(),
                    selected_drive: props.selected_drive.clone(),
                    on_drive_select: props.on_drive_select,
                }
            } else if let Some(ref key) = candidate_key {
                CdWorkflowContent {
                    key: "{key}",
                    state,
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

/// Step routing for CD workflow — reads candidate_states to determine step
#[component]
fn CdWorkflowContent(
    state: ReadStore<ImportState>,
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
    on_select_cover: EventHandler<SelectedCover>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    let cd_path = state
        .current_candidate_key()
        .read()
        .clone()
        .unwrap_or_default();
    let step = state
        .candidate_states()
        .read()
        .get(&cd_path)
        .map(|s| match s {
            CandidateState::Identifying(_) => ImportStep::Identify,
            CandidateState::Confirming(_) => ImportStep::Confirm,
        })
        .unwrap_or(ImportStep::Identify);

    match step {
        ImportStep::Identify => rsx! {
            CdIdentifyContent {
                state,
                cd_path,
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
                on_view_in_library,
            }
        },
        ImportStep::Confirm => rsx! {
            CdConfirmContent {
                state,
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

/// CD Identify content - reads state at leaf level
#[component]
fn CdIdentifyContent(
    state: ReadStore<ImportState>,
    cd_path: String,
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
    on_view_in_library: EventHandler<String>,
) -> Element {
    let is_looking_up = *state.is_looking_up().read();
    let toc_info = state
        .cd_toc_info()
        .read()
        .as_ref()
        .map(|(disc_id, first, last)| super::CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first,
            last_track: *last,
        });

    let current_key = state.current_candidate_key().read().clone();
    let candidate_states = state.candidate_states().read().clone();
    let cs = current_key.as_ref().and_then(|k| candidate_states.get(k));

    let (identify_mode, discid_lookup_error) = match cs {
        Some(CandidateState::Identifying(is)) => (is.mode.clone(), is.discid_lookup_error.clone()),
        _ => (IdentifyMode::Created, None),
    };

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected CD".to_string(),
                path: cd_path,
                on_clear,
                on_reveal: |_| {},
                CdTocDisplayView { toc: toc_info, is_reading: is_looking_up }
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
                        DiscIdLookupErrorView {
                            error_message: discid_lookup_error,
                            is_retrying: is_looking_up,
                            on_retry: on_retry_discid_lookup,
                        }
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

/// CD Confirm content - reads state at leaf level
#[component]
fn CdConfirmContent(
    state: ReadStore<ImportState>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_clear: EventHandler<()>,
    on_select_cover: EventHandler<SelectedCover>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    let cd_path = state
        .current_candidate_key()
        .read()
        .clone()
        .unwrap_or_default();
    let toc_info = state
        .cd_toc_info()
        .read()
        .as_ref()
        .map(|(disc_id, first, last)| super::CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first,
            last_track: *last,
        });

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
                ConfirmPhase::Preparing(msg) => (false, false, None, Some(msg.clone()), None),
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
                title: "Selected CD".to_string(),
                path: cd_path,
                on_clear,
                on_reveal: |_| {},
                CdTocDisplayView { toc: toc_info, is_reading: false }
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
