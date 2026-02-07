//! Release info modal with tabs for details and files

use crate::components::icons::XIcon;
use crate::components::utils::{format_duration, format_file_size};
use crate::components::Modal;
use crate::display_types::{File, Release};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Details,
    Files,
}

/// Modal component with tabs for release details and files (props-based)
#[component]
pub fn ReleaseInfoModal(
    is_open: ReadSignal<bool>,
    release: Release,
    on_close: EventHandler<()>,
    #[props(default)] files: Vec<File>,
    #[props(default = Tab::Details)] initial_tab: Tab,
    #[props(default)] track_count: usize,
    #[props(default)] total_duration_ms: Option<i64>,
) -> Element {
    let mut active_tab = use_signal(|| initial_tab);

    // Reset to initial_tab when modal opens
    use_effect(move || {
        if is_open() {
            active_tab.set(initial_tab);
        }
    });

    let current_tab = *active_tab.read();

    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                div { class: "border-b border-gray-700",
                    div { class: "flex items-center justify-between px-6 pt-6 pb-4",
                        h2 { class: "text-xl font-bold text-white", "Release Info" }
                        button {
                            class: "text-gray-400 hover:text-white transition-colors",
                            onclick: move |_| on_close.call(()),
                            XIcon { class: "w-5 h-5" }
                        }
                    }
                    div { class: "flex px-6",
                        button {
                            class: if current_tab == Tab::Details { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Details),
                            "Details"
                        }
                        button {
                            class: if current_tab == Tab::Files { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Files),
                            "Files"
                        }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1",
                    match current_tab {
                        Tab::Details => rsx! {
                            DetailsTab { release: release.clone(), track_count, total_duration_ms }
                        },
                        Tab::Files => rsx! {
                            FilesTab { files: files.clone() }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn DetailsTab(release: Release, track_count: usize, total_duration_ms: Option<i64>) -> Element {
    rsx! {
        div { class: "space-y-4",
            if release.year.is_some() || release.format.is_some() {
                div {
                    if let Some(year) = release.year {
                        span { class: "text-gray-300", "{year}" }
                        if release.format.is_some() {
                            span { class: "text-gray-300", " " }
                        }
                    }
                    if let Some(ref format) = release.format {
                        span { class: "text-gray-300", "{format}" }
                    }
                }
            }
            // Track count and duration
            if track_count > 0 || total_duration_ms.is_some() {
                div { class: "text-gray-300",
                    if track_count > 0 {
                        span {
                            "{track_count} "
                            if track_count == 1 {
                                "track"
                            } else {
                                "tracks"
                            }
                        }
                    }
                    if track_count > 0 && total_duration_ms.is_some() {
                        span { " · " }
                    }
                    if let Some(duration) = total_duration_ms {
                        span { {format_duration(duration)} }
                    }
                }
            }
            if release.label.is_some() || release.catalog_number.is_some() {
                div { class: "text-sm text-gray-400",
                    if let Some(ref label) = release.label {
                        span { "{label}" }
                        if release.catalog_number.is_some() {
                            span { " • " }
                        }
                    }
                    if let Some(ref catalog) = release.catalog_number {
                        span { "{catalog}" }
                    }
                }
            }
            if let Some(ref country) = release.country {
                div { class: "text-sm text-gray-400",
                    span { "{country}" }
                }
            }
            if let Some(ref barcode) = release.barcode {
                div { class: "text-sm text-gray-400",
                    span { class: "font-medium", "Barcode: " }
                    span { class: "font-mono", "{barcode}" }
                }
            }
            // External links
            if release.musicbrainz_release_id.is_some() || release.discogs_release_id.is_some() {
                div { class: "pt-4 border-t border-gray-700 space-y-2",
                    if let Some(ref mb_id) = release.musicbrainz_release_id {
                        a {
                            href: "https://musicbrainz.org/release/{mb_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "View on MusicBrainz" }
                        }
                    }
                    if let Some(ref discogs_id) = release.discogs_release_id {
                        a {
                            href: "https://www.discogs.com/release/{discogs_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "View on Discogs" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FilesTab(files: Vec<File>) -> Element {
    rsx! {
        if files.is_empty() {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        } else {
            div { class: "space-y-2",
                for file in files.iter() {
                    div { class: "flex items-center justify-between py-2 px-3 bg-gray-700/50 rounded hover:bg-gray-700 transition-colors",
                        div { class: "flex-1",
                            div { class: "text-white text-sm font-medium", {file.filename.clone()} }
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
