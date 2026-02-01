//! Gallery lightbox component
//!
//! A full-screen image viewer with gallery strip, used for both
//! plain image viewing and cover art selection.

use crate::components::icons::{CheckIcon, ChevronLeftIcon, ChevronRightIcon, XIcon};
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton, Modal};
use dioxus::prelude::*;

/// An image in the gallery lightbox
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryImage {
    pub display_url: String,
    pub label: String,
}

/// Gallery lightbox with optional cover selection
///
/// When `selected_index` is None, this is a plain image viewer.
/// When `selected_index` is Some, shows "Select as Cover" button and green selection badges.
#[component]
pub fn GalleryLightbox(
    images: Vec<GalleryImage>,
    initial_index: usize,
    on_close: EventHandler<()>,
    on_navigate: EventHandler<usize>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    let is_open = use_memo(|| true);
    let mut current_index = use_signal(|| initial_index);
    let has_selection = selected_index.is_some();

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

    let is_current_selected = selected_index == Some(idx);

    let mut navigate = move |new_idx: usize| {
        current_index.set(new_idx);
        on_navigate.call(new_idx);
    };

    let on_keydown = move |evt: KeyboardEvent| match evt.key() {
        Key::ArrowLeft if can_prev => navigate(idx - 1),
        Key::ArrowRight if can_next => navigate(idx + 1),
        Key::Enter if has_selection => on_select.call(*current_index.read()),
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
                            navigate(idx - 1);
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
                            navigate(idx + 1);
                        },
                        ChevronRightIcon {
                            class: "w-8 h-8 text-gray-300 translate-x-0.5",
                            stroke_width: "1.5",
                        }
                    }
                }

                // Main image
                if has_selection {
                    // Selection mode: overlay with label and select button
                    div { class: "relative max-w-[90vw] max-h-[60vh]",
                        img {
                            src: "{url}",
                            alt: "{label}",
                            class: "max-w-[90vw] max-h-[60vh] object-contain rounded-lg shadow-2xl",
                        }
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
                                        onclick: move |_| on_select.call(idx),
                                        "Select as Cover"
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // View mode: image with filename below
                    div { class: "flex flex-col items-center",
                        img {
                            src: "{url}",
                            alt: "{label}",
                            class: "max-w-[90vw] max-h-[80vh] object-contain rounded-lg shadow-2xl",
                        }
                        div { class: "mt-4 text-gray-300 text-sm", {label.clone()} }
                    }
                }

                // Gallery strip
                if total > 1 {
                    div { class: "mt-6 flex gap-2 overflow-x-auto max-w-[90vw] p-1",
                        for (i , img) in images.iter().enumerate() {
                            {
                                let is_active = i == idx;
                                let is_selected = selected_index == Some(i);
                                let ring_class = if is_active {
                                    "ring-2 ring-white"
                                } else if is_selected {
                                    "ring-2 ring-green-500"
                                } else {
                                    "ring-1 ring-gray-600 hover:ring-gray-500"
                                };
                                rsx! {
                                    ChromelessButton {
                                        key: "{img.display_url}",
                                        class: Some(format!("relative flex-shrink-0 w-16 h-16 rounded-md {ring_class}")),
                                        onclick: move |_| navigate(i),
                                        div { class: "w-full h-full rounded-md overflow-clip",
                                            img {
                                                src: "{img.display_url}",
                                                alt: "{img.label}",
                                                class: "w-full h-full object-cover",
                                            }
                                        }
                                        if is_selected {
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
