//! Library management section for settings

use crate::components::SettingsSection;
use dioxus::prelude::*;

/// Library info for the settings UI (uses String for path since bae-ui targets wasm too)
#[derive(Clone, PartialEq)]
pub struct LibraryInfo {
    pub id: String,
    pub name: Option<String>,
    pub path: String,
    pub is_active: bool,
}

/// Pure view component for the Library settings section
#[component]
pub fn LibrarySectionView(
    libraries: Vec<LibraryInfo>,
    on_switch: EventHandler<String>,
    on_create: EventHandler<()>,
    on_join: EventHandler<()>,
    on_rename: EventHandler<(String, String)>,
    on_remove: EventHandler<String>,
) -> Element {
    let mut renaming_path = use_signal(|| None::<String>);
    let mut rename_value = use_signal(String::new);
    let mut confirming_switch = use_signal(|| None::<String>);
    let mut confirming_delete = use_signal(|| None::<String>);

    let mut start_rename = move |lib: &LibraryInfo| {
        renaming_path.set(Some(lib.path.clone()));
        rename_value.set(lib.name.clone().unwrap_or_default());
    };

    let mut commit_rename = move |path: String| {
        let new_name = rename_value.read().clone();
        renaming_path.set(None);
        on_rename.call((path, new_name));
    };

    let mut cancel_rename = move |_| {
        renaming_path.set(None);
    };

    rsx! {
        SettingsSection {
            div { class: "flex items-center justify-between",
                div {
                    h2 { class: "text-xl font-semibold text-white", "Library" }
                    p { class: "text-sm text-gray-400 mt-1", "Manage your music libraries" }
                }
                div { class: "flex gap-2",
                    button {
                        class: "px-3 py-1.5 text-sm bg-indigo-600 hover:bg-indigo-500 text-white rounded-md transition-colors",
                        onclick: move |_| on_create.call(()),
                        "New Library"
                    }
                    button {
                        class: "px-3 py-1.5 text-sm bg-gray-700 hover:bg-gray-600 text-white rounded-md transition-colors",
                        onclick: move |_| on_join.call(()),
                        "Join Shared..."
                    }
                }
            }

            div { class: "space-y-2",
                for lib in &libraries {
                    {
                        let lib_path_rename = lib.path.clone();
                        let lib_path_switch = lib.path.clone();
                        let lib_path_confirm_switch = lib.path.clone();
                        let lib_path_remove = lib.path.clone();
                        let lib_path_confirm = lib.path.clone();
                        let is_renaming = renaming_path.read().as_ref() == Some(&lib.path);
                        let is_confirming_switch = confirming_switch.read().as_ref() == Some(&lib.path);
                        let is_confirming = confirming_delete.read().as_ref() == Some(&lib.path);

                        rsx! {
                            div {
                                key: "{lib.id}",
                                class: "flex items-center justify-between p-4 rounded-lg border border-border-subtle",
                                div { class: "flex-1 min-w-0",
                                    div { class: "flex items-center gap-2",
                                        if is_renaming {
                                            input {
                                                class: "bg-gray-700 text-white text-sm rounded px-2 py-1 border border-gray-600 focus:border-indigo-500 outline-none",
                                                value: "{rename_value}",
                                                autofocus: true,
                                                oninput: move |e| rename_value.set(e.value()),
                                                onkeydown: {
                                                    let path = lib_path_rename.clone();
                                                    move |e: KeyboardEvent| {
                                                        if e.key() == Key::Enter {
                                                            commit_rename(path.clone());
                                                        } else if e.key() == Key::Escape {
                                                            cancel_rename(());
                                                        }
                                                    }
                                                },
                                            }
                                        } else {
                                            span { class: "text-sm font-medium text-white",
                                                "{lib.name.as_deref().unwrap_or(&lib.id)}"
                                            }
                                        }
                                        if lib.is_active {
                                            span { class: "px-2 py-0.5 text-xs bg-indigo-600 text-white rounded-full",
                                                "Active"
                                            }
                                        }
                                    }
                                    p { class: "text-xs text-gray-500 mt-1 truncate", "{lib.path}" }
                                }
                                div { class: "flex items-center gap-2 ml-4 flex-shrink-0",
                                    if !is_renaming {
                                        button {
                                            class: "px-2 py-1 text-xs text-gray-400 hover:text-white transition-colors",
                                            onclick: {
                                                let lib_clone = lib.clone();
                                                move |_| start_rename(&lib_clone)
                                            },
                                            "Rename"
                                        }
                                    }
                                    if !lib.is_active {
                                        if is_confirming_switch {
                                            span { class: "text-xs text-gray-400 mr-1", "App will restart. Switch?" }
                                            button {
                                                class: "px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 text-white rounded transition-colors",
                                                onclick: move |_| {
                                                    confirming_switch.set(None);
                                                    on_switch.call(lib_path_switch.clone());
                                                },
                                                "Yes"
                                            }
                                            button {
                                                class: "px-2 py-1 text-xs text-gray-400 hover:text-white transition-colors",
                                                onclick: move |_| confirming_switch.set(None),
                                                "No"
                                            }
                                        } else {
                                            button {
                                                class: "px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 text-white rounded transition-colors",
                                                onclick: move |_| confirming_switch.set(Some(lib_path_confirm_switch.clone())),
                                                "Switch"
                                            }
                                        }
                                        if is_confirming {
                                            span { class: "text-xs text-red-400 mr-1", "Delete?" }
                                            button {
                                                class: "px-2 py-1 text-xs bg-red-600 hover:bg-red-500 text-white rounded transition-colors",
                                                onclick: move |_| {
                                                    confirming_delete.set(None);
                                                    on_remove.call(lib_path_remove.clone());
                                                },
                                                "Yes"
                                            }
                                            button {
                                                class: "px-2 py-1 text-xs text-gray-400 hover:text-white transition-colors",
                                                onclick: move |_| confirming_delete.set(None),
                                                "No"
                                            }
                                        } else {
                                            button {
                                                class: "px-2 py-1 text-xs text-red-400 hover:text-red-300 transition-colors",
                                                onclick: move |_| confirming_delete.set(Some(lib_path_confirm.clone())),
                                                "Delete"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
