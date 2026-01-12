//! Chevron icon component

use dioxus::prelude::*;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ChevronDirection {
    Up,
    #[default]
    Down,
}

/// A chevron/caret icon
#[component]
pub fn Chevron(#[props(default)] direction: ChevronDirection) -> Element {
    let rotation = match direction {
        ChevronDirection::Up => "rotate-180",
        ChevronDirection::Down => "",
    };

    rsx! {
        svg {
            class: "w-4 h-4 {rotation}",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 20 20",
            path {
                stroke: "currentColor",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "1.5",
                d: "m6 8 4 4 4-4",
            }
        }
    }
}
