//! Cover picker wrapper - combines current cover + release images + remote covers into GalleryLightbox

use crate::components::{GalleryItem, GalleryItemContent, GalleryLightbox};
use crate::display_types::CoverChange;
use crate::stores::album_detail::{AlbumDetailState, AlbumDetailStateStoreExt};
use dioxus::prelude::*;

/// Cover picker that reuses GalleryLightbox in selection mode.
///
/// Item 0 is always the current cover (selected_index=0). Release images and
/// remote covers follow. The change_map skips index 0 so selecting the current
/// cover is a no-op.
#[component]
pub fn CoverPickerWrapper(
    state: ReadStore<AlbumDetailState>,
    show: Signal<bool>,
    on_select: EventHandler<CoverChange>,
) -> Element {
    let album = state.album().read().clone();
    let images = state.images().read().clone();
    let remote_covers = state.remote_covers().read().clone();
    let loading_remote_covers = *state.loading_remote_covers().read();

    let mut gallery_items: Vec<GalleryItem> = Vec::new();
    // change_map is offset by 1 from gallery_items (index 0 = current cover, no change)
    let mut change_map: Vec<CoverChange> = Vec::new();

    // Item 0: current cover (if one exists)
    let cover_url = album.as_ref().and_then(|a| a.cover_url.clone());
    let has_cover = cover_url.is_some();
    if let Some(url) = cover_url {
        gallery_items.push(GalleryItem {
            label: "Current cover".to_string(),
            content: GalleryItemContent::Image {
                url: url.clone(),
                thumbnail_url: url,
            },
        });
    }

    // Release images
    for img in &images {
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

    // Remote covers
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

    let is_open: ReadSignal<bool> = show.into();

    rsx! {
        GalleryLightbox {
            is_open,
            items: gallery_items,
            initial_index: 0usize,
            on_close: move |_| show.set(false),
            on_navigate: move |_: usize| {},
            selected_index: if has_cover { Some(0usize) } else { None },
            on_select: move |index: usize| {
                let offset = if has_cover { 1 } else { 0 };
                if let Some(change) = change_map.get(index.wrapping_sub(offset)) {
                    on_select.call(change.clone());
                    show.set(false);
                }
            },
        }
    }
}
