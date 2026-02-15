//! Torrent import workflow wrapper - reads context and delegates to TorrentImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    build_caa_client, check_candidates_for_duplicates, check_cover_art, confirm_and_start_import,
    lookup_discid, search_by_barcode, search_by_catalog_number, search_general, DiscIdLookupResult,
};
use bae_core::torrent::ffi::TorrentInfo as BaeTorrentInfo;
use bae_ui::components::import::{TorrentImportView, TrackerConnectionStatus, TrackerStatus};
use bae_ui::display_types::{
    MatchCandidate, SearchSource, SearchTab, TorrentFileInfo, TorrentInfo as DisplayTorrentInfo,
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

    // Torrent-specific local state
    let is_dragging = use_signal(|| false);
    let mut input_mode = use_signal(|| TorrentInputMode::File);
    let torrent_info_signal = use_signal(|| Option::<BaeTorrentInfo>::None);

    // Get lenses for reactive props
    let import_state = app.state.import();
    // Prepare torrent display data from local signal
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

    let on_confirm_exact_match = {
        let app = app.clone();
        move |_: MatchCandidate| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::ConfirmExactMatch);
        }
    };

    let on_switch_to_manual_search = {
        let app = app.clone();
        move |_| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SwitchToManualSearch);
        }
    };

    let on_switch_to_exact_matches = {
        let app = app.clone();
        move |disc_id: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SwitchToMultipleExactMatches(disc_id));
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

                        let result = search_general(
                            metadata,
                            source,
                            artist,
                            album,
                            year,
                            label,
                            &app.key_service,
                        )
                        .await;
                        match result {
                            Ok(mut candidates) => {
                                check_candidates_for_duplicates(&app, &mut candidates).await;
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

                        let result =
                            search_by_catalog_number(metadata, source, catno, &app.key_service)
                                .await;
                        match result {
                            Ok(mut candidates) => {
                                check_candidates_for_duplicates(&app, &mut candidates).await;
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

                        let result =
                            search_by_barcode(metadata, source, barcode, &app.key_service).await;
                        match result {
                            Ok(mut candidates) => {
                                check_candidates_for_duplicates(&app, &mut candidates).await;
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

    // Cancel search handler
    let cancel_search = {
        let app = app.clone();
        move || {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::CancelSearch);
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

    let on_retry_cover = {
        let app = app.clone();
        move |index: usize| {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();

                let mb_release_id = {
                    let state = import_store.read();
                    state.get_search_state().and_then(|s| {
                        s.current_tab_state()
                            .search_results
                            .get(index)
                            .and_then(|r| r.musicbrainz_release_id.clone())
                    })
                };

                if let Some(release_id) = mb_release_id {
                    let client = build_caa_client();
                    let (cover_url, failed) = check_cover_art(&client, &release_id).await;

                    import_store
                        .write()
                        .dispatch(CandidateEvent::UpdateSearchResultCover {
                            index,
                            cover_url,
                            cover_fetch_failed: failed,
                        });
                }
            });
        }
    };

    let on_retry_discid_lookup = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();

                let mb_discid = import_store
                    .read()
                    .get_metadata()
                    .and_then(|m| m.mb_discid.clone());

                if let Some(mb_discid) = mb_discid {
                    import_store
                        .write()
                        .dispatch(CandidateEvent::StartDiscIdLookup(mb_discid.clone()));

                    info!("Retrying DiscID lookup...");
                    match lookup_discid(&mb_discid, &app.key_service).await {
                        Ok(result) => {
                            let mut matches = match result {
                                DiscIdLookupResult::NoMatches => vec![],
                                DiscIdLookupResult::SingleMatch(c) => vec![*c],
                                DiscIdLookupResult::MultipleMatches(cs) => cs,
                            };
                            check_candidates_for_duplicates(&app, &mut matches).await;
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches,
                                    error: None,
                                });
                        }
                        Err(e) => {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches: vec![],
                                    error: Some(e),
                                });
                        }
                    }
                }
            });
        }
    };

    let on_detect_metadata = {
        move |_| {
            info!("Torrent metadata detection not yet implemented in new architecture");
        }
    };

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
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::StartImport);
            spawn(async move {
                let confirmed = app.state.import().read().get_confirmed_candidate();
                if let Some(candidate) = confirmed {
                    if let Err(e) =
                        confirm_and_start_import(&app, candidate, ImportSource::Torrent, None).await
                    {
                        warn!("Failed to confirm and start import: {}", e);
                        app.state
                            .import()
                            .write()
                            .dispatch(CandidateEvent::ImportFailed(e));
                    }
                } else {
                    warn!("No confirmed candidate after StartImport");
                    app.state
                        .import()
                        .write()
                        .dispatch(CandidateEvent::ImportFailed(
                            "No candidate selected".to_string(),
                        ));
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
            // Pass the state lens
            state: import_state,
            // Torrent-specific state
            torrent_info: display_torrent_info,
            tracker_statuses,
            torrent_files,
            input_mode: *input_mode.read(),
            is_dragging: *is_dragging.read(),
            on_mode_change: move |mode| input_mode.set(mode),
            on_file_select,
            on_magnet_submit,
            // Callbacks
            on_exact_match_select,
            on_confirm_exact_match,
            on_switch_to_manual_search,
            on_switch_to_exact_matches,
            on_search_source_change,
            on_search_tab_change,
            on_artist_change,
            on_album_change,
            on_catalog_number_change,
            on_barcode_change,
            on_manual_match_select,
            on_search: move |_| perform_search(),
            on_cancel_search: move |_| cancel_search(),
            on_manual_confirm,
            on_retry_cover,
            on_retry_discid_lookup,
            on_detect_metadata,
            on_select_cover: |_| {},
            on_managed_change: |_| {},
            on_edit,
            on_confirm,
            on_clear,
            on_view_in_library: move |album_id: String| {
                navigator
                    .push(crate::ui::Route::AlbumDetail {
                        album_id,
                        release_id: String::new(),
                    });
            },
        }
    }
}
