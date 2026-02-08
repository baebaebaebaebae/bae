//! Error display views for import workflow

use super::{DiscIdPill, DiscIdSource};
use crate::components::icons::AlertTriangleIcon;
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::floating_ui::Placement;
use dioxus::prelude::*;

/// Display DiscID lookup error with retry and optional skip buttons.
/// When `disc_id` and `on_skip` are provided, shows a centered view with disc ID pill.
/// Otherwise, shows a simple banner suitable for inline display.
#[component]
pub fn DiscIdLookupErrorView(
    error_message: Option<String>,
    on_retry: EventHandler<()>,
    #[props(default)] disc_id: Option<String>,
    #[props(default)] on_skip: Option<EventHandler<()>>,
) -> Element {
    let Some(ref error) = error_message else {
        return rsx! {};
    };

    // Full view with disc ID and skip button
    if let (Some(disc_id), Some(on_skip)) = (disc_id, on_skip) {
        rsx! {
            div { class: "text-center space-y-4 max-w-md",
                div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4",
                    div { class: "flex items-start gap-3",
                        AlertTriangleIcon { class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" }
                        div { class: "flex-1 text-left",
                            p { class: "text-sm text-gray-400 flex items-center gap-2 mb-2",
                                "Disc ID "
                                DiscIdPill {
                                    disc_id,
                                    source: DiscIdSource::Files,
                                    tooltip_placement: Placement::Top,
                                }
                            }
                            p { class: "text-sm text-amber-200", "{error}" }
                        }
                    }
                }
                div { class: "flex justify-center gap-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Small,
                        onclick: move |_| on_retry.call(()),
                        "Retry Lookup"
                    }
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        onclick: move |_| on_skip.call(()),
                        "Skip and search manually"
                    }
                }
            }
        }
    } else {
        // Simple banner for inline display
        rsx! {
            div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4 mb-4",
                div { class: "flex items-start gap-3",
                    AlertTriangleIcon { class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" }
                    div { class: "flex-1",
                        p { class: "text-sm text-amber-200", "{error}" }
                        div { class: "mt-3 flex gap-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                onclick: move |_| on_retry.call(()),
                                "Retry Lookup"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Display import error with retry button.
///
/// Shows an amber warning banner matching the DiscID error style.
/// Strips nested "Failed to start import:" prefixes for cleaner display.
#[component]
pub fn ImportErrorDisplayView(
    error_message: Option<String>,
    on_retry: EventHandler<()>,
) -> Element {
    let Some(ref error) = error_message else {
        return rsx! {};
    };

    // Strip nested "Failed to start import:" prefix for cleaner display
    let display_error = error
        .strip_prefix("Failed to start import: ")
        .unwrap_or(error);

    rsx! {
        div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4",
            div { class: "flex items-start gap-3",
                AlertTriangleIcon { class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" }
                div { class: "flex-1",
                    p { class: "text-sm font-medium text-amber-200 mb-1", "Import failed" }
                    p { class: "text-sm text-gray-400 select-text break-words", "{display_error}" }
                    div { class: "mt-3 flex gap-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_retry.call(()),
                            "Retry Import"
                        }
                    }
                }
            }
        }
    }
}
