//! Search source selector view component

use crate::components::button::ButtonVariant;
use crate::components::segmented_control::{Segment, SegmentedControl};
use crate::display_types::SearchSource;
use dioxus::prelude::*;

/// Segmented button group to select between MusicBrainz and Discogs
#[component]
pub fn SearchSourceSelectorView(
    selected_source: SearchSource,
    on_select: EventHandler<SearchSource>,
) -> Element {
    let segments = vec![
        Segment::new("MusicBrainz", "musicbrainz"),
        Segment::new("Discogs", "discogs"),
    ];

    rsx! {
        SegmentedControl {
            segments,
            selected: match selected_source {
                SearchSource::MusicBrainz => "musicbrainz".to_string(),
                SearchSource::Discogs => "discogs".to_string(),
            },
            selected_variant: ButtonVariant::Secondary,
            on_select: move |value: &str| {
                let source = match value {
                    "discogs" => SearchSource::Discogs,
                    _ => SearchSource::MusicBrainz,
                };
                on_select.call(source);
            },
        }
    }
}
