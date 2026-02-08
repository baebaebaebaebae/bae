//! Cover picker wrapper - combines existing images + remote covers into GalleryLightbox

use crate::components::{GalleryItem, GalleryItemContent, GalleryLightbox};
use crate::display_types::CoverChange;
use crate::stores::album_detail::{AlbumDetailState, AlbumDetailStateStoreExt};
use dioxus::prelude::*;

/// Cover picker that reuses GalleryLightbox in selection mode.
///
/// Combines existing release images with async-fetched remote covers.
/// Maps gallery index back to `CoverChange` on selection.
#[component]
pub fn CoverPickerWrapper(
    state: ReadStore<AlbumDetailState>,
    show: Signal<bool>,
    on_select: EventHandler<CoverChange>,
) -> Element {
    let images = state.images().read().clone();
    let remote_covers = state.remote_covers().read().clone();
    let loading_remote_covers = *state.loading_remote_covers().read();

    // Build combined gallery items + parallel CoverChange mapping
    let mut gallery_items: Vec<GalleryItem> = Vec::new();
    let mut change_map: Vec<CoverChange> = Vec::new();

    // Existing images first
    let mut cover_index: Option<usize> = None;
    for img in &images {
        if img.is_cover {
            cover_index = Some(gallery_items.len());
        }
        change_map.push(CoverChange::ReleaseImage {
            file_id: img.id.clone(),
        });
        gallery_items.push(GalleryItem {
            label: img.filename.clone(),
            content: GalleryItemContent::Image {
                url: img.url.clone(),
                thumbnail_url: img.url.clone(),
            },
        });
    }

    // Remote covers after existing images
    for rc in &remote_covers {
        change_map.push(CoverChange::RemoteCover {
            url: rc.url.clone(),
            source: rc.source.clone(),
        });
        gallery_items.push(GalleryItem {
            label: rc.label.clone(),
            content: GalleryItemContent::Image {
                url: rc.url.clone(),
                thumbnail_url: rc.thumbnail_url.clone(),
            },
        });
    }

    // If still loading, add a placeholder text item
    if loading_remote_covers {
        gallery_items.push(GalleryItem {
            label: "Loading remote covers...".to_string(),
            content: GalleryItemContent::Text {
                content: Some(Ok(
                    "Fetching covers from MusicBrainz and Discogs...".to_string()
                )),
            },
        });
    }

    let initial_index = cover_index.unwrap_or(0);
    let is_open: ReadSignal<bool> = show.into();

    rsx! {
        GalleryLightbox {
            is_open,
            items: gallery_items,
            initial_index,
            on_close: move |_| show.set(false),
            on_navigate: move |_: usize| {},
            selected_index: cover_index,
            on_select: move |index: usize| {
                if let Some(change) = change_map.get(index) {
                    on_select.call(change.clone());
                    show.set(false);
                }
            },
        }
    }
}
