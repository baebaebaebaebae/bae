//! Disc ID pill component

use crate::components::{Pill, PillVariant, Tooltip};
use crate::floating_ui::Placement;
use dioxus::prelude::*;

/// A clickable pill that displays a MusicBrainz disc ID and links to its page.
/// Includes a tooltip explaining how the disc ID is computed.
#[component]
pub fn DiscIdPill(disc_id: String) -> Element {
    rsx! {
        Tooltip {
            text: "MusicBrainz Disc ID computed from CD table of contents (track count, offsets, and total length). Extracted from .log files or read directly from CDs."
                .to_string(),
            placement: Placement::Bottom,
            Pill {
                variant: PillVariant::Link,
                href: "https://musicbrainz.org/cdtoc/{disc_id}",
                monospace: true,
                "{disc_id}"
            }
        }
    }
}
