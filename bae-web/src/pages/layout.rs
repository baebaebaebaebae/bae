use crate::playback::WebPlaybackService;
use crate::Route;
use bae_ui::stores::playback::PlaybackUiState;
use bae_ui::stores::ui::{SidebarState, SidebarStateStoreExt};
use bae_ui::{
    AppLayoutView, GroupedSearchResults, NavItem, NowPlayingBarView, QueueSidebarView, TitleBarView,
};
use dioxus::prelude::*;
use wasm_bindgen_x::JsCast;

#[component]
pub fn AppLayout() -> Element {
    let current_route = use_route::<Route>();
    let mut search_query = use_signal(String::new);

    let playback_store = use_context_provider(|| {
        use_store(|| PlaybackUiState {
            volume: 1.0,
            ..PlaybackUiState::default()
        })
    });
    let sidebar_store = use_store(SidebarState::default);
    let mut service = use_context_provider(|| Signal::new(WebPlaybackService::new(playback_store)));

    let nav_items = vec![NavItem {
        id: "library".to_string(),
        label: "Library".to_string(),
        is_active: matches!(current_route, Route::Library {} | Route::AlbumDetail { .. }),
    }];

    rsx! {
        // Hidden audio element — persists across route changes
        audio {
            id: "bae-audio",
            preload: "metadata",
            onmounted: move |evt| {
                if let Some(el) = evt.data().downcast::<web_sys_x::Element>() {
                    if let Ok(media_el) = el.clone().dyn_into::<web_sys_x::HtmlMediaElement>() {
                        service.write().set_audio_element(media_el);
                    }
                }
            },
            ontimeupdate: move |_| service.write().on_time_update(),
            onended: move |_| service.write().on_ended(),
            onloadedmetadata: move |_| service.write().on_loaded_metadata(),
            onerror: move |_| service.write().on_error(),
            onplay: move |_| service.write().on_play(),
            onpause: move |_| service.write().on_pause_event(),
        }

        AppLayoutView {
            title_bar: rsx! {
                TitleBarView {
                    nav_items,
                    on_nav_click: move |id: String| {
                        if id == "library" {
                            navigator().push(Route::Library {});
                        }
                    },
                    search_value: search_query(),
                    on_search_change: move |value: String| {
                        search_query.set(value);
                    },
                    search_results: GroupedSearchResults::default(),
                    on_search_result_click: |_| {},
                    on_search_focus: |_| {},
                    on_search_blur: |_| {},
                    on_settings_click: |_| {},
                    left_padding: 16,
                }
            },
            playback_bar: rsx! {
                NowPlayingBarView {
                    state: playback_store,
                    on_previous: move |_| service.write().previous(),
                    on_pause: move |_| service.write().pause(),
                    on_resume: move |_| service.write().resume(),
                    on_next: move |_| service.write().next(),
                    on_seek: move |ms: u64| service.write().seek(ms),
                    on_cycle_repeat: move |_| service.write().cycle_repeat_mode(),
                    on_volume_change: move |vol: f32| service.write().set_volume(vol),
                    on_toggle_mute: move |_| service.write().toggle_mute(),
                    on_toggle_queue: move |_| {
                        let current = *sidebar_store.is_open().read();
                        sidebar_store.is_open().set(!current);
                    },
                    on_track_click: |_| {},
                    on_artist_click: |_| {},
                    on_dismiss_error: move |_| service.write().dismiss_error(),
                }
            },
            queue_sidebar: rsx! {
                QueueSidebarView {
                    sidebar: sidebar_store,
                    playback: playback_store,
                    on_close: move |_| sidebar_store.is_open().set(false),
                    on_clear: move |_| service.write().clear_queue(),
                    on_remove: move |idx: usize| service.write().remove_from_queue(idx),
                    on_track_click: |_| {},
                    on_play_index: move |idx: usize| service.write().skip_to(idx),
                    on_pause: move |_| service.write().pause(),
                    on_resume: move |_| service.write().resume(),
                }
            },
            Outlet::<Route> {}
        }
    }
}
