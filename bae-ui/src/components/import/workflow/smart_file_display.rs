//! Smart file display view component

use super::gallery_lightbox::{GalleryItem, GalleryItemContent, GalleryLightbox};
use crate::components::icons::{DiscIcon, FileTextIcon, RowsIcon};
use crate::display_types::{AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, FileInfo};
use dioxus::prelude::*;

/// Base file row container - horizontal list item
#[component]
fn FileRow(
    /// Background class (e.g., "bg-white/5")
    bg: &'static str,
    /// Click handler - if present, renders as button with hover states
    #[props(default)]
    on_click: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let base = "flex items-center gap-2 px-3 py-2.5 rounded-lg";

    if let Some(handler) = on_click {
        rsx! {
            button {
                class: "{base} {bg} hover:bg-white/10 transition-all duration-150 cursor-pointer w-full text-left",
                onclick: move |_| handler.call(()),
                {children}
            }
        }
    } else {
        rsx! {
            div { class: "{base} {bg} w-full", {children} }
        }
    }
}

/// Section header for file groups
#[component]
fn FileSection(label: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "space-y-2",
            div { class: "text-[11px] font-medium text-gray-500 uppercase tracking-wide",
                "{label}"
            }
            {children}
        }
    }
}

/// Smart file display view - shows release materials grouped by type
///
/// Displays audio, artwork, documents, and other files with section headers.
/// Opens a unified gallery lightbox for viewing both images and text files.
///
/// Parent owns the `viewing_index` signal (which gallery item is open)
/// and derives text content loading from it.
#[component]
pub fn SmartFileDisplayView(
    files: CategorizedFileInfo,
    viewing_index: ReadSignal<Option<usize>>,
    text_file_content: Option<String>,
    on_view_change: EventHandler<Option<usize>>,
) -> Element {
    if files.is_empty() {
        return rsx! {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        };
    }

    // Check which sections have content
    let has_audio = !matches!(&files.audio, AudioContentInfo::TrackFiles(t) if t.is_empty());
    let has_artwork = !files.artwork.is_empty();
    let has_documents = !files.documents.is_empty();

    // Build combined gallery items: images first, then documents
    let artwork_count = files.artwork.len();
    let mut gallery_items: Vec<GalleryItem> = Vec::new();

    for file in files.artwork.iter() {
        gallery_items.push(GalleryItem {
            label: file.name.clone(),
            content: GalleryItemContent::Image {
                url: file.display_url.clone(),
                thumbnail_url: file.display_url.clone(),
            },
        });
    }
    for doc in files.documents.iter() {
        gallery_items.push(GalleryItem {
            label: doc.name.clone(),
            content: GalleryItemContent::Text { content: None },
        });
    }

    // Inject loaded text content into the viewed gallery item
    if let Some(idx) = viewing_index() {
        if let Some(item) = gallery_items.get_mut(idx) {
            if matches!(item.content, GalleryItemContent::Text { .. }) {
                item.content = GalleryItemContent::Text {
                    content: text_file_content.clone(),
                };
            }
        }
    }

    rsx! {
        div { class: "space-y-5",
            // Audio section - list rows
            if has_audio {
                FileSection { label: "Audio",
                    AudioListView {
                        audio: files.audio.clone(),
                        on_cue_click: {
                            let gallery_items = gallery_items.clone();
                            move |(name, _path): (String, String)| {
                                // Find this CUE file in the combined gallery
                                if let Some(idx) = gallery_items.iter().position(|item| item.label == name) {
                                    on_view_change.call(Some(idx));
                                }
                            }
                        },
                    }
                }
            }

            // Images section - covers, scans, booklets
            if has_artwork {
                FileSection { label: "Images",
                    div { class: "flex flex-wrap gap-2 content-start",
                        for (idx , file) in files.artwork.iter().enumerate() {
                            GalleryThumbnailView {
                                key: "{file.path}",
                                filename: file.name.clone(),
                                url: file.display_url.clone(),
                                index: idx,
                                on_click: move |idx| on_view_change.call(Some(idx)),
                            }
                        }
                    }
                }
            }

            // Documents section - list rows
            if has_documents {
                FileSection { label: "Text",
                    div { class: "flex flex-col gap-1",
                        for (doc_idx , doc) in files.documents.iter().enumerate() {
                            DocumentRowView {
                                key: "{doc.path}",
                                file: doc.clone(),
                                on_click: {
                                    let combined_idx = artwork_count + doc_idx;
                                    move |(_name, _path): (String, String)| {
                                        on_view_change.call(Some(combined_idx));
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }

        // Unified gallery lightbox - always rendered, visibility controlled by signal
        {
            let is_gallery_open = use_memo(move || viewing_index().is_some());
            let is_open: ReadSignal<bool> = is_gallery_open.into();
            rsx! {
                GalleryLightbox {
                    is_open,
                    items: gallery_items.clone(),
                    initial_index: viewing_index().unwrap_or(0),
                    on_close: move |_| on_view_change.call(None),
                    on_navigate: move |idx| on_view_change.call(Some(idx)),
                    selected_index: None::<usize>,
                    on_select: |_| {},
                }
            }
        }
    }
}

/// Audio list view (row format)
#[component]
fn AudioListView(audio: AudioContentInfo, on_cue_click: EventHandler<(String, String)>) -> Element {
    match audio {
        AudioContentInfo::CueFlacPairs(pairs) => {
            rsx! {
                div { class: "flex flex-col gap-1",
                    for pair in pairs.iter() {
                        CueFlacRowView {
                            key: "{pair.cue_path}",
                            pair: pair.clone(),
                            on_click: move |(name, path)| on_cue_click.call((name, path)),
                        }
                    }
                }
            }
        }
        AudioContentInfo::TrackFiles(tracks) if !tracks.is_empty() => {
            rsx! {
                FileRow { bg: "bg-blue-500/10",
                    RowsIcon { class: "w-4 h-4 text-blue-400 flex-shrink-0" }
                    span { class: "text-xs font-medium text-blue-300",
                        {format!("{} tracks", tracks.len())}
                    }
                    span { class: "text-xs text-gray-500", "FLAC" }
                }
            }
        }
        AudioContentInfo::TrackFiles(_) => rsx! {},
    }
}

/// CUE/FLAC pair row (list format)
#[component]
fn CueFlacRowView(pair: CueFlacPairInfo, on_click: EventHandler<(String, String)>) -> Element {
    let cue_name = pair.cue_name.clone();
    let track_count = pair.track_count;

    rsx! {
        FileRow {
            bg: "bg-purple-500/10",
            on_click: {
                let name = cue_name.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            DiscIcon { class: "w-4 h-4 text-purple-400 flex-shrink-0" }
            span { class: "text-xs font-medium text-purple-300", {format!("{} tracks", track_count)} }
            span { class: "text-xs text-gray-500", "CUE/FLAC" }
        }
    }
}

/// Gallery thumbnail - fixed 72x72px to match other tiles
#[component]
fn GalleryThumbnailView(
    filename: String,
    url: String,
    index: usize,
    on_click: EventHandler<usize>,
) -> Element {
    rsx! {
        button {
            class: "relative w-[72px] h-[72px] flex-shrink-0 rounded-xl overflow-clip hover:ring-2 hover:ring-white/20 transition-all duration-150 group",
            onclick: move |_| on_click.call(index),
            img {
                src: "{url}",
                alt: "{filename}",
                class: "w-full h-full object-cover",
            }
            div { class: "absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-1.5",
                span { class: "text-[10px] text-white truncate w-full", {filename.clone()} }
            }
        }
    }
}

/// Document row (list format, clickable to view)
#[component]
fn DocumentRowView(file: FileInfo, on_click: EventHandler<(String, String)>) -> Element {
    let filename = file.name.clone();

    rsx! {
        FileRow {
            bg: "bg-white/5",
            on_click: {
                let name = filename.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            FileTextIcon { class: "w-4 h-4 text-gray-400 flex-shrink-0" }
            span { class: "text-xs text-gray-200 truncate", {file.name.clone()} }
        }
    }
}
