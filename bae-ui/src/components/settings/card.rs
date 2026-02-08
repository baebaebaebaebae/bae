//! Reusable layout and card components for settings sections

use dioxus::prelude::*;

/// Standard layout wrapper for a settings section content pane.
///
/// Constrains width and provides consistent vertical spacing.
#[component]
pub fn SettingsSection(children: Element) -> Element {
    rsx! {
        div { class: "max-w-2xl space-y-6", {children} }
    }
}

/// A consistent card container used across all settings sections.
///
/// Renders a bordered, rounded container with standard padding.
#[component]
pub fn SettingsCard(#[props(default = "p-6")] padding: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "border border-border-subtle rounded-lg {padding}", {children} }
    }
}
