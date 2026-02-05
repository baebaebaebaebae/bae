//! Folder import workflow view
//!
//! Multi-pane layout for importing music from folders:
//!
//! ## Initial State
//! User picks a folder (full-width folder selector).
//!
//! ## Working State (after folder selected)
//! Three-column layout with context header:
//! - Left sidebar: List of detected releases with status
//! - Detail area (right of sidebar):
//!   - Header: Shows selected folder name and current step (Identifying/Confirming)
//!   - Files column (narrow): SmartFileDisplay showing folder contents
//!   - Workflow area (main): Identify/Confirm workflow for selected release
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down through the tree. Use lenses to read
//! individual fields. Only call `.read()` at the leaf level where you
//! actually render values.

use super::{
    ConfirmationView, DiscIdPill, DiscIdSource, ImportErrorDisplayView, LoadingIndicator,
    ManualSearchPanelView, MultipleExactMatchesView, SmartFileDisplayView,
};
use crate::components::icons::{CloudOffIcon, LoaderIcon};
use crate::components::StorageProfile;
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::components::{PanelPosition, ResizablePanel, ResizeDirection};
use crate::display_types::{
    IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab, SelectedCover,
};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

// ============================================================================
// Main Folder Import View
// ============================================================================

/// Props for the folder import workflow view (main content area only)
///
/// The sidebar (ReleaseSidebarView) is rendered separately by the parent
/// and passed to ImportView.
#[derive(Clone, PartialEq, Props)]
pub struct FolderImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === UI-only state (not in ImportState) ===
    /// Which gallery item is currently viewed in the lightbox (None = closed)
    pub viewing_index: ReadSignal<Option<usize>>,
    /// Loaded text file content (for viewed file, if it's a text file)
    #[props(default)]
    pub text_file_content: Option<Result<String, String>>,

    // === External data (not in ImportState) ===
    /// Storage profiles (from app context)
    pub storage_profiles: ReadSignal<Vec<StorageProfile>>,

    // === Callbacks ===
    pub on_folder_select_click: EventHandler<()>,
    pub on_view_change: EventHandler<Option<usize>>,
    pub on_skip_detection: EventHandler<()>,
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
    pub on_view_in_library: EventHandler<String>,
}

/// Folder import workflow view - main content area only
///
/// The sidebar (ReleaseSidebarView) is rendered by the parent via ImportView.
/// This component renders the workflow steps and files dock.
#[component]
pub fn FolderImportView(props: FolderImportViewProps) -> Element {
    let state = props.state;

    // Lens reads only — no .read() on full state
    let candidate_key = state.current_candidate_key().read().clone();
    let is_empty = state.detected_candidates().read().is_empty();
    let is_scanning = *state.is_scanning_candidates().read();

    rsx! {
        if is_empty {
            // Empty state - centered
            div { class: "flex-1 flex flex-col",
                EmptyView {
                    is_scanning,
                    on_folder_select: props.on_folder_select_click,
                }
            }
        } else if let Some(key) = candidate_key {
            // Detail pane: keyed on candidate so it remounts when switching releases
            div {
                key: "{key}",
                class: "flex-1 flex flex-col min-h-0 m-2 ml-0 bg-gray-900/40 rounded-xl overflow-clip",
                // Context header showing folder name and step
                DetailHeader { state }

                // Content: Files | Workflow
                div { class: "flex-1 flex flex-row min-h-0",
                    // Left: Files column (narrow, scrollable)
                    FilesColumn {
                        state,
                        viewing_index: props.viewing_index,
                        text_file_content: props.text_file_content.clone(),
                        on_view_change: props.on_view_change,
                    }

                    // Right: Workflow (main, fills remaining space)
                    div { class: "flex-1 min-h-0 flex flex-col bg-gray-800/30",
                        WorkflowContent {
                            state,
                            storage_profiles: props.storage_profiles,
                            on_skip_detection: props.on_skip_detection,
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
    }
}

// ============================================================================
// Empty State
// ============================================================================

/// Empty state shown when no candidates are detected yet
#[component]
fn EmptyView(is_scanning: bool, on_folder_select: EventHandler<()>) -> Element {
    rsx! {
        div { class: "flex-1 flex items-center justify-center px-6 py-4",
            div { class: "w-full max-w-3xl text-center space-y-3",
                if is_scanning {
                    LoaderIcon { class: "w-5 h-5 text-gray-400 animate-spin mx-auto" }
                    p { class: "text-sm text-gray-400", "Scanning folder for releases..." }
                } else {
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Medium,
                        onclick: move |_| on_folder_select.call(()),
                        "Select folder"
                    }
                    p { class: "text-sm text-gray-400", "Scans for folders with music files" }
                }
            }
        }
    }
}

/// Main workflow content area with step routing
#[component]
fn WorkflowContent(
    state: ReadStore<ImportState>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_skip_detection: EventHandler<()>,
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
    let step = state
        .current_candidate_key()
        .read()
        .as_ref()
        .and_then(|k| {
            state.candidate_states().read().get(k).map(|s| match s {
                CandidateState::Identifying(_) => ImportStep::Identify,
                CandidateState::Confirming(_) => ImportStep::Confirm,
            })
        })
        .unwrap_or(ImportStep::Identify);

    rsx! {
        div { class: "flex-1 min-h-0 overflow-auto bg-gray-900/40 rounded-tl-xl flex flex-col",
            match step {
                ImportStep::Identify => rsx! {
                    IdentifyStep {
                        state,
                        on_skip_detection,
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
                    ConfirmStep {
                        state,
                        storage_profiles,
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
    }
}

// ============================================================================
// Step 2: Identify
// ============================================================================

/// Identify step - passes state down, reads at leaf level
#[component]
fn IdentifyStep(
    state: ReadStore<ImportState>,
    on_skip_detection: EventHandler<()>,
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
    let mode = state
        .current_candidate_key()
        .read()
        .as_ref()
        .and_then(|k| {
            state
                .candidate_states()
                .read()
                .get(k)
                .and_then(|s| match s {
                    CandidateState::Identifying(is) => Some(is.mode.clone()),
                    _ => None,
                })
        })
        .unwrap_or(IdentifyMode::Created);

    rsx! {
        match mode {
            IdentifyMode::Created => rsx! {},
            IdentifyMode::DiscIdLookup(disc_id) => rsx! {
                DiscIdLookupProgressView {
                    state,
                    disc_id,
                    on_skip: on_skip_detection,
                    on_retry: on_retry_discid_lookup,
                }
            },
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

// ============================================================================
// Step 3: Confirm
// ============================================================================

/// Confirm step - reads state at leaf level for display
#[component]
fn ConfirmStep(
    state: ReadStore<ImportState>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_select_cover: EventHandler<SelectedCover>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_in_library: EventHandler<String>,
) -> Element {
    // Read state at this level to get confirm-specific data
    let st = state.read();
    let confirmed_candidate = st.get_confirmed_candidate();
    let selected_cover = st.get_selected_cover();
    let display_cover_url = st.get_display_cover_url();
    let artwork_files = st
        .current_candidate_state()
        .map(|s| s.files().artwork.clone())
        .unwrap_or_default();
    let selected_profile_id = st.get_storage_profile_id();

    let (is_importing, preparing_step_text, import_error) = st
        .current_candidate_state()
        .and_then(|s| match s {
            CandidateState::Confirming(cs) => Some(&cs.phase),
            _ => None,
        })
        .map(|phase| match phase {
            ConfirmPhase::Ready => (false, None, None),
            ConfirmPhase::Preparing(msg) => (false, Some(msg.clone()), None),
            ConfirmPhase::Importing => (true, None, None),
            ConfirmPhase::Failed(err) => (false, None, Some(err.clone())),
            ConfirmPhase::Completed => (false, None, None),
        })
        .unwrap_or((false, None, None));

    let Some(candidate) = confirmed_candidate else {
        return rsx! {};
    };

    rsx! {
        div { class: "space-y-6",
            ConfirmationView {
                candidate: candidate.clone(),
                selected_cover,
                display_cover_url,
                artwork_files,
                remote_cover_url: candidate.cover_url.clone(),
                storage_profiles,
                selected_profile_id,
                is_importing,
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

// ============================================================================
// Detail Header
// ============================================================================

/// Header showing the selected folder name
#[component]
fn DetailHeader(state: ReadStore<ImportState>) -> Element {
    let folder_path = state.current_candidate_key().read().clone();
    let folder_name = folder_path.as_ref().and_then(|key| {
        state
            .detected_candidates()
            .read()
            .iter()
            .find(|c| &c.path == key)
            .map(|c| c.name.clone())
    });

    let Some(name) = folder_name else {
        return rsx! {};
    };

    let tooltip = folder_path.unwrap_or_default();

    rsx! {
        div { class: "flex-shrink-0 px-4 py-4 bg-gray-800/30",
            div { class: "cursor-default", title: "{tooltip}",
                span { class: "text-[0.9375rem] font-medium text-gray-300 truncate select-text",
                    "{name}"
                }
            }
        }
    }
}

// ============================================================================
// Files Column
// ============================================================================

/// Vertical files column showing folder contents
#[component]
fn FilesColumn(
    state: ReadStore<ImportState>,
    viewing_index: ReadSignal<Option<usize>>,
    text_file_content: Option<Result<String, String>>,
    on_view_change: EventHandler<Option<usize>>,
) -> Element {
    let files = state
        .current_candidate_key()
        .read()
        .as_ref()
        .and_then(|k| {
            state
                .candidate_states()
                .read()
                .get(k)
                .map(|s| s.files().clone())
        })
        .unwrap_or_default();

    // Snap to image grid widths when images present
    // thumbnail=72px, gap=8px, padding=32px → width(N) = 80N + 24
    let has_images = !files.artwork.is_empty();
    let snap_points = if has_images {
        Some(vec![184.0, 264.0, 344.0]) // 2, 3, 4 columns
    } else {
        None
    };

    rsx! {
        ResizablePanel {
            storage_key: "import-files-width",
            min_size: 184.0,
            max_size: 344.0,
            default_size: 264.0,
            grabber_span_ratio: 0.8,
            direction: ResizeDirection::Horizontal,
            position: PanelPosition::Relative,
            snap_points,
            div { class: "h-full overflow-y-auto pt-1 px-4 pb-4 bg-gray-800/30",
                SmartFileDisplayView {
                    files,
                    viewing_index,
                    text_file_content,
                    on_view_change,
                }
            }
        }
    }
}

// ============================================================================
// DiscID Lookup Progress
// ============================================================================

/// Shown while looking up the release via MusicBrainz disc ID.
/// Also displays error state if the lookup fails.
#[component]
fn DiscIdLookupProgressView(
    state: ReadStore<ImportState>,
    disc_id: String,
    on_skip: EventHandler<()>,
    on_retry: EventHandler<()>,
) -> Element {
    let error = state.current_candidate_key().read().as_ref().and_then(|k| {
        state
            .candidate_states()
            .read()
            .get(k)
            .and_then(|s| match s {
                CandidateState::Identifying(is) => is.discid_lookup_error.clone(),
                _ => None,
            })
    });
    let is_retrying = *state.is_looking_up().read();

    rsx! {
        div { class: "flex-1 flex justify-center items-center",
            div { class: "text-center space-y-2 max-w-md",
                if error.is_none() {
                    // Loading state
                    LoadingIndicator { message: "Checking MusicBrainz...".to_string() }
                    p { class: "text-xs text-gray-500 flex items-center justify-center gap-2 pt-1",
                        "Disc ID:"
                        DiscIdPill {
                            disc_id: disc_id.clone(),
                            source: DiscIdSource::Files,
                            tooltip_placement: crate::floating_ui::Placement::Bottom,
                        }
                    }
                } else if error.is_some() {
                    // Error state - mirrors loading state structure
                    p { class: "text-sm text-gray-300 flex items-center justify-center gap-2",
                        CloudOffIcon { class: "w-5 h-5 text-gray-400" }
                        "MusicBrainz unavailable"
                    }
                    p { class: "text-xs text-gray-500 flex items-center justify-center gap-2 pt-1",
                        "Disc ID:"
                        DiscIdPill {
                            disc_id: disc_id.clone(),
                            source: DiscIdSource::Files,
                            tooltip_placement: crate::floating_ui::Placement::Bottom,
                        }
                    }

                    // Actions
                    div { class: "flex justify-center gap-2 pt-3",
                        Button {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Small,
                            onclick: move |_| on_skip.call(()),
                            "Search manually"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Small,
                            disabled: is_retrying,
                            loading: is_retrying,
                            onclick: move |_| on_retry.call(()),
                            if is_retrying {
                                "Retrying..."
                            } else {
                                "Retry"
                            }
                        }
                    }
                }
            }
        }
    }
}
