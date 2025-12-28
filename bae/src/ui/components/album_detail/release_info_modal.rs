use crate::db::{DbAlbum, DbFile, DbImage, DbRelease};
use crate::library::use_library_manager;
use dioxus::prelude::*;
use tracing::error;
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Details,
    Files,
    Gallery,
}
/// Modal component with tabs for release details and files
#[component]
pub fn ReleaseInfoModal(album: DbAlbum, release_id: String, on_close: EventHandler<()>) -> Element {
    let mut active_tab = use_signal(|| Tab::Details);
    let library_manager = use_library_manager();
    let release = use_signal(|| None::<DbRelease>);
    let files = use_signal(Vec::<DbFile>::new);
    let images = use_signal(Vec::<DbImage>::new);
    let is_loading_files = use_signal(|| false);
    let is_loading_images = use_signal(|| false);
    let error_message = use_signal(|| None::<String>);
    let images_error = use_signal(|| None::<String>);
    use_effect({
        let release_id_clone = release_id.clone();
        let library_manager_clone = library_manager.clone();
        let mut release_signal = release;
        let album_id = album.id.clone();
        move || {
            let release_id = release_id_clone.clone();
            let library_manager = library_manager_clone.clone();
            let album_id = album_id.clone();
            spawn(async move {
                match library_manager
                    .get()
                    .get_releases_for_album(&album_id)
                    .await
                {
                    Ok(releases) => {
                        if let Some(rel) = releases.into_iter().find(|r| r.id == release_id) {
                            release_signal.set(Some(rel));
                        }
                    }
                    Err(e) => {
                        error!("Failed to load release: {}", e);
                    }
                }
            });
        }
    });
    use_effect({
        let release_id_clone = release_id.clone();
        let library_manager_clone = library_manager.clone();
        let mut files_signal = files;
        let mut is_loading_signal = is_loading_files;
        let mut error_message_signal = error_message;
        let tab = *active_tab.read();
        move || {
            if tab == Tab::Files {
                let release_id = release_id_clone.clone();
                let library_manager = library_manager_clone.clone();
                spawn(async move {
                    is_loading_signal.set(true);
                    error_message_signal.set(None);
                    match library_manager
                        .get()
                        .get_files_for_release(&release_id)
                        .await
                    {
                        Ok(mut release_files) => {
                            release_files
                                .sort_by(|a, b| a.original_filename.cmp(&b.original_filename));
                            files_signal.set(release_files);
                            is_loading_signal.set(false);
                        }
                        Err(e) => {
                            error!("Failed to load files: {}", e);
                            error_message_signal.set(Some(format!("Failed to load files: {}", e)));
                            is_loading_signal.set(false);
                        }
                    }
                });
            }
        }
    });
    use_effect({
        let release_id_clone = release_id.clone();
        let library_manager_clone = library_manager.clone();
        let mut images_signal = images;
        let mut is_loading_signal = is_loading_images;
        let mut error_signal = images_error;
        let tab = *active_tab.read();
        move || {
            if tab == Tab::Gallery {
                let release_id = release_id_clone.clone();
                let library_manager = library_manager_clone.clone();
                spawn(async move {
                    is_loading_signal.set(true);
                    error_signal.set(None);
                    match library_manager
                        .get()
                        .get_images_for_release(&release_id)
                        .await
                    {
                        Ok(release_images) => {
                            images_signal.set(release_images);
                            is_loading_signal.set(false);
                        }
                        Err(e) => {
                            error!("Failed to load images: {}", e);
                            error_signal.set(Some(format!("Failed to load images: {}", e)));
                            is_loading_signal.set(false);
                        }
                    }
                });
            }
        }
    });
    let current_tab = *active_tab.read();
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div { class: "border-b border-gray-700",
                    div { class: "flex items-center justify-between px-6 pt-6 pb-4",
                        h2 { class: "text-xl font-bold text-white", "Release Info" }
                        button {
                            class: "text-gray-400 hover:text-white transition-colors",
                            onclick: move |_| on_close.call(()),
                            "✕"
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
                        button {
                            class: if current_tab == Tab::Gallery { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Gallery),
                            "Gallery"
                        }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1",
                    match current_tab {
                        Tab::Details => rsx! {
                            DetailsTab { album: album.clone(), release: release().clone() }
                        },
                        Tab::Files => rsx! {
                            FilesTab { files, is_loading: is_loading_files, error_message }
                        },
                        Tab::Gallery => rsx! {
                            GalleryTab { images, is_loading: is_loading_images, error_message: images_error }
                        },
                    }
                }
            }
        }
    }
}
#[component]
fn DetailsTab(album: DbAlbum, release: Option<DbRelease>) -> Element {
    if let Some(release) = release {
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
                div { class: "pt-4 border-t border-gray-700 space-y-2",
                    if let Some(ref mb_release) = album.musicbrainz_release {
                        a {
                            href: "https://musicbrainz.org/release/{mb_release.release_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "🔗" }
                            span { "View on MusicBrainz" }
                        }
                    }
                    if let Some(ref discogs) = album.discogs_release {
                        a {
                            href: "https://www.discogs.com/release/{discogs.release_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "🔗" }
                            span { "View on Discogs" }
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "text-gray-400 text-center py-8", "Loading release details..." }
        }
    }
}
#[component]
fn FilesTab(
    files: ReadSignal<Vec<DbFile>>,
    is_loading: ReadSignal<bool>,
    error_message: ReadSignal<Option<String>>,
) -> Element {
    let files = files();
    let is_loading = is_loading();
    let error_message = error_message();
    rsx! {
        if is_loading {
            div { class: "text-gray-400 text-center py-8", "Loading files..." }
        } else if let Some(ref error) = error_message {
            div { class: "text-red-400 text-center py-8", {error.clone()} }
        } else if files.is_empty() {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        } else {
            div { class: "space-y-2",
                for file in files.iter() {
                    div { class: "flex items-center justify-between py-2 px-3 bg-gray-700/50 rounded hover:bg-gray-700 transition-colors",
                        div { class: "flex-1",
                            div { class: "text-white text-sm font-medium",
                                {file.original_filename.clone()}
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
fn format_file_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
#[component]
fn GalleryTab(
    images: ReadSignal<Vec<DbImage>>,
    is_loading: ReadSignal<bool>,
    error_message: ReadSignal<Option<String>>,
) -> Element {
    let images = images();
    let is_loading = is_loading();
    let error_message = error_message();
    rsx! {
        if is_loading {
            div { class: "text-gray-400 text-center py-8", "Loading images..." }
        } else if let Some(ref error) = error_message {
            div { class: "text-red-400 text-center py-8", {error.clone()} }
        } else if images.is_empty() {
            div { class: "text-gray-400 text-center py-8", "No images found" }
        } else {
            div { class: "grid grid-cols-2 sm:grid-cols-3 gap-4",
                for image in images.iter() {
                    {render_gallery_image(image)}
                }
            }
        }
    }
}
fn render_gallery_image(image: &DbImage) -> Element {
    let is_cover = image.is_cover;
    let filename = image.filename.clone();
    let source_label = match image.source {
        crate::db::ImageSource::Local => "Local",
        crate::db::ImageSource::MusicBrainz => "MusicBrainz",
        crate::db::ImageSource::Discogs => "Discogs",
    };
    rsx! {
        div { class: "relative group",
            div { class: if is_cover { "aspect-square bg-gray-700 rounded-lg overflow-hidden ring-2 ring-blue-500" } else { "aspect-square bg-gray-700 rounded-lg overflow-hidden" },
                div { class: "w-full h-full flex items-center justify-center text-gray-500",
                    "🖼️"
                }
            }
            div { class: "absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-2",
                div { class: "text-xs text-white truncate", {filename} }
                div { class: "flex items-center gap-2 mt-1",
                    if is_cover {
                        span { class: "text-xs px-1.5 py-0.5 bg-blue-500 text-white rounded",
                            "Cover"
                        }
                    }
                    span { class: "text-xs text-gray-400", {source_label} }
                }
            }
        }
    }
}
