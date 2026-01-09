use super::album_art::AlbumArt;
use crate::ui::display_types::Album;
use dioxus::prelude::*;

#[component]
pub fn AlbumCoverSection(
    album: Album,
    import_progress: ReadSignal<Option<u8>>,
    is_deleting: Signal<bool>,
    is_exporting: Signal<bool>,
    first_release_id: Option<String>,
    has_single_release: bool,
    // Callbacks (all optional - if None, actions are hidden)
    #[props(into)] on_export: Option<EventHandler<String>>,
    #[props(into)] on_delete: Option<EventHandler<String>>,
    #[props(into)] on_view_release_info: Option<EventHandler<String>>,
) -> Element {
    let mut show_dropdown = use_signal(|| false);
    let mut hover_cover = use_signal(|| false);

    // Check if we have any actions to show
    let has_actions = on_export.is_some() || on_delete.is_some() || on_view_release_info.is_some();

    rsx! {
        div {
            class: "mb-6 relative",
            onmouseenter: move |_| hover_cover.set(true),
            onmouseleave: move |_| hover_cover.set(false),
            AlbumArt {
                title: album.title.clone(),
                cover_url: album.cover_url.clone(),
                import_progress,
                is_ephemeral: false,
            }
            // Only show dropdown button if we have actions
            if has_actions && (hover_cover() || show_dropdown()) {
                div { class: "absolute top-2 right-2 z-10",
                    button {
                        class: "w-8 h-8 bg-gray-800/40 hover:bg-gray-800/60 text-white rounded-lg flex items-center justify-center transition-colors",
                        disabled: import_progress().is_some() || is_deleting(),
                        class: if import_progress().is_some() || is_deleting() { "opacity-50 cursor-not-allowed" } else { "" },
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if !is_deleting() && import_progress().is_none() {
                                show_dropdown.set(!show_dropdown());
                            }
                        },
                        div { class: "flex flex-col gap-1",
                            div { class: "w-1 h-1 bg-white rounded-full" }
                            div { class: "w-1 h-1 bg-white rounded-full" }
                            div { class: "w-1 h-1 bg-white rounded-full" }
                        }
                    }
                    if show_dropdown() {
                        div { class: "absolute top-full right-0 mt-2 bg-gray-700 rounded-lg shadow-lg overflow-hidden z-20 border border-gray-600 min-w-[160px]",
                            // Release Info - only for single release
                            if has_single_release {
                                if let Some(ref release_id) = first_release_id {
                                    if let Some(ref handler) = on_view_release_info {
                                        button {
                                            class: "w-full px-4 py-3 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2",
                                            disabled: is_deleting() || is_exporting(),
                                            onclick: {
                                                let release_id = release_id.clone();
                                                let handler = *handler;
                                                move |evt| {
                                                    evt.stop_propagation();
                                                    show_dropdown.set(false);
                                                    handler.call(release_id.clone());
                                                }
                                            },
                                            "Release Info"
                                        }
                                    }
                                    if let Some(ref handler) = on_export {
                                        button {
                                            class: "w-full px-4 py-3 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2",
                                            disabled: is_deleting() || is_exporting(),
                                            onclick: {
                                                let release_id = release_id.clone();
                                                let handler = *handler;
                                                move |evt| {
                                                    evt.stop_propagation();
                                                    show_dropdown.set(false);
                                                    handler.call(release_id.clone());
                                                }
                                            },
                                            if is_exporting() {
                                                "Exporting..."
                                            } else {
                                                "Export"
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(ref handler) = on_delete {
                                button {
                                    class: "w-full px-4 py-3 text-left text-red-400 hover:bg-gray-600 transition-colors flex items-center gap-2",
                                    disabled: is_deleting(),
                                    onclick: {
                                        let album_id = album.id.clone();
                                        let handler = *handler;
                                        move |evt| {
                                            evt.stop_propagation();
                                            show_dropdown.set(false);
                                            handler.call(album_id.clone());
                                        }
                                    },
                                    "Delete Album"
                                }
                            }
                        }
                    }
                }
            }
        }
        if show_dropdown() {
            div {
                class: "fixed inset-0 z-[5]",
                onclick: move |_| show_dropdown.set(false),
            }
        }
    }
}
