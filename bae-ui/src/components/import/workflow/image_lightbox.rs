//! Image lightbox view component

use super::gallery_lightbox::{GalleryImage, GalleryLightbox};
use crate::display_types::FileInfo;
use dioxus::prelude::*;

/// Image lightbox view for viewing images in full screen
#[component]
pub fn ImageLightboxView(
    /// Controls whether the lightbox is open
    is_open: ReadSignal<bool>,
    /// Artwork files with display_url
    images: Vec<FileInfo>,
    /// Current image index
    current_index: usize,
    /// Called when lightbox is closed
    on_close: EventHandler<()>,
    /// Called when navigating to a different image
    on_navigate: EventHandler<usize>,
) -> Element {
    let gallery_images: Vec<GalleryImage> = images
        .iter()
        .map(|f| GalleryImage {
            display_url: f.display_url.clone(),
            label: f.name.clone(),
        })
        .collect();

    rsx! {
        GalleryLightbox {
            is_open,
            images: gallery_images,
            initial_index: current_index,
            on_close,
            on_navigate,
            selected_index: None::<usize>,
            on_select: |_| {},
        }
    }
}
