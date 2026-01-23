//! Folder import workflow wrapper - reads context and delegates to FolderImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    confirm_and_start_import, load_selected_release, lookup_discid, search_by_barcode,
    search_by_catalog_number, search_general, DiscIdLookupResult,
};
use bae_ui::components::import::FolderImportView;
use bae_ui::display_types::{DetectedCandidate, DetectedCandidateStatus, SearchSource, SearchTab};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use dioxus::prelude::*;
use tracing::{info, warn};

#[component]
pub fn FolderImport() -> Element {
    let app = use_app();
    let navigator = use_navigator();

    let is_dragging = use_signal(|| false);

    // Get import store for reads
    let import_store = app.state.import();

    // Read state from the Store
    let state = import_store.read();
    let step = state.get_import_step();
    let identify_mode = state.get_identify_mode();
    let display_folder_files = state.folder_files.clone();
    let current_candidate_key = state.current_candidate_key.clone();
    let is_looking_up = state.is_looking_up;
    let is_scanning_candidates = state.is_scanning_candidates;
    let duplicate_album_id = state.duplicate_album_id.clone();
    let import_error_message = state.import_error_message.clone();
    let search_state = state.get_search_state();
    let display_exact_candidates = state.get_exact_match_candidates();
    let display_confirmed = state.get_confirmed_candidate();
    let display_metadata = state.get_metadata();
    let discid_lookup_error = state.get_discid_lookup_error();
    let selected_match_index = state.current_candidate_state().and_then(|s| match s {
        bae_ui::stores::import::CandidateState::Identifying(is) => is.selected_match_index,
        _ => None,
    });

    // Convert detected candidates to display type with status from state machine
    let display_detected_candidates: Vec<DetectedCandidate> = state
        .detected_candidates
        .iter()
        .map(|c| {
            let status = state
                .candidate_states
                .get(&c.path)
                .map(|s| {
                    if s.is_imported() {
                        DetectedCandidateStatus::Imported
                    } else if s.is_importing() {
                        DetectedCandidateStatus::Importing
                    } else {
                        DetectedCandidateStatus::Pending
                    }
                })
                .unwrap_or(DetectedCandidateStatus::Pending);
            DetectedCandidate {
                name: c.name.clone(),
                path: c.path.clone(),
                status,
            }
        })
        .collect();

    // Derive selected candidate index from current candidate key
    let selected_candidate_index: Option<usize> = current_candidate_key.as_ref().and_then(|key| {
        state
            .detected_candidates
            .iter()
            .position(|c| &c.path == key)
    });

    // Get search state values
    let has_searched = search_state
        .as_ref()
        .map(|s| s.has_searched)
        .unwrap_or(false);
    let error_message = search_state.as_ref().and_then(|s| s.error_message.clone());
    let display_manual_candidates = search_state
        .as_ref()
        .map(|s| s.search_results.clone())
        .unwrap_or_default();

    // Clone detected candidates for async use
    let detected_candidates_for_handlers = state.detected_candidates.clone();

    // Drop the state borrow before creating handlers
    drop(state);

    // Handlers
    let on_folder_select = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                if let Some(path) = rfd::AsyncFileDialog::new().pick_folder().await {
                    let path_str = path.path().to_string_lossy().to_string();
                    let import_handle = app.import_handle.clone();

                    // Clear existing candidates if this is the first folder
                    {
                        let mut import_store = app.state.import();
                        if import_store.read().detected_candidates.is_empty() {
                            import_store.write().reset();
                        }
                        import_store.write().is_scanning_candidates = true;
                    }

                    if let Err(e) =
                        import_handle.enqueue_folder_scan(std::path::PathBuf::from(path_str))
                    {
                        warn!("Failed to add folder to scan: {}", e);
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

    let on_reveal = {
        let key = current_candidate_key.clone();
        move |_| {
            if let Some(path) = key.clone() {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("explorer").arg(&path).spawn();
                }
            }
        }
    };

    let on_remove_release = {
        let app = app.clone();
        move |index: usize| {
            app.state.import().write().remove_detected_release(index);
        }
    };

    let on_clear_all_releases = {
        let app = app.clone();
        move |_| {
            let mut store = app.state.import();
            let mut state = store.write();
            state.detected_candidates.clear();
            state.candidate_states.clear();
            state.loading_candidates.clear();
            state.discid_lookup_attempted.clear();
            state.switch_candidate(None);
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

    let on_release_select = {
        let app = app.clone();
        let detected = detected_candidates_for_handlers.clone();
        move |index: usize| {
            let app = app.clone();
            let detected = detected.clone();
            spawn(async move {
                if let Err(e) = load_selected_release(&app, index, &detected).await {
                    warn!("Failed to switch to release: {}", e);
                }
            });
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
                    bae_ui::display_types::SearchTab::General => {
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
                    bae_ui::display_types::SearchTab::CatalogNumber => {
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
                    bae_ui::display_types::SearchTab::Barcode => {
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

                // Dispatch event to set mode to DiscIdLookup
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
                        confirm_and_start_import(&app, candidate, ImportSource::Folder, navigator)
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

    // Text file viewing
    let mut selected_text_file = use_signal(|| None::<String>);

    // Load text file contents when selected
    let folder_path = current_candidate_key.clone();
    let text_file_contents_resource = use_resource(move || {
        let folder = folder_path.clone().unwrap_or_default();
        let selected = selected_text_file.read().clone();
        async move {
            let name = selected?;
            let path = std::path::Path::new(&folder).join(&name);
            std::fs::read_to_string(&path).ok()
        }
    });

    let text_file_content = text_file_contents_resource.read().clone().unwrap_or(None);

    // Generate image URLs from artwork files
    let image_data: Vec<(String, String)> = display_folder_files
        .artwork
        .iter()
        .map(crate::ui::local_file_url::local_file_url)
        .collect();

    rsx! {
        FolderImportView {
            step,
            identify_mode,
            folder_path: current_candidate_key.clone().unwrap_or_default(),
            folder_files: display_folder_files,
            image_data,
            selected_text_file: selected_text_file.read().clone(),
            text_file_content,
            on_text_file_select: move |name| selected_text_file.set(Some(name)),
            on_text_file_close: move |_| selected_text_file.set(None),
            is_dragging: *is_dragging.read(),
            on_folder_select_click: on_folder_select,
            is_scanning_candidates,
            detected_candidates: display_detected_candidates,
            selected_candidate_index,
            on_release_select,
            on_skip_detection: |_| {},
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
            on_reveal,
            on_remove_release,
            on_clear_all_releases,
            import_error: import_error_message,
            duplicate_album_id,
            on_view_duplicate: |_| {},
        }
    }
}
