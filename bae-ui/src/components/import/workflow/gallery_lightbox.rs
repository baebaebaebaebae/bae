//! Gallery lightbox component
//!
//! A full-screen viewer with gallery strip for images and text files,
//! used for both plain viewing and cover art selection.

use crate::components::icons::{CheckIcon, ChevronLeftIcon, ChevronRightIcon, FileTextIcon, XIcon};
use crate::components::{Button, ButtonSize, ButtonVariant, ChromelessButton, Modal};
use dioxus::prelude::*;

/// Content variant for a gallery item
#[derive(Clone, Debug, PartialEq)]
pub enum GalleryItemContent {
    Image { url: String, thumbnail_url: String },
    Text { content: Option<String> },
}

/// An item in the gallery lightbox (image or text file)
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryItem {
    pub label: String,
    pub content: GalleryItemContent,
}

/// Gallery lightbox with optional cover selection
///
/// Always rendered by parent. Visibility controlled via `is_open` signal.
/// When `selected_index` is None, this is a plain viewer.
/// When `selected_index` is Some, shows "Select as Cover" button and green selection badges
/// (only for image items).
#[component]
pub fn GalleryLightbox(
    is_open: ReadSignal<bool>,
    items: Vec<GalleryItem>,
    initial_index: usize,
    on_close: EventHandler<()>,
    on_navigate: EventHandler<usize>,
    selected_index: Option<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    let mut current_index = use_signal(|| initial_index);
    let mut last_initial = use_signal(|| initial_index);

    // Sync with initial_index when parent reopens with a different file
    if initial_index != *last_initial.read() {
        last_initial.set(initial_index);
        current_index.set(initial_index);
    }

    let has_selection = selected_index.is_some();

    let total = items.len();

    if total == 0 {
        return rsx! {
            Modal { is_open, on_close,
                div { class: "text-white", "No files available" }
            }
        };
    }

    let idx = (*current_index.read()).min(total - 1);
    let current_item = &items[idx];
    let label = &current_item.label;
    let can_prev = idx > 0;
    let can_next = idx < total - 1;

    let is_current_image = matches!(current_item.content, GalleryItemContent::Image { .. });
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
            div { onkeydown: on_keydown, class: "flex flex-col items-center",

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

                // Main content area
                if has_selection && is_current_image {
                    // Selection mode (images only): overlay with label and select button
                    {
                        let url = match &current_item.content {
                            GalleryItemContent::Image { url, .. } => url,
                            _ => unreachable!(),
                        };
                        rsx! {
                            div { class: "relative max-w-[90vw] max-h-[60vh]",
                                img {
                                    src: "{url}",
                                    alt: "{label}",
                                    class: "max-w-[90vw] max-h-[60vh] object-contain rounded-lg shadow-2xl",
                                }
                                div { class: "absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/40 to-transparent rounded-b-lg px-4 py-3 flex items-end gap-3",
                                    span { class: "text-white text-xs", {label.clone()} }
                                    div { class: "ml-auto h-8 flex items-center whitespace-nowrap",
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
                        }
                    }
                } else {
                    // View mode: render based on content type
                    match &current_item.content {
                        GalleryItemContent::Image { url, .. } => rsx! {
                            div { class: "flex flex-col items-center",
                                img {
                                    src: "{url}",
                                    alt: "{label}",
                                    class: "max-w-[90vw] max-h-[80vh] object-contain rounded-lg shadow-2xl",
                                }
                                div { class: "mt-4 text-gray-300 text-sm", {label.clone()} }
                            }
                        },
                        GalleryItemContent::Text { content: Some(text) } => rsx! {
                            div { class: "flex flex-col items-center",
                                div { class: "bg-gray-800 rounded-lg w-[min(42rem,90vw)] max-h-[80vh] overflow-auto shadow-2xl",
                                    pre { class: "text-sm text-gray-300 font-mono whitespace-pre-wrap select-text p-4",
                                        {text.clone()}
                                    }
                                }
                                div { class: "mt-4 text-gray-300 text-sm", {label.clone()} }
                            }
                        },
                        GalleryItemContent::Text { content: None } => rsx! {
                            div { class: "flex flex-col items-center",
                                div { class: "bg-gray-800 rounded-lg w-[min(42rem,90vw)] p-8 flex items-center justify-center shadow-2xl",
                                    span { class: "text-gray-400 text-sm", "Loading..." }
                                }
                                div { class: "mt-4 text-gray-300 text-sm", {label.clone()} }
                            }
                        },
                    }
                }

                // Gallery strip
                if total > 1 {
                    div { class: "mt-6 flex gap-2 overflow-x-auto max-w-[90vw] p-1",
                        for (i , item) in items.iter().enumerate() {
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
                                        key: "{item.label}-{i}",
                                        class: Some(format!("relative flex-shrink-0 w-16 h-16 rounded-md {ring_class}")),
                                        onclick: move |_| navigate(i),
                                        div { class: "w-full h-full rounded-md overflow-clip",
                                            match &item.content {
                                                GalleryItemContent::Image { thumbnail_url, .. } => rsx! {
                                                    img {
                                                        src: "{thumbnail_url}",
                                                        alt: "{item.label}",
                                                        class: "w-full h-full object-cover",
                                                    }
                                                },
                                                GalleryItemContent::Text { .. } => rsx! {
                                                    div { class: "w-full h-full bg-gray-800 flex items-center justify-center",
                                                        FileTextIcon { class: "w-6 h-6 text-gray-400" }
                                                    }
                                                },
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
