//! Folder import workflow wrapper - reads context and delegates to FolderImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    build_caa_client, check_candidates_for_duplicates, check_cover_art, confirm_and_start_import,
    lookup_discid, search_by_barcode, search_by_catalog_number, search_general, DiscIdLookupResult,
};
use crate::ui::Route;
use bae_ui::components::import::FolderImportView;
use bae_ui::display_types::{MatchCandidate, SearchSource, SearchTab, SelectedCover};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::import::ImportStateStoreExt;
use bae_ui::stores::{AppStateStoreExt, StorageProfilesStateStoreExt};
use bae_ui::ImportSource;
use dioxus::prelude::*;
use tracing::{info, warn};

// ============================================================================
// Main Content Component
// ============================================================================

#[component]
pub fn FolderImport() -> Element {
    let app = use_app();
    let navigator = use_navigator();

    // Get lenses for reactive props - pass directly for granular reactivity
    let import_state = app.state.import();
    let storage_profiles = app.state.storage_profiles().profiles();

    // Extract values needed by handlers (handlers need current values, not lenses)
    let current_candidate_key = import_state.current_candidate_key().read().clone();

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
                    import_store.write().is_looking_up = true;

                    info!("Retrying DiscID lookup...");
                    match lookup_discid(&mb_discid).await {
                        Ok(result) => {
                            let mut matches = match result {
                                DiscIdLookupResult::NoMatches => vec![],
                                DiscIdLookupResult::SingleMatch(c) => vec![*c],
                                DiscIdLookupResult::MultipleMatches(cs) => cs,
                            };
                            check_candidates_for_duplicates(&app, &mut matches).await;
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
            spawn(async move {
                let confirmed = app.state.import().read().get_confirmed_candidate();
                if let Some(candidate) = confirmed {
                    if let Err(e) =
                        confirm_and_start_import(&app, candidate, ImportSource::Folder).await
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

    // Skip detection - go directly to manual search
    let on_skip_detection = {
        let app = app.clone();
        move |_| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SwitchToManualSearch);
        }
    };

    // Retry cover art for a search result
    let on_retry_cover = {
        let app = app.clone();
        move |index: usize| {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();

                // Get the MB release ID from the search result
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

    // Cover selection handler
    let on_select_cover = {
        let app = app.clone();
        move |cover: SelectedCover| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectCover(Some(cover)));
        }
    };

    // Storage profile change
    let on_storage_profile_change = {
        let app = app.clone();
        move |profile_id: Option<String>| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectStorageProfile(profile_id));
        }
    };

    // Configure storage - navigate to settings
    let on_configure_storage = move |_| {
        navigator.push(Route::Settings {});
    };

    // View album in library
    let on_view_in_library = move |album_id: String| {
        navigator.push(Route::AlbumDetail {
            album_id,
            release_id: String::new(),
        });
    };

    // Gallery lightbox viewing index (None = closed)
    let mut viewing_index = use_signal(|| None::<usize>);

    // Load text file contents when the viewed gallery item is a document.
    // Gallery ordering: artwork first, then documents.
    let folder_path = current_candidate_key.clone();
    let text_file_contents_resource = use_resource(move || {
        let idx = *viewing_index.read();
        let folder = folder_path.clone().unwrap_or_default();
        let key = import_state.current_candidate_key().read().clone();
        let states = import_state.candidate_states().read().clone();
        let files = key
            .as_ref()
            .and_then(|k| states.get(k))
            .map(|s| s.files().clone());
        async move {
            let idx = idx?;
            let files = files?;
            let artwork_count = files.artwork.len();
            let doc = files.documents.get(idx.checked_sub(artwork_count)?)?;
            let path = std::path::Path::new(&folder).join(&doc.name);
            Some(bae_core::text_encoding::read_text_file(&path).map_err(|e| e.to_string()))
        }
    });

    let text_file_content = text_file_contents_resource.read().clone().unwrap_or(None);

    rsx! {
        FolderImportView {
            state: import_state,
            viewing_index: ReadSignal::from(viewing_index),
            text_file_content,
            storage_profiles,
            on_folder_select_click: on_folder_select,
            on_view_change: move |idx| viewing_index.set(idx),
            on_skip_detection,
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
            on_select_cover,
            on_storage_profile_change,
            on_edit,
            on_confirm,
            on_configure_storage,
            on_view_in_library,
        }
    }
}
