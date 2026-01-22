//! Import source selector view

use dioxus::prelude::*;

/// Import source type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportSource {
    #[default]
    Folder,
    Torrent,
    Cd,
}

impl ImportSource {
    pub fn label(&self) -> &'static str {
        match self {
            ImportSource::Folder => "Folder",
            ImportSource::Torrent => "Torrent",
            ImportSource::Cd => "CD",
        }
    }

    pub fn all() -> &'static [ImportSource] {
        &[
            ImportSource::Folder,
            #[cfg(feature = "torrent")]
            ImportSource::Torrent,
            #[cfg(feature = "cd-rip")]
            ImportSource::Cd,
        ]
    }
}

/// Import source selector tabs
#[component]
pub fn ImportSourceSelectorView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 rounded-lg bg-gray-800/40 p-1",
            for source in ImportSource::all() {
                button {
                    class: if selected_source == *source { "px-3 py-1 text-xs font-medium rounded-md bg-gray-700 text-gray-100" } else { "px-3 py-1 text-xs font-medium rounded-md text-gray-400 hover:text-gray-200" },
                    onclick: {
                        let source = *source;
                        move |_| on_source_select.call(source)
                    },
                    "{source.label()}"
                }
            }
            if !cfg!(feature = "torrent") {
                button {
                    class: "px-3 py-1 text-xs font-medium rounded-md text-gray-600 cursor-not-allowed",
                    disabled: true,
                    "Torrent"
                }
            }
            if !cfg!(feature = "cd-rip") {
                button {
                    class: "px-3 py-1 text-xs font-medium rounded-md text-gray-600 cursor-not-allowed",
                    disabled: true,
                    "CD"
                }
            }
        }
    }
}
