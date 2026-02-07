//! Storage modal — shows file list and storage status for a release

use crate::components::icons::XIcon;
use crate::components::utils::format_file_size;
use crate::components::Modal;
use crate::display_types::File;
use dioxus::prelude::*;

#[component]
pub fn StorageModal(
    is_open: ReadSignal<bool>,
    on_close: EventHandler<()>,
    files: Vec<File>,
) -> Element {
    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                div { class: "flex items-center justify-between px-6 pt-6 pb-4 border-b border-gray-700",
                    h2 { class: "text-xl font-bold text-white", "Storage" }
                    button {
                        class: "text-gray-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        XIcon { class: "w-5 h-5" }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1 space-y-6",
                    // Storage status placeholder
                    div { class: "flex items-center justify-between p-4 bg-gray-700/50 rounded-lg",
                        div {
                            div { class: "text-sm font-medium text-white", "No storage profile" }
                            div { class: "text-xs text-gray-400 mt-1",
                                "Files are referenced from their original location"
                            }
                        }
                        button { class: "px-3 py-1.5 text-xs font-medium text-blue-400 hover:text-blue-300 border border-blue-500/50 hover:border-blue-400/50 rounded-lg transition-colors",
                            "Set Up Storage"
                        }
                    }

                    // Files section
                    div {
                        div { class: "text-sm font-medium text-gray-300 mb-3", "Files ({files.len()})" }
                        if files.is_empty() {
                            div { class: "text-gray-400 text-center py-8", "No files found" }
                        } else {
                            div { class: "space-y-2",
                                for file in files.iter() {
                                    div { class: "flex items-center justify-between py-2 px-3 bg-gray-700/50 rounded hover:bg-gray-700 transition-colors",
                                        div { class: "flex-1",
                                            div { class: "text-white text-sm font-medium",
                                                {file.filename.clone()}
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
            }
        }
    }
}
