//! Torrent import workflow wrapper - reads context and delegates to TorrentImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    confirm_and_start_import, lookup_discid, search_by_barcode, search_by_catalog_number,
    search_general, DiscIdLookupResult,
};
use bae_core::torrent::ffi::TorrentInfo as BaeTorrentInfo;
use bae_ui::components::import::{TorrentImportView, TrackerConnectionStatus, TrackerStatus};
use bae_ui::display_types::{
    SearchSource, SearchTab, TorrentFileInfo, TorrentInfo as DisplayTorrentInfo,
};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use bae_ui::TorrentInputMode;
use dioxus::prelude::*;
use std::path::PathBuf;
use tracing::{info, warn};

/// Convert bae TorrentInfo to display TorrentInfo
fn to_display_torrent_info(info: &BaeTorrentInfo) -> DisplayTorrentInfo {
    DisplayTorrentInfo {
        name: info.name.clone(),
        total_size: info.total_size,
        piece_length: info.piece_length,
        num_pieces: info.num_pieces,
        is_private: info.is_private,
        comment: info.comment.clone(),
        creator: info.creator.clone(),
        creation_date: info.creation_date,
        files: info
            .files
            .iter()
            .map(|f| TorrentFileInfo {
                path: f.path.clone(),
                size: f.size,
            })
            .collect(),
        trackers: info.trackers.clone(),
    }
}

/// Generate mock tracker statuses from tracker URLs
fn generate_tracker_statuses(trackers: &[String]) -> Vec<TrackerStatus> {
    trackers
        .iter()
        .enumerate()
        .map(|(idx, url)| {
            let peer_count = 15 + (idx * 7) % 35;
            let seeders = peer_count / 4;
            let leechers = peer_count - seeders;
            let status = if url.contains("udp") {
                TrackerConnectionStatus::Connected
            } else {
                let progress = idx % 3;
                if progress == 2 {
                    TrackerConnectionStatus::Connected
                } else {
                    TrackerConnectionStatus::Announcing
                }
            };
            TrackerStatus {
                url: url.clone(),
                status,
                peer_count,
                seeders,
                leechers,
            }
        })
        .collect()
}

#[component]
pub fn TorrentImport() -> Element {
    let app = use_app();
    let navigator = use_navigator();

    let is_dragging = use_signal(|| false);
    let mut input_mode = use_signal(|| TorrentInputMode::File);

    // Torrent info stored locally (not in ImportState since it's torrent-specific)
    let torrent_info_signal = use_signal(|| Option::<BaeTorrentInfo>::None);

    // Get import store for reads
    let import_store = app.state.import();
    let state = import_store.read();

    // Read from state
    let step = state.get_import_step();
    let identify_mode = state.get_identify_mode();
    let current_candidate_key = state.current_candidate_key.clone();
    let is_looking_up = state.is_looking_up;
    let import_error_message = state.import_error_message.clone();
    let duplicate_album_id = state.duplicate_album_id.clone();
    let folder_files = state.folder_files.clone();
    let search_state = state.get_search_state();
    let display_exact_candidates = state.get_exact_match_candidates();
    let display_confirmed = state.get_confirmed_candidate();
    let display_metadata = state.get_metadata();
    let discid_lookup_error = state.get_discid_lookup_error();
    let selected_match_index = state.get_selected_match_index();

    let has_searched = search_state
        .as_ref()
        .map(|s| s.has_searched)
        .unwrap_or(false);
    let error_message = search_state.as_ref().and_then(|s| s.error_message.clone());

    // Drop state borrow before handlers
    drop(state);

    // Prepare torrent display data
    let torrent_info_read = torrent_info_signal.read();
    let tracker_statuses = torrent_info_read
        .as_ref()
        .map(|info| generate_tracker_statuses(&info.trackers))
        .unwrap_or_default();
    let display_torrent_info = torrent_info_read.as_ref().map(to_display_torrent_info);
    let torrent_files = display_torrent_info
        .as_ref()
        .map(|info| info.files.clone())
        .unwrap_or_default();
    drop(torrent_info_read);

    // Check for cue files (for metadata detection prompt)
    let has_cue_files = folder_files
        .documents
        .iter()
        .any(|f| f.format.to_lowercase() == "cue" || f.format.to_lowercase() == "log");

    // Torrent metadata detection prompt - check if metadata is available from state machine
    let show_metadata_detection_prompt = has_cue_files && display_metadata.is_none();

    let display_manual_candidates = search_state
        .as_ref()
        .map(|s| s.search_results.clone())
        .unwrap_or_default();

    // Handlers
    let on_file_select = {
        let app = app.clone();
        let mut torrent_info_signal = torrent_info_signal;
        move |_| {
            let app = app.clone();
            spawn(async move {
                if let Some(file) = rfd::AsyncFileDialog::new()
                    .add_filter("Torrent files", &["torrent"])
                    .pick_file()
                    .await
                {
                    let path = PathBuf::from(file.path());
                    let path_str = path.to_string_lossy().to_string();

                    // Parse torrent file
                    match bae_core::torrent::parse_torrent_info(&path) {
                        Ok(info) => {
                            torrent_info_signal.set(Some(info));
                            app.state.import().write().switch_candidate(Some(path_str));
                        }
                        Err(e) => {
                            warn!("Failed to load torrent: {}", e);
                        }
                    }
                }
            });
        }
    };

    let on_magnet_submit = {
        move |_magnet: String| {
            info!("Magnet link selection not yet implemented");
        }
    };

    let on_clear = {
        let app = app.clone();
        let mut torrent_info_signal = torrent_info_signal;
        move |_| {
            app.state.import().write().reset();
            torrent_info_signal.set(None);
        }
    };

    let on_exact_match_select = {
        let app = app.clone();
        move |index: usize| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectExactMatch(index));
        }
    };

    // Manual search handler
    let perform_search = {
        let app = app.clone();
        move || {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();
                let search_state = import_store.read().get_search_state();
                let metadata = import_store.read().get_metadata();

                let Some(search_state) = search_state else {
                    return;
                };

                let tab = search_state.search_tab;
                let source = search_state.search_source;

                match tab {
                    SearchTab::General => {
                        let artist = search_state.search_artist.clone();
                        let album = search_state.search_album.clone();
                        let year = search_state.search_year.clone();
                        let label = search_state.search_label.clone();

                        if artist.trim().is_empty()
                            && album.trim().is_empty()
                            && year.trim().is_empty()
                            && label.trim().is_empty()
                        {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please fill in at least one field".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result =
                            search_general(metadata, source, artist, album, year, label).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                    SearchTab::CatalogNumber => {
                        let catno = search_state.search_catalog_number.clone();
                        if catno.trim().is_empty() {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please enter a catalog number".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result = search_by_catalog_number(metadata, source, catno).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                    SearchTab::Barcode => {
                        let barcode = search_state.search_barcode.clone();
                        if barcode.trim().is_empty() {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please enter a barcode".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result = search_by_barcode(metadata, source, barcode).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                }
            });
        }
    };

    let on_manual_match_select = {
        let app = app.clone();
        move |index: usize| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectSearchResult(index));
        }
    };

    let on_manual_confirm = {
        let app = app.clone();
        move |_candidate: bae_ui::display_types::MatchCandidate| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::ConfirmSearchResult);
        }
    };

    let on_retry_discid_lookup = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();

                import_store
                    .write()
                    .dispatch(CandidateEvent::RetryDiscIdLookup);
                import_store.write().is_looking_up = true;

                let mb_discid = import_store
                    .read()
                    .get_metadata()
                    .and_then(|m| m.mb_discid.clone());

                if let Some(mb_discid) = mb_discid {
                    info!("Retrying DiscID lookup...");
                    match lookup_discid(&mb_discid).await {
                        Ok(result) => {
                            let matches = match result {
                                DiscIdLookupResult::NoMatches => vec![],
                                DiscIdLookupResult::SingleMatch(c) => vec![*c],
                                DiscIdLookupResult::MultipleMatches(cs) => cs,
                            };
                            import_store.write().is_looking_up = false;
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches,
                                    error: None,
                                });
                        }
                        Err(e) => {
                            import_store.write().is_looking_up = false;
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches: vec![],
                                    error: Some(e),
                                });
                        }
                    }
                } else {
                    import_store.write().is_looking_up = false;
                }
            });
        }
    };

    let on_detect_metadata = {
        move |_| {
            info!("Torrent metadata detection not yet implemented in new architecture");
        }
    };

    // Confirmation handlers
    let on_edit = {
        let app = app.clone();
        move |_| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::GoBackToIdentify);
        }
    };

    let on_confirm = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            let navigator = navigator;
            spawn(async move {
                let confirmed = app.state.import().read().get_confirmed_candidate();
                if let Some(candidate) = confirmed {
                    if let Err(e) =
                        confirm_and_start_import(&app, candidate, ImportSource::Torrent, navigator)
                            .await
                    {
                        warn!("Failed to confirm and start import: {}", e);
                    }
                }
            });
        }
    };

    // Search field change handlers
    let on_search_source_change = {
        let app = app.clone();
        move |source: SearchSource| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SetSearchSource(source));
        }
    };

    let on_search_tab_change = {
        let app = app.clone();
        move |tab: SearchTab| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SetSearchTab(tab));
        }
    };

    let on_artist_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Artist,
                    value,
                });
        }
    };

    let on_album_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Album,
                    value,
                });
        }
    };

    let on_year_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Year,
                    value,
                });
        }
    };

    let on_label_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Label,
                    value,
                });
        }
    };

    let on_catalog_number_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::CatalogNumber,
                    value,
                });
        }
    };

    let on_barcode_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Barcode,
                    value,
                });
        }
    };

    rsx! {
        TorrentImportView {
            step,
            identify_mode,
            torrent_path: current_candidate_key.clone().unwrap_or_default(),
            torrent_info: display_torrent_info,
            tracker_statuses,
            torrent_files,
            input_mode: *input_mode.read(),
            is_dragging: *is_dragging.read(),
            on_mode_change: move |mode| input_mode.set(mode),
            on_file_select,
            on_magnet_submit,
            exact_match_candidates: display_exact_candidates,
            selected_match_index,
            on_exact_match_select,
            detected_metadata: display_metadata,
            search_source: search_state.as_ref().map(|s| s.search_source).unwrap_or(SearchSource::MusicBrainz),
            on_search_source_change,
            search_tab: search_state.as_ref().map(|s| s.search_tab).unwrap_or(SearchTab::General),
            on_search_tab_change,
            search_artist: search_state.as_ref().map(|s| s.search_artist.clone()).unwrap_or_default(),
            on_artist_change,
            search_album: search_state.as_ref().map(|s| s.search_album.clone()).unwrap_or_default(),
            on_album_change,
            search_year: search_state.as_ref().map(|s| s.search_year.clone()).unwrap_or_default(),
            on_year_change,
            search_label: search_state.as_ref().map(|s| s.search_label.clone()).unwrap_or_default(),
            on_label_change,
            search_catalog_number: search_state.as_ref().map(|s| s.search_catalog_number.clone()).unwrap_or_default(),
            on_catalog_number_change,
            search_barcode: search_state.as_ref().map(|s| s.search_barcode.clone()).unwrap_or_default(),
            on_barcode_change,
            is_searching: search_state.as_ref().map(|s| s.is_searching).unwrap_or(false),
            search_error: error_message,
            has_searched,
            manual_match_candidates: display_manual_candidates,
            on_manual_match_select,
            on_search: move |_| perform_search(),
            on_manual_confirm,
            discid_lookup_error,
            is_retrying_discid_lookup: is_looking_up,
            on_retry_discid_lookup,
            show_metadata_detection_prompt,
            on_detect_metadata,
            confirmed_candidate: display_confirmed,
            selected_cover: None,
            display_cover_url: None,
            artwork_files: Vec::new(),
            storage_profiles: Vec::new(),
            selected_profile_id: None,
            is_importing: false,
            preparing_step_text: None,
            on_select_remote_cover: |_| {},
            on_select_local_cover: |_| {},
            on_storage_profile_change: |_| {},
            on_edit,
            on_confirm,
            on_configure_storage: |_| {},
            on_clear,
            import_error: import_error_message,
            duplicate_album_id,
            on_view_duplicate: |_| {},
        }
    }
}
