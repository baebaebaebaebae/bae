//! Torrent input view component

use crate::components::icons::UploadIcon;
use crate::components::{Button, ButtonSize, ButtonVariant, TextInput, TextInputSize};
use dioxus::prelude::*;

/// Torrent input mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TorrentInputMode {
    #[default]
    File,
    Magnet,
}

/// Torrent input view - file/magnet selector with drop zone
#[component]
pub fn TorrentInputView(
    /// Current input mode
    input_mode: TorrentInputMode,
    /// Whether drag is currently active
    #[props(default = false)]
    is_dragging: bool,
    /// Called when mode tab is clicked
    on_mode_change: EventHandler<TorrentInputMode>,
    /// Called when select button is clicked
    on_select_click: EventHandler<()>,
    /// Called when magnet link is submitted
    on_magnet_submit: EventHandler<String>,
) -> Element {
    let mut magnet_input = use_signal(String::new);

    let drag_classes = if is_dragging {
        "border-blue-500 bg-blue-900/20 border-solid"
    } else {
        "border-gray-600 border-dashed"
    };

    rsx! {
        div { class: "space-y-4",
            // Mode tabs
            div { class: "flex space-x-4 mb-4",
                Button {
                    variant: if input_mode == TorrentInputMode::File { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Medium,
                    onclick: move |_| on_mode_change.call(TorrentInputMode::File),
                    "File"
                }
                Button {
                    variant: if input_mode == TorrentInputMode::Magnet { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    size: ButtonSize::Medium,
                    onclick: move |_| on_mode_change.call(TorrentInputMode::Magnet),
                    "Magnet Link"
                }
            }

            match input_mode {
                TorrentInputMode::File => rsx! {
                    div { class: "border-2 rounded-lg p-12 {drag_classes}",
                        div { class: "flex flex-col items-center justify-center space-y-6",
                            div { class: "w-16 h-16 text-gray-400",
                                UploadIcon { class: "w-full h-full" }
                            }
                            div { class: "text-center space-y-2",
                                h3 { class: "text-lg font-semibold text-gray-200", "Select a torrent file" }
                                p { class: "text-sm text-gray-400", "Drop a .torrent file here or click to browse" }
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Medium,
                                onclick: move |_| on_select_click.call(()),
                                "Select Torrent"
                            }
                        }
                    }
                },
                TorrentInputMode::Magnet => rsx! {
                    div { class: "space-y-4",
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Magnet Link" }
                            TextInput {
                                value: magnet_input(),
                                on_input: move |v| magnet_input.set(v),
                                size: TextInputSize::Medium,
                                placeholder: "magnet:?xt=urn:btih:...",
                                monospace: true,
                            }
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Medium,
                            disabled: magnet_input.read().is_empty(),
                            onclick: move |_| {
                                let value = magnet_input.read().clone();
                                if !value.is_empty() {
                                    on_magnet_submit.call(value);
                                }
                            },
                            "Add Magnet Link"
                        }
                    }
                },
            }
        }
    }
}
