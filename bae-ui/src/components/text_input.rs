//! Reusable text input component

use dioxus::prelude::*;

/// Text input size
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextInputSize {
    /// Smaller padding
    Small,
    /// Standard padding
    Medium,
}

/// Text input type
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextInputType {
    Text,
    Password,
}

impl TextInputType {
    fn as_str(self) -> &'static str {
        match self {
            TextInputType::Text => "text",
            TextInputType::Password => "password",
        }
    }
}

/// Reusable text input component with consistent styling
#[component]
pub fn TextInput(
    value: String,
    on_input: EventHandler<String>,
    size: TextInputSize,
    input_type: TextInputType,
    #[props(default)] placeholder: Option<&'static str>,
    #[props(default)] disabled: bool,
    #[props(default)] monospace: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] autofocus: bool,
) -> Element {
    let padding = match size {
        TextInputSize::Small => "px-2.5 py-1.5 text-sm",
        TextInputSize::Medium => "px-3 py-2",
    };

    let base = "w-full bg-gray-800/50 rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-gray-300 placeholder-gray-500";

    let disabled_class = if disabled {
        "opacity-50 cursor-not-allowed"
    } else {
        ""
    };

    let font_class = if monospace { "font-mono" } else { "" };

    let class = format!("{base} {padding} {disabled_class} {font_class}");
    let type_str = input_type.as_str();

    rsx! {
        input {
            r#type: "{type_str}",
            class: "{class}",
            id: id.as_deref(),
            value: "{value}",
            placeholder,
            disabled,
            autocomplete: "off",
            autocapitalize: "off",
            autocorrect: "off",
            spellcheck: false,
            oninput: move |e| on_input.call(e.value()),
            onmounted: move |event| async move {
                if autofocus {
                    let _ = event.data().set_focus(true).await;
                }
            },
        }
    }
}
