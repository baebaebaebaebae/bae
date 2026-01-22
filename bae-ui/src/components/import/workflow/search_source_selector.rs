//! Search source selector view component

use crate::display_types::SearchSource;
use dioxus::prelude::*;

/// Radio buttons to select between MusicBrainz and Discogs
#[component]
pub fn SearchSourceSelectorView(
    selected_source: SearchSource,
    on_select: EventHandler<SearchSource>,
) -> Element {
    rsx! {
        div { class: "flex gap-4",
            label { class: "flex items-center gap-2 cursor-pointer group",
                input {
                    r#type: "radio",
                    name: "search_source",
                    class: "accent-gray-200",
                    checked: selected_source == SearchSource::MusicBrainz,
                    onchange: move |_| on_select.call(SearchSource::MusicBrainz),
                }
                span { class: "text-xs text-gray-300 group-hover:text-white transition-colors",
                    "MusicBrainz"
                }
            }
            label { class: "flex items-center gap-2 cursor-pointer group",
                input {
                    r#type: "radio",
                    name: "search_source",
                    class: "accent-gray-200",
                    checked: selected_source == SearchSource::Discogs,
                    onchange: move |_| on_select.call(SearchSource::Discogs),
                }
                span { class: "text-xs text-gray-300 group-hover:text-white transition-colors",
                    "Discogs"
                }
            }
        }
    }
}
