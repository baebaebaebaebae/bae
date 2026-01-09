use dioxus::prelude::*;

#[component]
pub fn DeleteReleaseDialog(
    release_id: String,
    is_last_release: bool,
    is_deleting: Signal<bool>,
    on_confirm: EventHandler<String>, // Pass release_id to caller
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
            onclick: move |_| {
                if !is_deleting() {
                    on_cancel.call(());
                }
            },
            div {
                class: "bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4",
                onclick: move |evt| evt.stop_propagation(),
                h2 { class: "text-xl font-bold text-white mb-4", "Delete Release?" }
                p { class: "text-gray-300 mb-6",
                    "Are you sure you want to delete this release? This will delete all tracks and associated data for this release."
                    if is_last_release {
                        " Since this is the only release, the album will also be deleted."
                    } else {
                        ""
                    }
                }
                div { class: "flex gap-3 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg",
                        disabled: is_deleting(),
                        onclick: move |_| {
                            if !is_deleting() {
                                on_cancel.call(());
                            }
                        },
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-lg",
                        disabled: is_deleting(),
                        onclick: {
                            let release_id = release_id.clone();
                            move |_| {
                                if !is_deleting() {
                                    on_confirm.call(release_id.clone());
                                }
                            }
                        },
                        if is_deleting() {
                            "Deleting..."
                        } else {
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
