//! Storage modal — shows storage status, file list, and transfer actions

use crate::components::icons::{
    AlertTriangleIcon, ArrowRightLeftIcon, CloudIcon, DownloadIcon, FolderIcon, HardDriveIcon,
    LoaderIcon, LockIcon, XIcon,
};
use crate::components::settings::StorageProfile;
use crate::components::utils::format_file_size;
use crate::components::Modal;
use crate::display_types::File;
use crate::stores::album_detail::TransferProgressState;
use dioxus::prelude::*;

#[component]
pub fn StorageModal(
    is_open: ReadSignal<bool>,
    on_close: EventHandler<()>,
    files: Vec<File>,
    storage_profile: Option<StorageProfile>,
    transfer_progress: Option<TransferProgressState>,
    transfer_error: Option<String>,
    available_profiles: Vec<StorageProfile>,
    on_transfer_to_profile: EventHandler<String>,
    on_eject: EventHandler<()>,
) -> Element {
    let total_size: i64 = files
        .iter()
        .map(|f| f.file_size)
        .collect::<Vec<_>>()
        .iter()
        .sum();
    let is_self_managed = storage_profile.is_none();
    let is_transferring = transfer_progress.is_some();

    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                // Header
                div { class: "flex items-center justify-between px-6 pt-6 pb-4 border-b border-gray-700",
                    h2 { class: "text-xl font-bold text-white", "Storage" }
                    button {
                        class: "text-gray-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }

                div { class: "p-6 overflow-y-auto flex-1 space-y-6",
                    // Current storage status
                    StorageStatusSection {
                        storage_profile: storage_profile.clone(),
                        total_size,
                        file_count: files.len(),
                    }

                    // Transfer progress
                    if let Some(ref progress) = transfer_progress {
                        TransferProgressSection { progress: progress.clone() }
                    }

                    // Transfer error
                    if let Some(ref error) = transfer_error {
                        div { class: "flex items-start gap-3 p-4 bg-red-900/30 border border-red-700/50 rounded-lg",
                            AlertTriangleIcon { class: "w-5 h-5 text-red-400 shrink-0 mt-0.5" }
                            div {
                                div { class: "text-sm font-medium text-red-300", "Transfer failed" }
                                div { class: "text-xs text-red-400 mt-1", {error.clone()} }
                            }
                        }
                    }

                    // Transfer actions
                    if !is_transferring {
                        TransferActionsSection {
                            is_self_managed,
                            storage_profile: storage_profile.clone(),
                            available_profiles,
                            on_transfer_to_profile,
                            on_eject,
                        }
                    }

                    // Files section
                    div {
                        div { class: "text-sm font-medium text-gray-300 mb-3", "Files ({files.len()})" }
                        if files.is_empty() {
                            div { class: "text-gray-400 text-center py-8", "No files found" }
                        } else {
                            div { class: "space-y-2",
                                for file in files.iter() {
                                    div { class: "flex items-center justify-between py-2 px-3 bg-gray-700/50 rounded hover:bg-gray-700 transition-colors",
                                        div { class: "flex-1",
                                            div { class: "text-white text-sm font-medium",
                                                {file.filename.clone()}
                                            }
                                            div { class: "text-gray-400 text-xs mt-1",
                                                {format!("{} • {}", format_file_size(file.file_size), file.format)}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StorageStatusSection(
    storage_profile: Option<StorageProfile>,
    total_size: i64,
    file_count: usize,
) -> Element {
    match storage_profile {
        Some(ref profile) => {
            let is_cloud = profile.location == crate::components::settings::StorageLocation::Cloud;
            rsx! {
                div { class: "p-4 bg-gray-700/50 rounded-lg space-y-2",
                    div { class: "flex items-center gap-3",
                        div { class: "p-2 rounded-lg bg-blue-500/20",
                            if is_cloud {
                                CloudIcon { class: "w-5 h-5 text-blue-400" }
                            } else {
                                HardDriveIcon { class: "w-5 h-5 text-blue-400" }
                            }
                        }
                        div {
                            div { class: "text-sm font-medium text-white", {profile.name.clone()} }
                            div { class: "text-xs text-gray-400 mt-0.5",
                                if is_cloud {
                                    {
                                        format!(
                                            "Cloud storage • {} files • {}",
                                            file_count,
                                            format_file_size(total_size),
                                        )
                                    }
                                } else {
                                    {
                                        format!(
                                            "Local storage • {} files • {}",
                                            file_count,
                                            format_file_size(total_size),
                                        )
                                    }
                                }
                            }
                        }
                        if profile.encrypted {
                            div { class: "ml-auto",
                                LockIcon { class: "w-4 h-4 text-yellow-400" }
                            }
                        }
                    }
                }
            }
        }
        None => {
            rsx! {
                div { class: "p-4 bg-gray-700/50 rounded-lg",
                    div { class: "flex items-center gap-3",
                        div { class: "p-2 rounded-lg bg-gray-600/50",
                            FolderIcon { class: "w-5 h-5 text-gray-400" }
                        }
                        div {
                            div { class: "text-sm font-medium text-white", "Self-managed" }
                            div { class: "text-xs text-gray-400 mt-0.5",
                                {
                                    format!(
                                        "Files referenced from original location • {} files • {}",
                                        file_count,
                                        format_file_size(total_size),
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TransferProgressSection(progress: TransferProgressState) -> Element {
    let overall_percent = if progress.total_files > 0 {
        let file_weight = 100.0 / progress.total_files as f64;
        let completed = progress.file_index as f64 * file_weight;
        let current = progress.percent as f64 / 100.0 * file_weight;
        (completed + current) as u8
    } else {
        0
    };

    rsx! {
        div { class: "p-4 bg-blue-900/20 border border-blue-700/30 rounded-lg space-y-3",
            div { class: "flex items-center gap-2",
                LoaderIcon { class: "w-4 h-4 text-blue-400 animate-spin" }
                div { class: "text-sm font-medium text-blue-300", "Transferring..." }
            }
            div { class: "text-xs text-gray-400",
                {
                    format!(
                        "File {} of {}: {}",
                        progress.file_index + 1,
                        progress.total_files,
                        progress.filename,
                    )
                }
            }
            // Progress bar
            div { class: "w-full bg-gray-700 rounded-full h-2",
                div {
                    class: "bg-blue-500 h-2 rounded-full transition-all duration-300",
                    style: "width: {overall_percent}%",
                }
            }
        }
    }
}

#[component]
fn TransferActionsSection(
    is_self_managed: bool,
    storage_profile: Option<StorageProfile>,
    available_profiles: Vec<StorageProfile>,
    on_transfer_to_profile: EventHandler<String>,
    on_eject: EventHandler<()>,
) -> Element {
    // Filter out the current profile from available targets
    let current_profile_id = storage_profile.as_ref().map(|p| p.id.clone());
    let target_profiles: Vec<&StorageProfile> = available_profiles
        .iter()
        .filter(|p| Some(&p.id) != current_profile_id.as_ref())
        .collect();

    let has_targets = !target_profiles.is_empty();
    let can_eject = !is_self_managed;

    if !has_targets && !can_eject {
        return rsx! {};
    }

    rsx! {
        div { class: "space-y-3",
            div {
                div { class: "text-sm font-medium text-gray-300",
                    if is_self_managed {
                        "Copy to storage"
                    } else {
                        "Transfer"
                    }
                }
                if is_self_managed {
                    div { class: "text-xs text-gray-500 mt-1", "Original files will not be modified" }
                }
            }

            if has_targets {
                div { class: "space-y-2",
                    for profile in target_profiles {
                        {
                            let profile_id = profile.id.clone();
                            let is_cloud = profile.location
                                == crate::components::settings::StorageLocation::Cloud;
                            rsx! {
                                button {
                                    class: "w-full flex items-center gap-3 p-3 bg-gray-700/50 hover:bg-gray-700 rounded-lg transition-colors text-left",
                                    onclick: move |_| on_transfer_to_profile.call(profile_id.clone()),
                                    div { class: "p-1.5 rounded bg-gray-600/50",
                                        if is_cloud {
                                            CloudIcon { class: "w-4 h-4 text-gray-300" }
                                        } else {
                                            HardDriveIcon { class: "w-4 h-4 text-gray-300" }
                                        }
                                    }
                                    div { class: "flex-1",
                                        div { class: "text-sm text-white", {profile.name.clone()} }
                                        div { class: "text-xs text-gray-400",
                                            if is_cloud {
                                                "Cloud storage"
                                            } else {
                                                "Local storage"
                                            }
                                            if profile.encrypted {
                                                " • Encrypted"
                                            }
                                        }
                                    }
                                    ArrowRightLeftIcon { class: "w-4 h-4 text-gray-400" }
                                }
                            }
                        }
                    }
                }
            }

            if can_eject {
                button {
                    class: "w-full flex items-center gap-3 p-3 bg-gray-700/50 hover:bg-gray-700 rounded-lg transition-colors text-left",
                    onclick: move |_| on_eject.call(()),
                    div { class: "p-1.5 rounded bg-gray-600/50",
                        DownloadIcon { class: "w-4 h-4 text-gray-300" }
                    }
                    div { class: "flex-1",
                        div { class: "text-sm text-white", "Eject to folder" }
                        div { class: "text-xs text-gray-400",
                            "Export files to a local folder and remove from managed storage"
                        }
                    }
                }
            }
        }
    }
}
