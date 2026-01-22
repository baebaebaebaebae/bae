//! CD import workflow wrapper - reads context and delegates to CdImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    confirm_and_start_import, lookup_discid, search_by_barcode, search_by_catalog_number,
    search_general, DiscIdLookupResult,
};
use bae_core::cd::CdDrive;
use bae_ui::components::import::{CdImportView, CdTocInfo};
use bae_ui::display_types::{CategorizedFileInfo, FolderMetadata, IdentifyMode};
use bae_ui::display_types::{CdDriveInfo, SearchSource, SearchTab};
use bae_ui::stores::import::{
    CandidateEvent, CandidateState, ConfirmPhase, ConfirmingState, IdentifyingState,
    ManualSearchState,
};
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use dioxus::prelude::*;
use std::path::PathBuf;
use tracing::{info, warn};

#[component]
pub fn CdImport() -> Element {
    let app = use_app();
    let navigator = use_navigator();

    // CD drive scanning state
    let is_scanning = use_signal(|| true);
    let drives = use_signal(Vec::<CdDriveInfo>::new);
    let mut selected_drive = use_signal(|| Option::<String>::None);

    // Scan for drives on mount
    use_effect({
        let mut is_scanning = is_scanning;
        let mut drives = drives;
        move || {
            spawn(async move {
                is_scanning.set(true);
                match CdDrive::detect_drives() {
                    Ok(drive_list) => {
                        let display_drives: Vec<CdDriveInfo> = drive_list
                            .iter()
                            .map(|d| CdDriveInfo {
                                device_path: d.device_path.to_string_lossy().to_string(),
                                name: d.name.clone(),
                            })
                            .collect();
                        drives.set(display_drives);
                    }
                    Err(e) => {
                        warn!("Failed to list CD drives: {}", e);
                    }
                }
                is_scanning.set(false);
            });
        }
    });

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
    let cd_toc_info = state.cd_toc_info.clone();
    let search_state = state.get_search_state();
    let display_exact_candidates = state.get_exact_match_candidates();
    let display_confirmed = state.get_confirmed_candidate();
    let discid_lookup_error = state.get_discid_lookup_error();
    let selected_match_index = state.get_selected_match_index();

    let has_searched = search_state
        .as_ref()
        .map(|s| s.has_searched)
        .unwrap_or(false);
    let error_message = search_state.as_ref().and_then(|s| s.error_message.clone());

    // Drop state borrow before handlers
    drop(state);

    // Prepare TOC info for view
    let toc_info = cd_toc_info
        .as_ref()
        .map(|(disc_id, first_track, last_track)| CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first_track,
            last_track: *last_track,
        });

    let display_manual_candidates = search_state
        .as_ref()
        .map(|s| s.search_results.clone())
        .unwrap_or_default();

    // Handlers
    let on_drive_select = {
        let app = app.clone();
        move |drive_path_str: String| {
            selected_drive.set(Some(drive_path_str.clone()));
            let app = app.clone();
            let drive_path = PathBuf::from(&drive_path_str);
            spawn(async move {
                let drive = CdDrive {
                    device_path: drive_path.clone(),
                    name: drive_path_str.clone(),
                };
                match drive.read_toc() {
                    Ok(toc) => {
                        let disc_id = toc.disc_id.clone();

                        // Update state with TOC info
                        {
                            let mut import_store = app.state.import();
                            import_store.write().cd_toc_info =
                                Some((disc_id.clone(), toc.first_track, toc.last_track));
                            import_store
                                .write()
                                .switch_candidate(Some(drive_path_str.clone()));
                            import_store
                                .write()
                                .loading_candidates
                                .insert(drive_path_str.clone(), true);
                            import_store.write().is_looking_up = true;
                        }

                        // Perform DiscID lookup
                        match lookup_discid(&disc_id).await {
                            Ok(result) => {
                                let mut import_store = app.state.import();
                                import_store.write().is_looking_up = false;
                                import_store
                                    .write()
                                    .loading_candidates
                                    .remove(&drive_path_str);

                                match result {
                                    DiscIdLookupResult::NoMatches => {
                                        // Initialize in ManualSearch mode
                                        let state = CandidateState::Identifying(IdentifyingState {
                                            files: CategorizedFileInfo::default(),
                                            metadata: FolderMetadata::default(),
                                            mode: IdentifyMode::ManualSearch,
                                            auto_matches: vec![],
                                            selected_match_index: None,
                                            search_state: ManualSearchState::default(),
                                            discid_lookup_error: None,
                                        });
                                        import_store
                                            .write()
                                            .candidate_states
                                            .insert(drive_path_str, state);
                                    }
                                    DiscIdLookupResult::SingleMatch(candidate) => {
                                        // Single match - go directly to Confirming
                                        let state =
                                            CandidateState::Confirming(Box::new(ConfirmingState {
                                                files: CategorizedFileInfo::default(),
                                                metadata: FolderMetadata::default(),
                                                confirmed_candidate: *candidate,
                                                selected_cover: None,
                                                selected_profile_id: None,
                                                phase: ConfirmPhase::Ready,
                                                auto_matches: vec![],
                                                search_state: ManualSearchState::default(),
                                            }));
                                        import_store
                                            .write()
                                            .candidate_states
                                            .insert(drive_path_str, state);
                                    }
                                    DiscIdLookupResult::MultipleMatches(candidates) => {
                                        // Multiple matches - MultipleExactMatches mode
                                        let state = CandidateState::Identifying(IdentifyingState {
                                            files: CategorizedFileInfo::default(),
                                            metadata: FolderMetadata::default(),
                                            mode: IdentifyMode::MultipleExactMatches,
                                            auto_matches: candidates,
                                            selected_match_index: None,
                                            search_state: ManualSearchState::default(),
                                            discid_lookup_error: None,
                                        });
                                        import_store
                                            .write()
                                            .candidate_states
                                            .insert(drive_path_str, state);
                                    }
                                }
                            }
                            Err(e) => {
                                let mut import_store = app.state.import();
                                import_store.write().is_looking_up = false;
                                import_store
                                    .write()
                                    .loading_candidates
                                    .remove(&drive_path_str);
                                import_store.write().import_error_message =
                                    Some(format!("DiscID lookup failed: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        let mut import_store = app.state.import();
                        import_store.write().is_looking_up = false;
                        import_store.write().import_error_message =
                            Some(format!("Failed to read CD TOC: {}", e));
                    }
                }
            });
        }
    };

    let on_clear = {
        let app = app.clone();
        move |_| {
            app.state.import().write().reset();
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

                        // CD imports don't use folder metadata
                        let result = search_general(None, source, artist, album, year, label).await;
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

                        let result = search_by_catalog_number(None, source, catno).await;
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

                        let result = search_by_barcode(None, source, barcode).await;
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
                let disc_id = import_store
                    .read()
                    .cd_toc_info
                    .as_ref()
                    .map(|(id, _, _)| id.clone());

                if let Some(disc_id) = disc_id {
                    import_store
                        .write()
                        .dispatch(CandidateEvent::RetryDiscIdLookup);
                    import_store.write().is_looking_up = true;

                    info!("Retrying DiscID lookup...");
                    match lookup_discid(&disc_id).await {
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
                }
            });
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
                        confirm_and_start_import(&app, candidate, ImportSource::Cd, navigator).await
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
        CdImportView {
            step,
            identify_mode,
            cd_path: current_candidate_key.clone().unwrap_or_default(),
            toc_info,
            is_scanning: *is_scanning.read(),
            drives: drives.read().clone(),
            selected_drive: selected_drive.read().clone(),
            on_drive_select,
            is_loading_exact_matches: is_looking_up,
            exact_match_candidates: display_exact_candidates,
            selected_match_index,
            on_exact_match_select,
            detected_metadata: None, // CD imports don't use folder metadata
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
