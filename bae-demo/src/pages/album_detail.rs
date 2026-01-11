//! Album detail page

use crate::demo_data;
use crate::Route;
use bae_ui::{AlbumDetailView, BackButton, ErrorDisplay, PageContainer, PlaybackDisplay, Track};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetail(album_id: String) -> Element {
    let album = demo_data::get_album(&album_id);
    let artists = demo_data::get_artists_for_album(&album_id);
    let releases = demo_data::get_releases_for_album(&album_id);
    let tracks = demo_data::get_tracks_for_album(&album_id);

    // Create per-track signals for reactivity
    let track_signals: Vec<Signal<Track>> = tracks.into_iter().map(Signal::new).collect();

    // Signals for import state (not used in demo, but required by component)
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    let selected_release_id = releases.first().map(|r| r.id.clone());

    rsx! {
        PageContainer {
            BackButton {
                on_click: move |_| {
                    navigator().push(Route::Library {});
                },
            }

            if let Some(album) = album {
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    tracks: track_signals,
                    selected_release_id,
                    import_progress,
                    import_error,
                    playback: PlaybackDisplay::Stopped,
                    on_release_select: |_release_id: String| {},
                    on_album_deleted: |_| {},
                    on_export_release: |_| {},
                    on_delete_album: |_| {},
                    on_delete_release: |_| {},
                    on_track_play: |_| {},
                    on_track_pause: |_| {},
                    on_track_resume: |_| {},
                    on_track_add_next: |_| {},
                    on_track_add_to_queue: |_| {},
                    on_track_export: |_| {},
                    on_play_album: |_| {},
                    on_add_album_to_queue: |_| {},
                }
            } else {
                ErrorDisplay { message: "Album not found in demo data".to_string() }
            }
        }
    }
}
