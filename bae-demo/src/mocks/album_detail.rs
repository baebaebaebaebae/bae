//! AlbumDetailView mock with playback controls

use bae_ui::{Album, AlbumDetailView, Artist, PlaybackDisplay, Release, Track, TrackImportState};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetailMock() -> Element {
    // Playback state
    let mut playback_state = use_signal(|| PlaybackState::Stopped);
    let position_ms = use_signal(|| 45_000u64);

    // Import state
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    // Mock album data
    let album = Album {
        id: "album-1".to_string(),
        title: "Neon Frequencies".to_string(),
        year: Some(2023),
        cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        is_compilation: false,
    };

    let artists = vec![Artist {
        id: "artist-1".to_string(),
        name: "The Midnight Signal".to_string(),
    }];

    let releases = vec![
        Release {
            id: "release-1".to_string(),
            album_id: "album-1".to_string(),
            release_name: Some("CD Edition".to_string()),
            year: Some(2023),
            format: Some("CD".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001".to_string()),
            country: Some("US".to_string()),
            barcode: Some("123456789012".to_string()),
            discogs_release_id: Some("12345678".to_string()),
            musicbrainz_release_id: Some("abc-123".to_string()),
        },
        Release {
            id: "release-2".to_string(),
            album_id: "album-1".to_string(),
            release_name: Some("Digital Deluxe".to_string()),
            year: Some(2023),
            format: Some("Digital".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001D".to_string()),
            country: Some("XW".to_string()),
            barcode: None,
            discogs_release_id: None,
            musicbrainz_release_id: Some("def-456".to_string()),
        },
    ];

    let mut selected_release_id = use_signal(|| Some("release-1".to_string()));

    let tracks_data = [
        ("track-1", "Broadcast", 1, 198_000i64),
        ("track-2", "Static Dreams", 2, 245_000),
        ("track-3", "Frequency Drift", 3, 312_000),
        ("track-4", "Night Transmission", 4, 267_000),
        ("track-5", "Signal Lost", 5, 289_000),
        ("track-6", "Airwave", 6, 234_000),
        ("track-7", "Carrier Wave", 7, 301_000),
        ("track-8", "Sign Off", 8, 356_000),
    ];

    let tracks: Vec<Signal<Track>> = tracks_data
        .iter()
        .map(|(id, title, num, duration)| {
            Signal::new(Track {
                id: id.to_string(),
                title: title.to_string(),
                track_number: Some(*num),
                disc_number: Some(1),
                duration_ms: Some(*duration),
                is_available: true,
                import_state: TrackImportState::Complete,
            })
        })
        .collect();

    // Build playback display from state
    let playback = match playback_state() {
        PlaybackState::Stopped => PlaybackDisplay::Stopped,
        PlaybackState::Playing => PlaybackDisplay::Playing {
            track_id: "track-1".to_string(),
            position_ms: position_ms(),
            duration_ms: 198_000,
        },
        PlaybackState::Paused => PlaybackDisplay::Paused {
            track_id: "track-1".to_string(),
            position_ms: position_ms(),
            duration_ms: 198_000,
        },
        PlaybackState::Loading => PlaybackDisplay::Loading {
            track_id: "track-1".to_string(),
        },
    };

    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white",
            // Controls panel at top
            div { class: "sticky top-0 z-50 bg-gray-800 border-b border-gray-700 p-4",
                div { class: "max-w-6xl mx-auto",
                    h1 { class: "text-lg font-semibold text-white mb-3", "AlbumDetailView" }
                    div { class: "flex flex-wrap gap-2 mb-3",
                        for (state , label) in [
                            (PlaybackState::Stopped, "Stopped"),
                            (PlaybackState::Playing, "Playing"),
                            (PlaybackState::Paused, "Paused"),
                            (PlaybackState::Loading, "Loading"),
                        ]
                        {
                            button {
                                class: if playback_state() == state { "px-3 py-1.5 text-sm rounded bg-blue-600 text-white" } else { "px-3 py-1.5 text-sm rounded bg-gray-700 text-gray-300 hover:bg-gray-600" },
                                onclick: move |_| playback_state.set(state),
                                "{label}"
                            }
                        }
                    }
                    div { class: "flex flex-wrap gap-4 text-sm",
                        label { class: "flex items-center gap-2 text-gray-400",
                            "Release:"
                            select {
                                class: "bg-gray-700 rounded px-2 py-1 text-white",
                                onchange: move |e| selected_release_id.set(Some(e.value())),
                                option { value: "release-1", "CD Edition" }
                                option { value: "release-2", "Digital Deluxe" }
                            }
                        }
                    }
                }
            }

            // Component render area
            div { class: "max-w-6xl mx-auto p-6",
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    tracks,
                    selected_release_id: selected_release_id(),
                    import_progress,
                    import_error,
                    playback,
                    on_release_select: move |id| selected_release_id.set(Some(id)),
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
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Loading,
}
