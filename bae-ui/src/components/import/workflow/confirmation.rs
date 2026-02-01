//! Confirmation view component

use crate::components::icons::{
    CheckIcon, ChevronLeftIcon, ChevronRightIcon, ImageIcon, PencilIcon, XIcon,
};
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChromelessButton, Modal, Select, SelectOption,
    StorageProfile,
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
    let mut picker_images: Vec<CoverOption> = Vec::new();
    if let Some(ref url) = remote_cover_url {
        let source_label = match candidate.source_type {
            MatchSourceType::MusicBrainz => "MusicBrainz",
            MatchSourceType::Discogs => "Discogs",
        };
        picker_images.push(CoverOption {
            display_url: url.clone(),
            label: format!("{source_label} cover"),
            cover: SelectedCover::Remote {
                url: url.clone(),
                source: String::new(),
            },
        });
    }
    for img in managed_artwork.iter() {
        picker_images.push(CoverOption {
            display_url: img.display_url.clone(),
            label: img.name.clone(),
            cover: SelectedCover::Local {
                filename: img.name.clone(),
            },
        });
    }
    for img in artwork_files.iter() {
        picker_images.push(CoverOption {
            display_url: img.display_url.clone(),
            label: img.name.clone(),
            cover: SelectedCover::Local {
                filename: img.name.clone(),
            },
        });
    }

    // Find current cover's index for opening picker at the right image
    let current_cover_index = selected_cover.as_ref().and_then(|sc| {
        picker_images
            .iter()
            .position(|opt| sc.same_cover(&opt.cover))
    });

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
                    variant: ButtonVariant::Outline,
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

        // Cover picker lightbox
        if *show_cover_picker.read() {
            CoverPickerLightbox {
                key: "{picker_open_count}",
                images: picker_images.clone(),
                selected_cover: selected_cover.clone(),
                initial_index: current_cover_index.unwrap_or(0),
                on_select: move |cover: SelectedCover| {
                    on_select_cover.call(cover);
                    show_cover_picker.set(false);
                },
                on_close: move |_| show_cover_picker.set(false),
            }
        }
    }
}

/// A cover art option in the picker
#[derive(Clone, Debug, PartialEq)]
struct CoverOption {
    display_url: String,
    label: String,
    cover: SelectedCover,
}

/// Lightbox-based cover picker with gallery strip and select button
#[component]
fn CoverPickerLightbox(
    images: Vec<CoverOption>,
    selected_cover: Option<SelectedCover>,
    initial_index: usize,
    on_select: EventHandler<SelectedCover>,
    on_close: EventHandler<()>,
) -> Element {
    let is_open = use_memo(|| true);
    let mut current_index = use_signal(|| initial_index);

    let total = images.len();

    if total == 0 {
        return rsx! {
            Modal { is_open, on_close,
                div { class: "text-white", "No images available" }
            }
        };
    }

    let idx = (*current_index.read()).min(total - 1);
    let current_image = &images[idx];
    let url = &current_image.display_url;
    let label = &current_image.label;
    let can_prev = idx > 0;
    let can_next = idx < total - 1;

    let is_current_selected = selected_cover
        .as_ref()
        .is_some_and(|sc| sc.same_cover(&current_image.cover));

    let images_for_keydown = images.clone();
    let on_keydown = move |evt: KeyboardEvent| match evt.key() {
        Key::ArrowLeft if can_prev => current_index.set(idx - 1),
        Key::ArrowRight if can_next => current_index.set(idx + 1),
        Key::Enter => {
            let cover = images_for_keydown[*current_index.read()].cover.clone();
            on_select.call(cover);
        }
        _ => {}
    };

    rsx! {
        Modal { is_open, on_close,
            div {
                tabindex: 0,
                autofocus: true,
                onkeydown: on_keydown,
                class: "flex flex-col items-center",

                // Close button
                button {
                    class: "fixed top-4 right-4 text-gray-400 hover:text-white transition-colors z-10",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_close.call(());
                    },
                    XIcon { class: "w-6 h-6" }
                }

                // Previous button
                if can_prev {
                    button {
                        class: "fixed left-4 top-1/2 -translate-y-1/2 w-14 h-14 bg-gray-800/60 hover:bg-gray-700/80 rounded-full flex items-center justify-center transition-colors z-10",
                        onclick: move |e| {
                            e.stop_propagation();
                            current_index.set(idx - 1);
                        },
                        ChevronLeftIcon {
                            class: "w-8 h-8 text-gray-300 -translate-x-0.5",
                            stroke_width: "1.5",
                        }
                    }
                }

                // Next button
                if can_next {
                    button {
                        class: "fixed right-4 top-1/2 -translate-y-1/2 w-14 h-14 bg-gray-800/60 hover:bg-gray-700/80 rounded-full flex items-center justify-center transition-colors z-10",
                        onclick: move |e| {
                            e.stop_propagation();
                            current_index.set(idx + 1);
                        },
                        ChevronRightIcon {
                            class: "w-8 h-8 text-gray-300 translate-x-0.5",
                            stroke_width: "1.5",
                        }
                    }
                }

                // Main image with overlay
                div { class: "relative max-w-[90vw] max-h-[60vh]",
                    img {
                        src: "{url}",
                        alt: "{label}",
                        class: "max-w-[90vw] max-h-[60vh] object-contain rounded-lg shadow-2xl",
                    }

                    // Bottom overlay with label and select button
                    div { class: "absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/40 to-transparent rounded-b-lg px-4 py-3 flex items-center gap-3",
                        span { class: "text-white text-sm", {label.clone()} }
                        div { class: "ml-auto h-8 flex items-center",
                            if is_current_selected {
                                span { class: "text-green-400 text-sm flex items-center gap-1 px-3",
                                    CheckIcon { class: "w-4 h-4" }
                                    "Selected"
                                }
                            } else {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Small,
                                    onclick: {
                                        let cover = current_image.cover.clone();
                                        move |_| on_select.call(cover.clone())
                                    },
                                    "Select as Cover"
                                }
                            }
                        }
                    }
                }

                // Gallery strip
                if total > 1 {
                    div { class: "mt-6 flex gap-2 overflow-x-auto max-w-[90vw] p-1",
                        for (i , opt) in images.iter().enumerate() {
                            {
                                let is_active = i == idx;
                                let is_selected_cover = selected_cover
                                    .as_ref()
                                    .is_some_and(|sc| sc.same_cover(&opt.cover));
                                let ring_class = if is_active {
                                    "ring-2 ring-white"
                                } else if is_selected_cover {
                                    "ring-2 ring-green-500"
                                } else {
                                    "ring-1 ring-gray-600 hover:ring-gray-500"
                                };
                                rsx! {
                                    ChromelessButton {
                                        key: "{opt.display_url}",
                                        class: Some(format!("relative flex-shrink-0 w-16 h-16 rounded-md {ring_class}")),
                                        onclick: move |_| current_index.set(i),
                                        div { class: "w-full h-full rounded-md overflow-clip",
                                            img {
                                                src: "{opt.display_url}",
                                                alt: "{opt.label}",
                                                class: "w-full h-full object-cover",
                                            }
                                        }
                                        if is_selected_cover {
                                            div { class: "absolute top-0.5 right-0.5 bg-green-500 text-white w-3.5 h-3.5 rounded-full flex items-center justify-center",
                                                CheckIcon { class: "w-2.5 h-2.5" }
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
