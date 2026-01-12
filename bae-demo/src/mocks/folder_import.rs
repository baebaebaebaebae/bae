//! FolderImportView mock with phase controls

use super::mock_header::MockHeader;
use bae_ui::{
    ArtworkFile, AudioContentInfo, CategorizedFileInfo, DetectedRelease, FileInfo,
    FolderImportView, FolderMetadata, ImportPhase, MatchCandidate, MatchSourceType, SearchSource,
    SearchTab, SelectedCover, StorageProfileInfo,
};
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn FolderImportMock() -> Element {
    // Phase control
    let mut phase = use_signal(|| ImportPhase::FolderSelection);

    // Folder selection state
    let mut is_dragging = use_signal(|| false);

    // Release selection state
    let mut selected_release_indices = use_signal(Vec::<usize>::new);

    // Loading states
    let mut is_detecting_metadata = use_signal(|| false);
    let mut is_loading_exact_matches = use_signal(|| false);
    let mut is_retrying_discid_lookup = use_signal(|| false);
    let mut selected_match_index = use_signal(|| None::<usize>);

    // Manual search state
    let mut search_source = use_signal(|| SearchSource::MusicBrainz);
    let mut search_tab = use_signal(|| SearchTab::General);
    let mut search_artist = use_signal(|| "The Midnight Signal".to_string());
    let mut search_album = use_signal(|| "Neon Frequencies".to_string());
    let mut search_year = use_signal(String::new);
    let mut search_label = use_signal(String::new);
    let mut search_catalog_number = use_signal(String::new);
    let mut search_barcode = use_signal(String::new);
    let mut is_searching = use_signal(|| false);
    let mut has_searched = use_signal(|| false);

    // Confirmation state
    let mut selected_cover = use_signal(|| None::<SelectedCover>);
    let mut selected_profile_id = use_signal(|| Some("profile-1".to_string()));
    let mut is_importing = use_signal(|| false);

    // Error state
    let mut show_error = use_signal(|| false);
    let mut show_discid_error = use_signal(|| false);

    // Mock data
    let folder_path = "/Users/demo/Music/The Midnight Signal - Neon Frequencies (2023)".to_string();

    let folder_files = CategorizedFileInfo {
        audio: AudioContentInfo::TrackFiles(vec![
            FileInfo {
                name: "01 - Broadcast.flac".to_string(),
                size: 32_000_000,
                format: "FLAC".to_string(),
            },
            FileInfo {
                name: "02 - Static Dreams.flac".to_string(),
                size: 28_500_000,
                format: "FLAC".to_string(),
            },
            FileInfo {
                name: "03 - Frequency Drift.flac".to_string(),
                size: 35_200_000,
                format: "FLAC".to_string(),
            },
            FileInfo {
                name: "04 - Night Transmission.flac".to_string(),
                size: 29_800_000,
                format: "FLAC".to_string(),
            },
            FileInfo {
                name: "05 - Signal Lost.flac".to_string(),
                size: 31_400_000,
                format: "FLAC".to_string(),
            },
        ]),
        artwork: vec![
            FileInfo {
                name: "cover.jpg".to_string(),
                size: 2_500_000,
                format: "JPEG".to_string(),
            },
            FileInfo {
                name: "back.jpg".to_string(),
                size: 1_800_000,
                format: "JPEG".to_string(),
            },
        ],
        documents: vec![FileInfo {
            name: "rip.log".to_string(),
            size: 4_500,
            format: "LOG".to_string(),
        }],
        other: vec![],
    };

    let detected_releases = vec![
        DetectedRelease {
            name: "CD1".to_string(),
            path: "/Users/demo/Music/Album/CD1".to_string(),
        },
        DetectedRelease {
            name: "CD2".to_string(),
            path: "/Users/demo/Music/Album/CD2".to_string(),
        },
    ];

    let exact_match_candidates = vec![
        MatchCandidate {
            title: "Neon Frequencies".to_string(),
            artist: "The Midnight Signal".to_string(),
            year: Some("2023".to_string()),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
            format: Some("CD".to_string()),
            country: Some("US".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001".to_string()),
            source_type: MatchSourceType::MusicBrainz,
            original_year: Some("2023".to_string()),
        },
        MatchCandidate {
            title: "Neon Frequencies (Deluxe)".to_string(),
            artist: "The Midnight Signal".to_string(),
            year: Some("2023".to_string()),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
            format: Some("Digital".to_string()),
            country: Some("XW".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001D".to_string()),
            source_type: MatchSourceType::MusicBrainz,
            original_year: Some("2023".to_string()),
        },
    ];

    let manual_match_candidates = if has_searched() {
        exact_match_candidates.clone()
    } else {
        vec![]
    };

    let confirmed_candidate = exact_match_candidates.first().cloned();

    let detected_metadata = Some(FolderMetadata {
        artist: Some("The Midnight Signal".to_string()),
        album: Some("Neon Frequencies".to_string()),
        year: Some(2023),
        track_count: Some(5),
        discid: None,
        confidence: 0.85,
        folder_tokens: vec![
            "midnight".to_string(),
            "signal".to_string(),
            "neon".to_string(),
            "frequencies".to_string(),
        ],
    });

    let artwork_files = vec![
        ArtworkFile {
            name: "cover.jpg".to_string(),
            display_url: "/covers/the-midnight-signal_neon-frequencies.png".to_string(),
        },
        ArtworkFile {
            name: "back.jpg".to_string(),
            display_url: "/covers/velvet-mathematics_proof-by-induction.png".to_string(),
        },
    ];

    let storage_profiles = vec![
        StorageProfileInfo {
            id: "profile-1".to_string(),
            name: "Cloud Storage".to_string(),
            is_default: true,
        },
        StorageProfileInfo {
            id: "profile-2".to_string(),
            name: "Local Backup".to_string(),
            is_default: false,
        },
    ];

    let import_error = if show_error() {
        Some("Failed to import: Network timeout".to_string())
    } else {
        None
    };
    let discid_lookup_error = if show_discid_error() {
        Some("DiscID lookup failed: No matching release found".to_string())
    } else {
        None
    };

    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white",
            // Controls panel at top
            div { class: "sticky top-0 z-50 bg-gray-800 border-b border-gray-700 p-4",
                div { class: "max-w-4xl mx-auto",
                    MockHeader { title: "FolderImportView".to_string() }
                    div { class: "flex flex-wrap gap-2 mb-3",
                        for (phase_option , label) in [
                            (ImportPhase::FolderSelection, "Folder Selection"),
                            (ImportPhase::ReleaseSelection, "Release Selection"),
                            (ImportPhase::MetadataDetection, "Metadata Detection"),
                            (ImportPhase::ExactLookup, "Exact Lookup"),
                            (ImportPhase::ManualSearch, "Manual Search"),
                            (ImportPhase::Confirmation, "Confirmation"),
                        ]
                        {
                            button {
                                class: if phase() == phase_option { "px-3 py-1.5 text-sm rounded bg-blue-600 text-white" } else { "px-3 py-1.5 text-sm rounded bg-gray-700 text-gray-300 hover:bg-gray-600" },
                                onclick: move |_| phase.set(phase_option),
                                "{label}"
                            }
                        }
                    }
                    div { class: "flex flex-wrap gap-4 text-sm",
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_dragging(),
                                onchange: move |e| is_dragging.set(e.checked()),
                            }
                            "Dragging"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_detecting_metadata(),
                                onchange: move |e| is_detecting_metadata.set(e.checked()),
                            }
                            "Detecting Metadata"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_loading_exact_matches(),
                                onchange: move |e| is_loading_exact_matches.set(e.checked()),
                            }
                            "Loading Exact Matches"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_retrying_discid_lookup(),
                                onchange: move |e| is_retrying_discid_lookup.set(e.checked()),
                            }
                            "Retrying DiscID"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_searching(),
                                onchange: move |e| is_searching.set(e.checked()),
                            }
                            "Searching"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: has_searched(),
                                onchange: move |e| has_searched.set(e.checked()),
                            }
                            "Has Results"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: is_importing(),
                                onchange: move |e| is_importing.set(e.checked()),
                            }
                            "Importing"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: show_error(),
                                onchange: move |e| show_error.set(e.checked()),
                            }
                            "Error"
                        }
                        label { class: "flex items-center gap-2 text-gray-400",
                            input {
                                r#type: "checkbox",
                                checked: show_discid_error(),
                                onchange: move |e| show_discid_error.set(e.checked()),
                            }
                            "DiscID Error"
                        }
                    }
                }
            }

            // Component render area
            div { class: "max-w-4xl mx-auto p-6",
                FolderImportView {
                    phase: phase(),
                    folder_path: folder_path.clone(),
                    folder_files: folder_files.clone(),
                    image_data: vec![
                        (
                            "cover.jpg".to_string(),
                            "/covers/the-midnight-signal_neon-frequencies.png".to_string(),
                        ),
                        (
                            "back.jpg".to_string(),
                            "/covers/velvet-mathematics_proof-by-induction.png".to_string(),
                        ),
                    ],
                    text_file_contents: HashMap::new(),
                    is_dragging: is_dragging(),
                    on_folder_select_click: |_| {},
                    detected_releases: detected_releases.clone(),
                    selected_release_indices: selected_release_indices(),
                    on_release_selection_change: move |indices| selected_release_indices.set(indices),
                    on_releases_import: |_| {},
                    is_detecting_metadata: is_detecting_metadata(),
                    on_skip_detection: |_| {},
                    is_loading_exact_matches: is_loading_exact_matches(),
                    exact_match_candidates: exact_match_candidates.clone(),
                    selected_match_index: selected_match_index(),
                    on_exact_match_select: move |idx| selected_match_index.set(Some(idx)),
                    detected_metadata: detected_metadata.clone(),
                    search_source: search_source(),
                    on_search_source_change: move |src| search_source.set(src),
                    search_tab: search_tab(),
                    on_search_tab_change: move |tab| search_tab.set(tab),
                    search_artist: search_artist(),
                    on_artist_change: move |v| search_artist.set(v),
                    search_album: search_album(),
                    on_album_change: move |v| search_album.set(v),
                    search_year: search_year(),
                    on_year_change: move |v| search_year.set(v),
                    search_label: search_label(),
                    on_label_change: move |v| search_label.set(v),
                    search_catalog_number: search_catalog_number(),
                    on_catalog_number_change: move |v| search_catalog_number.set(v),
                    search_barcode: search_barcode(),
                    on_barcode_change: move |v| search_barcode.set(v),
                    is_searching: is_searching(),
                    search_error: None,
                    has_searched: has_searched(),
                    manual_match_candidates,
                    on_manual_match_select: move |idx| selected_match_index.set(Some(idx)),
                    on_search: move |_| is_searching.set(true),
                    on_manual_confirm: |_| {},
                    discid_lookup_error,
                    is_retrying_discid_lookup: is_retrying_discid_lookup(),
                    on_retry_discid_lookup: |_| {},
                    confirmed_candidate,
                    selected_cover: selected_cover(),
                    display_cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
                    artwork_files,
                    storage_profiles,
                    selected_profile_id: selected_profile_id(),
                    is_importing: is_importing(),
                    preparing_step_text: if is_importing() { Some("Encoding tracks...".to_string()) } else { None },
                    on_select_remote_cover: move |url| {
                        selected_cover
                            .set(
                                Some(SelectedCover::Remote {
                                    url,
                                    source: "MusicBrainz".to_string(),
                                }),
                            )
                    },
                    on_select_local_cover: move |filename| selected_cover.set(Some(SelectedCover::Local { filename })),
                    on_storage_profile_change: move |id| selected_profile_id.set(id),
                    on_edit: |_| {},
                    on_confirm: |_| {},
                    on_configure_storage: |_| {},
                    on_clear: move |_| phase.set(ImportPhase::FolderSelection),
                    import_error,
                    duplicate_album_id: None,
                    on_view_duplicate: |_| {},
                }
            }
        }
    }
}
