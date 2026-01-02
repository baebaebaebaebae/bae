use crate::config::use_config;
use crate::AppContext;
use dioxus::prelude::*;
use tracing::{error, info};

#[component]
pub fn SubsonicSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut enabled = use_signal(|| config.subsonic_enabled);
    let mut port = use_signal(|| config.subsonic_port.to_string());

    let original_enabled = config.subsonic_enabled;
    let original_port = config.subsonic_port.to_string();

    let has_changes = *enabled.read() != original_enabled || *port.read() != original_port;

    let save_changes = move |_| {
        let new_enabled = *enabled.read();
        let new_port = port.read().clone();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.subsonic_enabled = new_enabled;
            config.subsonic_port = new_port.parse().unwrap_or(4533);

            match config.save() {
                Ok(()) => {
                    info!("Saved Subsonic settings");
                    is_editing.set(false);
                }
                Err(e) => {
                    error!("Failed to save config: {}", e);
                    save_error.set(Some(e.to_string()));
                }
            }
            is_saving.set(false);
        });
    };

    let cancel_edit = move |_| {
        enabled.set(original_enabled);
        port.set(original_port.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "max-w-2xl space-y-6",
            h2 { class: "text-xl font-semibold text-white mb-6", "Subsonic Server" }

            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Server Settings" }
                    if !*is_editing.read() {
                        button {
                            class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                            onclick: move |_| is_editing.set(true),
                            "Edit"
                        }
                    }
                }

                if *is_editing.read() {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: *enabled.read(),
                                onchange: move |e| enabled.set(e.checked()),
                            }
                            label { class: "text-sm text-gray-300", "Enable Subsonic API server" }
                        }
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-32", "Port:" }
                            input {
                                r#type: "number",
                                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                min: "1024",
                                max: "65535",
                                value: "{port}",
                                oninput: move |e| port.set(e.value()),
                            }
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Status:" }
                            span { class: if config.subsonic_enabled { "text-green-400" } else { "text-gray-500" },
                                if config.subsonic_enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Port:" }
                            span { class: "text-white font-mono", "{config.subsonic_port}" }
                        }
                        if config.subsonic_enabled {
                            div { class: "flex items-center gap-2",
                                span { class: "text-gray-400", "URL:" }
                                span { class: "text-indigo-400 font-mono",
                                    "http://127.0.0.1:{config.subsonic_port}"
                                }
                            }
                        }
                    }
                }
            }

            if *is_editing.read() {
                if let Some(error) = save_error.read().as_ref() {
                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                        "{error}"
                    }
                }

                div { class: "flex gap-3",
                    button {
                        class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: !has_changes || *is_saving.read(),
                        onclick: save_changes,
                        if *is_saving.read() {
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                        onclick: cancel_edit,
                        "Cancel"
                    }
                }

                div { class: "p-4 bg-yellow-900/20 border border-yellow-700/50 rounded-lg",
                    p { class: "text-sm text-yellow-200/80",
                        "Changes require an app restart to take effect."
                    }
                }
            }

            div { class: "bg-gray-800 rounded-lg p-6",
                h3 { class: "text-lg font-medium text-white mb-4", "About Subsonic" }
                div { class: "space-y-3 text-sm text-gray-400",
                    p {
                        "The Subsonic API allows you to stream your music library to compatible apps like "
                        "Symfonium, Ultrasonic, or any Subsonic/Navidrome-compatible client."
                    }
                    p {
                        "Connect your mobile app to "
                        span { class: "font-mono text-indigo-400",
                            "http://YOUR_IP:{config.subsonic_port}"
                        }
                        " (or use localhost for the same device)."
                    }
                }
            }
        }
    }
}
