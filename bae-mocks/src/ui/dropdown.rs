//! Dropdown select component

use dioxus::prelude::*;

/// Dropdown style variants
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum DropdownStyle {
    /// Transparent background with custom chevron
    Transparent,
    /// Gray background with border
    #[default]
    Panel,
}

/// A styled dropdown select component
#[component]
pub fn Dropdown(
    value: String,
    onchange: EventHandler<String>,
    #[props(default)] style: DropdownStyle,
    children: Element,
) -> Element {
    let (class, inline_style) = match style {
        DropdownStyle::Transparent => (
            "bg-transparent text-white font-medium text-sm appearance-none cursor-pointer pr-4 focus:outline-none",
            Some("background-image: url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E\"); background-position: right center; background-repeat: no-repeat; background-size: 1.25em;"),
        ),
        DropdownStyle::Panel => (
            "bg-gray-700 text-gray-300 text-sm rounded px-2 py-1 border border-gray-600",
            None,
        ),
    };

    rsx! {
        select {
            class,
            style: inline_style.unwrap_or(""),
            value,
            onchange: move |e| onchange.call(e.value()),
            {children}
        }
    }
}
