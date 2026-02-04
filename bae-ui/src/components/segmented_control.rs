//! Segmented control component — a group of toggle buttons where one is selected

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

/// A single segment in a segmented control
#[derive(Clone, PartialEq)]
pub struct Segment {
    pub label: &'static str,
    pub value: &'static str,
    pub disabled: bool,
}

impl Segment {
    pub fn new(label: &'static str, value: &'static str) -> Self {
        Self {
            label,
            value,
            disabled: false,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// A row of toggle buttons where exactly one is selected
#[component]
pub fn SegmentedControl(
    segments: Vec<Segment>,
    selected: String,
    on_select: EventHandler<&'static str>,
    selected_variant: ButtonVariant,
) -> Element {
    rsx! {
        div { class: "flex gap-1 bg-gray-800/50 rounded-lg p-1",
            for segment in &segments {
                Button {
                    variant: if segment.value == selected { selected_variant } else { ButtonVariant::Ghost },
                    size: ButtonSize::Small,
                    disabled: segment.disabled,
                    onclick: {
                        let value = segment.value;
                        move |_| on_select.call(value)
                    },
                    "{segment.label}"
                }
            }
        }
    }
}
