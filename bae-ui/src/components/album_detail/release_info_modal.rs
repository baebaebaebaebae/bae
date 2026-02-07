//! Release info modal — shows release metadata (year, format, label, links, etc.)

use crate::components::icons::XIcon;
use crate::components::utils::format_duration;
use crate::components::Modal;
use crate::display_types::Release;
use dioxus::prelude::*;

#[component]
pub fn ReleaseInfoModal(
    is_open: ReadSignal<bool>,
    release: Release,
    on_close: EventHandler<()>,
    #[props(default)] track_count: usize,
    #[props(default)] total_duration_ms: Option<i64>,
) -> Element {
    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                div { class: "flex items-center justify-between px-6 pt-6 pb-4 border-b border-gray-700",
                    h2 { class: "text-xl font-bold text-white", "Info" }
                    button {
                        class: "text-gray-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1",
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
        }
    }
}
