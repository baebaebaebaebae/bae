//! Confirmation view component

use super::gallery_lightbox::{GalleryImage, GalleryLightbox};
use crate::components::icons::{ImageIcon, PencilIcon};
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChromelessButton, Select, SelectOption, StorageProfile,
};
use crate::display_types::{FileInfo, MatchCandidate, MatchSourceType, SelectedCover};
use dioxus::prelude::*;

/// Final confirmation view before import
#[component]
pub fn ConfirmationView(
    /// The confirmed match candidate
    candidate: MatchCandidate,
    /// Currently selected cover
    selected_cover: Option<SelectedCover>,
    /// URL to display for the cover (resolved from selected_cover)
    display_cover_url: Option<String>,
    /// Artwork files available in the folder (with resolved display URLs)
    artwork_files: Vec<FileInfo>,
    /// Managed artwork files from .bae/ (downloaded covers)
    managed_artwork: Vec<FileInfo>,
    /// Remote cover URL from the match candidate
    remote_cover_url: Option<String>,
    /// Available storage profiles
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    /// Currently selected storage profile ID
    selected_profile_id: Option<String>,
    /// Whether import is in progress
    is_importing: bool,
    /// Current preparing step text (if preparing)
    preparing_step_text: Option<String>,
    /// Called when user selects a cover
    on_select_cover: EventHandler<SelectedCover>,
    /// Called when user changes storage profile
    on_storage_profile_change: EventHandler<Option<String>>,
    /// Called when user clicks Edit to go back
    on_edit: EventHandler<()>,
    /// Called when user confirms import
    on_confirm: EventHandler<()>,
    /// Called to navigate to settings
    on_configure_storage: EventHandler<()>,
) -> Element {
    let mut show_cover_picker = use_signal(|| false);
    let mut picker_open_count = use_signal(|| 0u32);

    let release_year = candidate.year.clone();
    let original_year = candidate.original_year.clone();

    let (format_text, country_text, label_text) = match candidate.source_type {
        MatchSourceType::MusicBrainz => (
            candidate.format.clone(),
            candidate.country.clone(),
            candidate.label.clone(),
        ),
        MatchSourceType::Discogs => (None, None, None),
    };

    let has_cover_options =
        !artwork_files.is_empty() || !managed_artwork.is_empty() || remote_cover_url.is_some();

    // Build combined image list for picker: remote + managed + release artwork
    // Each entry pairs a GalleryImage with the SelectedCover it represents
    let mut picker_covers: Vec<SelectedCover> = Vec::new();
    let mut picker_images: Vec<GalleryImage> = Vec::new();

    if let Some(ref url) = remote_cover_url {
        let source_label = match candidate.source_type {
            MatchSourceType::MusicBrainz => "MusicBrainz",
            MatchSourceType::Discogs => "Discogs",
        };
        picker_images.push(GalleryImage {
            display_url: url.clone(),
            label: format!("{source_label} cover"),
        });
        picker_covers.push(SelectedCover::Remote {
            url: url.clone(),
            source: String::new(),
        });
    }
    for img in managed_artwork.iter() {
        picker_images.push(GalleryImage {
            display_url: img.display_url.clone(),
            label: img.name.clone(),
        });
        picker_covers.push(SelectedCover::Local {
            filename: img.name.clone(),
        });
    }
    for img in artwork_files.iter() {
        picker_images.push(GalleryImage {
            display_url: img.display_url.clone(),
            label: img.name.clone(),
        });
        picker_covers.push(SelectedCover::Local {
            filename: img.name.clone(),
        });
    }

    // Find current cover's index for opening picker at the right image
    let current_cover_index = selected_cover
        .as_ref()
        .and_then(|sc| picker_covers.iter().position(|c| sc.same_cover(c)));

    rsx! {
        div { class: "p-5 space-y-5",
            // Release info card
            div { class: "bg-gray-800/50 rounded-lg px-5 py-4",
                div { class: "flex items-center gap-5",
                    // Cover art thumbnail (clickable if options available)
                    if has_cover_options {
                        ChromelessButton {
                            class: Some("flex-shrink-0 relative group".to_string()),
                            onclick: move |_| {
                                picker_open_count += 1;
                                show_cover_picker.set(true);
                            },
                            if let Some(ref url) = display_cover_url {
                                div { class: "w-20 h-20 rounded-lg overflow-clip ring-0 group-hover:ring-2 group-hover:ring-gray-500 transition-all",
                                    img {
                                        src: "{url}",
                                        alt: "Album cover",
                                        class: "w-full h-full object-cover",
                                    }
                                }
                            } else {
                                div { class: "w-20 h-20 rounded-lg bg-gray-700 flex items-center justify-center ring-0 group-hover:ring-2 group-hover:ring-gray-500 transition-all",
                                    ImageIcon { class: "w-8 h-8 text-gray-500" }
                                }
                            }
                            div { class: "absolute top-0.5 right-0.5 bg-black/40 group-hover:bg-black/60 backdrop-blur-sm rounded-md p-1 transition-colors",
                                PencilIcon { class: "w-3.5 h-3.5 text-gray-300 group-hover:text-white" }
                            }
                        }
                    } else {
                        if let Some(ref url) = display_cover_url {
                            div { class: "flex-shrink-0 w-20 h-20 rounded-lg overflow-clip",
                                img {
                                    src: "{url}",
                                    alt: "Album cover",
                                    class: "w-full h-full object-cover",
                                }
                            }
                        } else {
                            div { class: "flex-shrink-0 w-20 h-20 rounded-lg bg-gray-700 flex items-center justify-center",
                                ImageIcon { class: "w-8 h-8 text-gray-500" }
                            }
                        }
                    }

                    // Metadata
                    div { class: "flex-1 min-w-0",
                        h4 { class: "text-base font-medium text-white truncate", "{candidate.title}" }
                        div { class: "text-xs text-gray-400 flex flex-wrap gap-x-3",
                            if let Some(ref year) = release_year {
                                span { "{year}" }
                            }
                            if let Some(ref fmt) = format_text {
                                span { "{fmt}" }
                            }
                            if let Some(ref country) = country_text {
                                span { "{country}" }
                            }
                        }
                        if label_text.is_some() || original_year.is_some() {
                            div { class: "text-xs text-gray-500 flex flex-wrap gap-x-3",
                                if let Some(ref label) = label_text {
                                    span { "{label}" }
                                }
                                if original_year.is_some() && original_year != release_year {
                                    span { "Original: {original_year.as_ref().unwrap()}" }
                                }
                            }
                        }
                    }

                    // Edit button
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        disabled: is_importing,
                        onclick: move |_| on_edit.call(()),
                        "Edit"
                    }
                }
            }

            // Storage profile selection + Import button
            div { class: "flex items-center gap-3 px-5",
                label { class: "text-sm text-gray-400 ml-auto", "Storage:" }
                Select {
                    value: selected_profile_id.clone().unwrap_or_else(|| "__none__".to_string()),
                    disabled: is_importing,
                    onchange: move |val: String| {
                        if val == "__none__" {
                            on_storage_profile_change.call(None);
                        } else {
                            on_storage_profile_change.call(Some(val));
                        }
                    },
                    SelectOption {
                        value: "__none__",
                        label: "No Storage (files stay in place)",
                    }
                    for profile in storage_profiles.read().iter() {
                        SelectOption {
                            key: "{profile.id}",
                            value: "{profile.id}",
                            label: profile.name.clone(),
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Small,
                    onclick: move |_| on_configure_storage.call(()),
                    "Configure"
                }

                // Import status and button
                if is_importing {
                    if let Some(ref step) = preparing_step_text {
                        span { class: "text-sm text-gray-400", "{step}" }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Small,
                    disabled: is_importing,
                    loading: is_importing,
                    onclick: move |_| on_confirm.call(()),
                    if is_importing {
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                    "Import"
                }
            }
        }

        // Cover picker lightbox - always rendered, visibility controlled by signal
        {
            let is_open: ReadSignal<bool> = show_cover_picker.into();
            rsx! {
                GalleryLightbox {
                    is_open,
                    key: "{picker_open_count}",
                    images: picker_images.clone(),
                    initial_index: current_cover_index.unwrap_or(0),
                    on_close: move |_| show_cover_picker.set(false),
                    on_navigate: |_| {},
                    selected_index: current_cover_index,
                    on_select: {
                        let picker_covers = picker_covers.clone();
                        move |idx: usize| {
                            if let Some(cover) = picker_covers.get(idx) {
                                on_select_cover.call(cover.clone());
                            }
                            show_cover_picker.set(false);
                        }
                    },
                }
            }
        }
    }
}
