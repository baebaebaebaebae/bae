//! Disc ID pill component

use crate::components::{Pill, PillVariant, Tooltip};
use crate::floating_ui::Placement;
use dioxus::prelude::*;

/// Source of the disc ID
#[derive(Clone, Copy, PartialEq)]
pub enum DiscIdSource {
    /// Read directly from a physical CD
    Cd,
    /// Calculated from rip logs or CUE/FLAC files
    Files,
}

/// A clickable pill that displays a MusicBrainz disc ID and links to its page.
/// Includes a tooltip explaining how the disc ID is computed.
#[component]
pub fn DiscIdPill(disc_id: String, source: DiscIdSource, tooltip_placement: Placement) -> Element {
    let tooltip_text = match source {
        DiscIdSource::Cd => "Based on CD layout. Read from the CD.",
        DiscIdSource::Files => "Based on CD layout. Calculated using rip logs or CUE/FLAC files.",
    };

    rsx! {
        Tooltip { text: tooltip_text, placement: tooltip_placement, nowrap: true,
            Pill {
                variant: PillVariant::Link,
                href: "https://musicbrainz.org/cdtoc/{disc_id}",
                monospace: true,
                "{disc_id}"
            }
        }
    }
}
