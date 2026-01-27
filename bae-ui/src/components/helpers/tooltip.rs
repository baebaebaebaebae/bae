//! Tooltip component using floating-ui for positioning

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use crate::floating_ui::{self, ComputePositionOptions, Placement};

/// Counter for generating unique tooltip IDs
static TOOLTIP_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A hover-triggered tooltip that displays text near an anchor element.
///
/// Wrap content in this component to add a tooltip on hover.
#[component]
pub fn Tooltip(
    /// The tooltip text to display
    text: String,
    /// Placement relative to anchor (default: Top)
    #[props(default = Placement::Top)]
    placement: Placement,
    /// Children (the element to attach tooltip to)
    children: Element,
) -> Element {
    let mut is_visible = use_signal(|| false);

    // Generate unique IDs for this tooltip instance
    let ids = use_hook(|| {
        let id = TOOLTIP_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        (format!("tooltip-anchor-{}", id), format!("tooltip-{}", id))
    });
    let (anchor_id, tooltip_id) = ids.clone();

    // Position the tooltip when visible
    let anchor_id_effect = anchor_id.clone();
    let tooltip_id_effect = tooltip_id.clone();
    use_effect(move || {
        if !is_visible() {
            return;
        }

        let anchor_id = anchor_id_effect.clone();
        let tooltip_id = tooltip_id_effect.clone();

        spawn(async move {
            let Some(window) = web_sys_x::window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let Some(anchor) = document.get_element_by_id(&anchor_id) else {
                return;
            };
            let Some(tooltip) = document.get_element_by_id(&tooltip_id) else {
                return;
            };

            let options = ComputePositionOptions {
                placement,
                offset: Some(8.0),
                flip: true,
                shift: true,
            };

            if let Ok(result) = floating_ui::compute_position(&anchor, &tooltip, options).await {
                let style = format!(
                    "position: fixed; top: 0; left: 0; transform: translate({}px, {}px);",
                    result.x, result.y
                );
                let _ = tooltip.set_attribute("style", &style);
            }
        });
    });

    rsx! {
        // Wrapper that captures hover events
        span {
            id: "{anchor_id}",
            class: "inline-flex",
            onmouseenter: move |_| is_visible.set(true),
            onmouseleave: move |_| is_visible.set(false),
            {children}
        }

        // Tooltip portal (rendered at body level via fixed positioning)
        if is_visible() {
            div {
                id: "{tooltip_id}",
                class: "z-50 px-3 py-2 text-sm text-gray-200 bg-gray-800 rounded-lg shadow-lg border border-gray-700 max-w-xs",
                style: "position: fixed; top: -9999px; left: -9999px;",
                role: "tooltip",
                "{text}"
            }
        }
    }
}
