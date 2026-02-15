//! Candidate sidebar view component for import

use std::collections::HashMap;

use crate::components::helpers::{ConfirmDialogView, Tooltip, TOOLTIP_PADDING_X};
use crate::components::icons::{
    CheckIcon, EllipsisIcon, FolderIcon, LoaderIcon, PlusIcon, TrashIcon, XIcon,
};
use crate::components::{MenuDropdown, MenuItem};
use crate::display_types::{AudioContentInfo, DetectedCandidateStatus};
use crate::floating_ui::Placement;
use crate::platform;
use crate::stores::import::{CandidateState, ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

pub const MIN_SIDEBAR_WIDTH: f64 = 350.0;
pub const MAX_SIDEBAR_WIDTH: f64 = 500.0;
pub const DEFAULT_SIDEBAR_WIDTH: f64 = 375.0; // w-100

/// Compute the display status of a candidate from its state machine.
fn compute_status(
    candidate_states: &HashMap<String, CandidateState>,
    path: &str,
) -> DetectedCandidateStatus {
    candidate_states
        .get(path)
        .map(|s| {
            let files = s.files();
            if files.bad_audio_count > 0 || files.bad_image_count > 0 {
                let good_audio_count = match &files.audio {
                    AudioContentInfo::CueFlacPairs(p) => p.len(),
                    AudioContentInfo::TrackFiles(t) => t.len(),
                };
                return DetectedCandidateStatus::Incomplete {
                    bad_audio_count: files.bad_audio_count,
                    total_audio_count: good_audio_count + files.bad_audio_count,
                    bad_image_count: files.bad_image_count,
                };
            }
            if s.is_imported() {
                DetectedCandidateStatus::Imported
            } else if s.is_importing() {
                DetectedCandidateStatus::Importing
            } else {
                DetectedCandidateStatus::Pending
            }
        })
        .unwrap_or(DetectedCandidateStatus::Pending)
}

/// Sidebar showing detected candidates with selection and status
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn ReleaseSidebarView(
    /// Import state store - read at leaf level
    state: ReadStore<ImportState>,
    /// Called when a candidate is selected
    on_select: EventHandler<usize>,
    /// Called to add a folder
    on_add_folder: EventHandler<()>,
    /// Called to remove a folder from the list
    on_remove: EventHandler<usize>,
    /// Called to clear all folders
    on_clear_all: EventHandler<()>,
    /// Called to clear incomplete folders only
    on_clear_incomplete: EventHandler<()>,
    /// Called to open a folder in the native file manager
    on_open_folder: EventHandler<String>,
) -> Element {
    let is_scanning = *state.is_scanning_candidates().read();
    let detected = state.detected_candidates().read().clone();
    let candidate_states = state.candidate_states().read().clone();
    let current_key = state.current_candidate_key().read().clone();

    // Inline get_selected_candidate_index
    let selected_index = current_key
        .as_ref()
        .and_then(|key| detected.iter().position(|c| &c.path == key));

    let has_incomplete = detected.iter().any(|c| {
        matches!(
            compute_status(&candidate_states, &c.path),
            DetectedCandidateStatus::Incomplete { .. }
        )
    });

    let mut show_menu = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_menu.into();
    let mut show_clear_confirm = use_signal(|| false);
    let is_clear_confirm_open: ReadSignal<bool> = show_clear_confirm.into();
    // Static ID - only one sidebar menu per page
    let anchor_id = "release-sidebar-menu";

    // Deferred selection: when the dropdown menu is open and a candidate is clicked,
    // the popover light dismiss + candidate state change in the same render cycle
    // crashes wry-bindgen (U8BufferEmpty). Store the index here, close the menu,
    // and let the effect fire on_select after the close render completes.
    let mut pending_select: Signal<Option<usize>> = use_signal(|| None);
    use_effect(move || {
        if let Some(index) = pending_select() {
            pending_select.set(None);
            on_select.call(index);
        }
    });

    rsx! {
        // Panel container - padding comes from parent ImportView
        div { class: "flex-1 px-2 pb-2 min-w-0 h-full",
            div { class: "flex flex-col h-full min-w-0 bg-surface-raised rounded-xl shadow-lg shadow-black/10 overflow-clip",
                // Header
                div { class: "relative pt-2.5 px-3 pb-1 flex items-center justify-between",
                    span { class: "text-xs font-medium text-gray-300",
                        {
                            let count = detected.len();
                            if count == 0 {
                                "".to_string()
                            } else if count == 1 {
                                "1 possible release".to_string()
                            } else {
                                format!("{} possible releases", count)
                            }
                        }
                    }
                    div { class: "flex items-center gap-1.5",
                        if is_scanning {
                            LoaderIcon { class: "w-3.5 h-3.5 text-gray-400 animate-spin" }
                        }
                        if detected.is_empty() {
                            Tooltip {
                                text: "Scan folder",
                                placement: Placement::Top,
                                nowrap: true,
                                button {
                                    class: "p-1.5 text-gray-400 hover:text-white transition-colors rounded-md hover:bg-white/5",
                                    onclick: move |_| on_add_folder.call(()),
                                    PlusIcon { class: "w-4 h-4" }
                                }
                            }
                        } else {
                            Tooltip {
                                text: "More",
                                placement: Placement::Top,
                                nowrap: true,
                                button {
                                    id: "{anchor_id}",
                                    class: "p-1.5 text-gray-400 hover:text-white transition-colors rounded-md hover:bg-white/5",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        show_menu.set(!show_menu());
                                    },
                                    EllipsisIcon { class: "w-4 h-4" }
                                }
                            }
                        }
                    }
                }

                // Dropdown menu for candidates list
                if !detected.is_empty() {
                    MenuDropdown {
                        anchor_id: anchor_id.to_string(),
                        is_open,
                        on_close: move |_| show_menu.set(false),
                        MenuItem {
                            onclick: move |_| {
                                show_menu.set(false);
                                on_add_folder.call(());
                            },
                            FolderIcon { class: "w-3.5 h-3.5 text-gray-400" }
                            span { "Add" }
                        }
                        if has_incomplete {
                            MenuItem {
                                onclick: move |_| {
                                    show_menu.set(false);
                                    on_clear_incomplete.call(());
                                },
                                TrashIcon { class: "w-3.5 h-3.5 text-gray-400" }
                                span { "Clear incomplete" }
                            }
                        }
                        MenuItem {
                            onclick: move |_| {
                                show_menu.set(false);
                                show_clear_confirm.set(true);
                            },
                            TrashIcon { class: "w-3.5 h-3.5 text-gray-400" }
                            span { "Clear all" }
                        }
                    }
                }

                // Divider
                div { class: "mx-1.5 mb-1.5 border-b border-white/10" }

                // Folder list
                div { class: "flex-1 flex flex-col overflow-y-auto p-1.5 pt-0 space-y-0.5 min-w-0",
                    for (index , candidate) in detected.iter().enumerate() {
                        CandidateRow {
                            key: "{index}",
                            index,
                            name: candidate.name.clone(),
                            path: candidate.path.clone(),
                            status: compute_status(&candidate_states, &candidate.path),
                            is_selected: selected_index == Some(index),
                            on_select: move |index: usize| {
                                if *show_menu.peek() {
                                    show_menu.set(false);
                                    pending_select.set(Some(index));
                                } else {
                                    on_select.call(index);
                                }
                            },
                            on_open_folder,
                            on_remove,
                        }
                    }
                }
            }
        }

        // Confirm dialog rendered outside the panel so it's not clipped
        ConfirmDialogView {
            is_open: is_clear_confirm_open,
            title: "Clear all folders?".to_string(),
            message: "This will remove all detected folders from the list.".to_string(),
            confirm_label: "Clear".to_string(),
            cancel_label: "Cancel".to_string(),
            is_destructive: true,
            on_confirm: move |_| {
                show_clear_confirm.set(false);
                on_clear_all.call(());
            },
            on_cancel: move |_| {
                show_clear_confirm.set(false);
            },
        }
    }
}

/// Format the incomplete reason message for display.
fn incomplete_message(
    bad_audio_count: usize,
    total_audio_count: usize,
    bad_image_count: usize,
) -> String {
    match (bad_audio_count > 0, bad_image_count > 0) {
        (true, true) => format!(
            "{} of {} tracks incomplete, {} corrupt image{}",
            bad_audio_count,
            total_audio_count,
            bad_image_count,
            if bad_image_count == 1 { "" } else { "s" },
        ),
        (true, false) => format!(
            "{} of {} tracks incomplete",
            bad_audio_count, total_audio_count,
        ),
        (false, true) => format!(
            "{} corrupt image{}",
            bad_image_count,
            if bad_image_count == 1 { "" } else { "s" },
        ),
        (false, false) => String::new(),
    }
}

/// Single candidate row with tooltip on the folder icon.
#[component]
fn CandidateRow(
    index: usize,
    name: String,
    path: String,
    status: DetectedCandidateStatus,
    is_selected: bool,
    on_select: EventHandler<usize>,
    on_open_folder: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    let is_incomplete = matches!(status, DetectedCandidateStatus::Incomplete { .. });
    let is_removable = matches!(
        status,
        DetectedCandidateStatus::Pending | DetectedCandidateStatus::Incomplete { .. }
    );

    rsx! {
        div {
            class: format!(
                "group w-full flex items-center gap-2.5 pl-3 pr-2 py-2 rounded-lg transition-all duration-150 min-w-0 {}",
                if is_incomplete {
                    "text-gray-500 cursor-default"
                } else if is_selected {
                    "bg-hover text-white cursor-pointer"
                } else {
                    "text-gray-200 hover:bg-surface-overlay hover:text-white cursor-pointer"
                },
            ),
            onclick: move |_| {
                if !is_incomplete {
                    on_select.call(index);
                }
            },

            match &status {
                DetectedCandidateStatus::Pending => {
                    let open_path = path.clone();
                    rsx! {
                        Tooltip {
                            text: platform::reveal_in_file_manager().to_string(),
                            placement: Placement::TopStart,
                            nowrap: true,
                            cross_axis_offset: -TOOLTIP_PADDING_X,
                            button {
                                class: "flex-shrink-0 text-gray-400 hover:text-white transition-colors",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    on_open_folder.call(open_path.clone());
                                },
                                FolderIcon { class: "w-4 h-4" }
                            }
                        }
                    }
                }
                DetectedCandidateStatus::Importing => rsx! {
                    LoaderIcon { class: "w-4 h-4 flex-shrink-0 text-blue-400 animate-spin" }
                },
                DetectedCandidateStatus::Imported => rsx! {
                    CheckIcon { class: "w-4 h-4 flex-shrink-0 text-green-500" }
                },
                DetectedCandidateStatus::Incomplete { .. } => {
                    let open_path = path.clone();
                    rsx! {
                        Tooltip {
                            text: platform::reveal_in_file_manager().to_string(),
                            placement: Placement::TopStart,
                            nowrap: true,
                            cross_axis_offset: -TOOLTIP_PADDING_X,
                            button {
                                class: "flex-shrink-0 text-gray-600 hover:text-white transition-colors",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    on_open_folder.call(open_path.clone());
                                },
                                FolderIcon { class: "w-4 h-4" }
                            }
                        }
                    }
                }
            }

            div { class: "flex-1 min-w-0",
                div { class: format!("text-xs truncate {}", if is_incomplete { "text-gray-500" } else { "" }),
                    {name}
                }
                if let DetectedCandidateStatus::Incomplete {
                    bad_audio_count,
                    total_audio_count,
                    bad_image_count,
                } = &status
                {
                    div { class: "text-[10px] text-gray-600 truncate",
                        {incomplete_message(*bad_audio_count, *total_audio_count, *bad_image_count)}
                    }
                }
            }

            if is_removable {
                Tooltip { text: "Remove", placement: Placement::Top, nowrap: true,
                    button {
                        class: "flex-shrink-0 opacity-0 group-hover:opacity-100 p-1 text-gray-400 hover:text-white rounded transition-opacity",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            on_remove.call(index);
                        },
                        XIcon { class: "w-3.5 h-3.5" }
                    }
                }
            }
        }
    }
}
