use crate::Route;
use bae_ui::{AppLayoutView, GroupedSearchResults, NavItem, TitleBarView};
use dioxus::prelude::*;

#[component]
pub fn AppLayout() -> Element {
    let current_route = use_route::<Route>();
    let mut search_query = use_signal(String::new);

    let nav_items = vec![NavItem {
        id: "library".to_string(),
        label: "Library".to_string(),
        is_active: matches!(current_route, Route::Library {} | Route::AlbumDetail { .. }),
    }];

    rsx! {
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
            Outlet::<Route> {}
        }
    }
}
