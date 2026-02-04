//! Import source selector view

use crate::components::button::ButtonVariant;
use crate::components::segmented_control::{Segment, SegmentedControl};
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

    pub fn value(&self) -> &'static str {
        match self {
            ImportSource::Folder => "folder",
            ImportSource::Torrent => "torrent",
            ImportSource::Cd => "cd",
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

    fn from_value(value: &str) -> ImportSource {
        match value {
            "torrent" => ImportSource::Torrent,
            "cd" => ImportSource::Cd,
            _ => ImportSource::Folder,
        }
    }
}

/// Import source selector tabs
#[component]
pub fn ImportSourceSelectorView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
) -> Element {
    let mut segments: Vec<Segment> = ImportSource::all()
        .iter()
        .map(|s| Segment::new(s.label(), s.value()))
        .collect();

    if !cfg!(feature = "torrent") {
        segments.push(Segment::new("Torrent", "torrent").disabled());
    }
    if !cfg!(feature = "cd-rip") {
        segments.push(Segment::new("CD", "cd").disabled());
    }

    rsx! {
        SegmentedControl {
            segments,
            selected: selected_source.value().to_string(),
            selected_variant: ButtonVariant::Secondary,
            on_select: move |value: &str| {
                on_source_select.call(ImportSource::from_value(value));
            },
        }
    }
}
