//! Import workflow helpers
//!
//! Standalone async functions that operate on AppService state for import operations.
//!
//! Organized into sub-modules:
//! - `conversion`: Type conversions between bae-core and bae-ui display types
//! - `search`: MusicBrainz + Discogs search orchestration, ranking, cover art checking
//! - `scan`: Folder scan event consumption and candidate detection

pub mod conversion;
pub mod scan;
pub mod search;

// Re-export public API used by consumers outside this module
pub use conversion::{
    count_local_audio_files, extract_tracks_from_discogs, extract_tracks_from_mb_response,
};
pub use scan::consume_scan_events;
pub use search::{
    build_caa_client, check_cover_art, get_discogs_client, search_by_barcode,
    search_by_catalog_number, search_general,
};

use crate::ui::app_service::AppService;
use bae_core::discogs::DiscogsRelease;
use bae_core::import::{
    cover_art, CoverSelection, ImportProgress, ImportRequest, MatchCandidate, MatchSource,
};
use bae_core::musicbrainz::{lookup_by_discid, ExternalUrls, MbRelease};
use bae_ui::display_types::{
    MatchCandidate as DisplayMatchCandidate, MatchSourceType, SelectedCover,
};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use conversion::{count_discogs_release_tracks, to_display_candidate};
use dioxus::prelude::*;
use std::path::PathBuf;
use tracing::{error, info, warn};

// ============================================================================
// DiscID lookup
// ============================================================================

pub enum DiscIdLookupResult {
    NoMatches,
    SingleMatch(Box<DisplayMatchCandidate>),
    MultipleMatches(Vec<DisplayMatchCandidate>),
}

/// Lookup a MusicBrainz release by DiscID.
pub async fn lookup_discid(
    mb_discid: &str,
    key_service: &bae_core::keys::KeyService,
) -> Result<DiscIdLookupResult, String> {
    info!("Looking up MB DiscID: {}", mb_discid);

    match lookup_by_discid(mb_discid).await {
        Ok((releases, external_urls)) => {
            Ok(handle_discid_lookup_result(releases, external_urls, key_service).await)
        }
        Err(e) => {
            info!("MB DiscID lookup failed: {}", e);
            Err(format!(
                "MusicBrainz lookup failed: {}. You can retry or search manually.",
                e,
            ))
        }
    }
}

/// Handle DiscID lookup result: process 0/1/multiple matches and return result
async fn handle_discid_lookup_result(
    releases: Vec<MbRelease>,
    external_urls: ExternalUrls,
    key_service: &bae_core::keys::KeyService,
) -> DiscIdLookupResult {
    if releases.is_empty() {
        info!("No exact matches found");
        return DiscIdLookupResult::NoMatches;
    }
    info!("Found {} exact matches", releases.len());

    // Get discogs client if available (for cover art fallback)
    let discogs_client = get_discogs_client(key_service).ok();
    let cover_art_futures: Vec<_> = releases
        .iter()
        .map(|mb_release| {
            cover_art::fetch_cover_art_for_mb_release(
                mb_release,
                &external_urls,
                None,
                discogs_client.as_ref(),
            )
        })
        .collect();
    let cover_art_urls: Vec<_> = futures::future::join_all(cover_art_futures).await;

    // Create core MatchCandidates then convert to display types
    let core_candidates: Vec<MatchCandidate> = releases
        .into_iter()
        .zip(cover_art_urls.into_iter())
        .map(|(mb_release, cover_art_url)| MatchCandidate {
            source: MatchSource::MusicBrainz(mb_release),
            confidence: 100.0,
            match_reasons: vec!["Exact DiscID match".to_string()],
            cover_art_url,
        })
        .collect();

    let display_candidates: Vec<DisplayMatchCandidate> =
        core_candidates.iter().map(to_display_candidate).collect();

    if display_candidates.len() == 1 {
        let mut candidate = display_candidates.into_iter().next().unwrap();
        // Single disc ID match — fetch full release for track listing.
        // No track count validation needed: disc ID inherently guarantees a match.
        if let Some(ref mb_id) = candidate.musicbrainz_release_id {
            if let Ok((response, _)) = fetch_mb_release_for_validation(mb_id).await {
                candidate.tracks = extract_tracks_from_mb_response(&response);
            }
        }
        DiscIdLookupResult::SingleMatch(Box::new(candidate))
    } else {
        DiscIdLookupResult::MultipleMatches(display_candidates)
    }
}

// ============================================================================
// Duplicate detection
// ============================================================================

/// Check candidates against the library by exact release ID.
pub async fn check_candidates_for_duplicates(
    app: &AppService,
    candidates: &mut [DisplayMatchCandidate],
) {
    let lm = app.library_manager.get();
    for candidate in candidates.iter_mut() {
        if let Some(id) = &candidate.musicbrainz_release_id {
            if let Ok(Some(dup)) = lm.find_duplicate_by_musicbrainz(Some(id), None).await {
                candidate.existing_album_id = Some(dup.id);
                continue;
            }
        }
        if let Some(id) = &candidate.discogs_release_id {
            if let Ok(Some(dup)) = lm.find_duplicate_by_discogs(None, Some(id)).await {
                candidate.existing_album_id = Some(dup.id);
            }
        }
    }
}

// ============================================================================
// Prefetch / track count validation
// ============================================================================

/// Fetch a full MusicBrainz release by ID and return the typed response + track count.
pub async fn fetch_mb_release_for_validation(
    release_id: &str,
) -> Result<(bae_core::musicbrainz::MbReleaseResponse, usize), String> {
    let (_release, _urls, response) =
        bae_core::retry::retry_with_backoff(3, "MusicBrainz release prefetch", || {
            bae_core::musicbrainz::lookup_release_by_id(release_id)
        })
        .await
        .map_err(|e| format!("Failed to fetch release: {}", e))?;
    let track_count = response.track_count();
    Ok((response, track_count))
}

/// Fetch a full Discogs release and return the release + track count.
pub async fn fetch_discogs_release_for_validation(
    release_id: &str,
    master_id: Option<&str>,
    key_service: &bae_core::keys::KeyService,
) -> Result<(bae_core::discogs::DiscogsRelease, usize), String> {
    let release = fetch_discogs_release(release_id, master_id, key_service).await?;
    let track_count = count_discogs_release_tracks(&release);
    Ok((release, track_count))
}

/// Fetch full Discogs release details for import
async fn fetch_discogs_release(
    release_id: &str,
    master_id: Option<&str>,
    key_service: &bae_core::keys::KeyService,
) -> Result<DiscogsRelease, String> {
    let client = get_discogs_client(key_service)?;
    match client.get_release(release_id).await {
        Ok(mut release) => {
            // Prefer the master_id from the search result (if any) over what
            // the release endpoint returned, since the search result is what
            // the user selected.
            if master_id.is_some() {
                release.master_id = master_id.map(|s| s.to_string());
            }
            Ok(release)
        }
        Err(e) => Err(format!("Failed to fetch release details: {}", e)),
    }
}

// ============================================================================
// Import confirmation
// ============================================================================

/// Confirm a match candidate and start the import workflow.
///
/// `pre_fetched_discogs`: If the prefetch already fetched the Discogs release,
/// pass it here to skip the redundant fetch during import.
pub async fn confirm_and_start_import(
    app: &AppService,
    candidate: DisplayMatchCandidate,
    import_source: ImportSource,
    pre_fetched_discogs: Option<DiscogsRelease>,
) -> Result<(), String> {
    let mut import_store = app.state.import();

    // Get candidate key early for completion tracking
    let candidate_key = {
        let state = import_store.read();
        state
            .current_candidate_key
            .clone()
            .ok_or_else(|| "No candidate selected for import".to_string())?
    };

    let import_id = uuid::Uuid::new_v4().to_string();

    // Get state from store
    let (storage_profile_id, metadata, selected_cover) = {
        let state = import_store.read();
        (
            state.get_storage_profile_id(),
            state.get_metadata(),
            state.get_selected_cover(),
        )
    };
    let master_year = metadata.as_ref().and_then(|m| m.year).unwrap_or(1970);

    let selected_cover = selected_cover.map(|c| match c {
        SelectedCover::Remote { url, .. } => CoverSelection::Remote(url),
        SelectedCover::Local { filename } => CoverSelection::Local(filename),
    });

    let request = match import_source {
        ImportSource::Folder => match candidate.source_type {
            MatchSourceType::Discogs => {
                let discogs_release = if let Some(pre_fetched) = pre_fetched_discogs {
                    pre_fetched
                } else {
                    let release_id = candidate
                        .discogs_release_id
                        .as_ref()
                        .ok_or_else(|| "Missing Discogs release ID".to_string())?;

                    fetch_discogs_release(
                        release_id,
                        candidate.discogs_master_id.as_deref(),
                        &app.key_service,
                    )
                    .await?
                };

                ImportRequest::Folder {
                    import_id: import_id.clone(),
                    discogs_release: Some(discogs_release),
                    mb_release: None,
                    folder: PathBuf::from(&candidate_key),
                    master_year,
                    storage_profile_id: storage_profile_id.clone(),
                    selected_cover: selected_cover.clone(),
                }
            }
            MatchSourceType::MusicBrainz => {
                let release_id = candidate
                    .musicbrainz_release_id
                    .as_ref()
                    .ok_or_else(|| "Missing MusicBrainz release ID".to_string())?;

                info!(
                    "Starting import for MusicBrainz release: {}",
                    candidate.title
                );

                // Only release_id, title, and artist are used downstream;
                // the full release is re-fetched in fetch_and_parse_mb_release.
                let mb_release = MbRelease {
                    release_id: release_id.clone(),
                    release_group_id: candidate
                        .musicbrainz_release_group_id
                        .clone()
                        .unwrap_or_default(),
                    title: candidate.title.clone(),
                    artist: candidate.artist.clone(),
                    date: None,
                    first_release_date: candidate.original_year.clone(),
                    format: candidate.format.clone(),
                    country: candidate.country.clone(),
                    label: candidate.label.clone(),
                    catalog_number: candidate.catalog_number.clone(),
                    barcode: None,
                    // The full release is re-fetched in fetch_and_parse_mb_release,
                    // which reads is_compilation from the API response.
                    is_compilation: false,
                };

                ImportRequest::Folder {
                    import_id: import_id.clone(),
                    discogs_release: None,
                    mb_release: Some(mb_release),
                    folder: PathBuf::from(&candidate_key),
                    master_year,
                    storage_profile_id: storage_profile_id.clone(),
                    selected_cover: selected_cover.clone(),
                }
            }
        },
        _ => return Err("This import source is not yet supported".to_string()),
    };

    let import_handle = app.import_handle.clone();
    match import_handle.send_request(request).await {
        Ok((album_id, _release_id)) => {
            info!("Import started successfully: {}", album_id);
            import_store.write().dispatch(CandidateEvent::ImportStarted);

            // Spawn a task to listen for import completion
            let progress_handle = import_handle.progress_handle.clone();
            let mut import_store_clone = app.state.import();
            let album_id_for_completion = album_id.clone();
            spawn(async move {
                let mut progress_rx = progress_handle.subscribe_import(import_id.clone());
                while let Some(event) = progress_rx.recv().await {
                    match event {
                        ImportProgress::Complete { .. } => {
                            info!("Import completed for candidate: {}", candidate_key);
                            import_store_clone.write().dispatch_to_candidate(
                                &candidate_key,
                                CandidateEvent::ImportCompleted(album_id_for_completion.clone()),
                            );
                            break;
                        }
                        ImportProgress::Failed { error, .. } => {
                            warn!("Import failed for candidate {}: {}", candidate_key, error);
                            import_store_clone.write().dispatch_to_candidate(
                                &candidate_key,
                                CandidateEvent::ImportFailed(error),
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            });

            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Failed to start import: {}", e);
            error!("{}", error_msg);
            import_store
                .write()
                .dispatch(CandidateEvent::ImportFailed(error_msg.clone()));
            Err(error_msg)
        }
    }
}

// ============================================================================
// Navigation helpers
// ============================================================================

/// Load a selected release by index, performing DiscID lookup if needed
pub async fn load_selected_release(
    app: &AppService,
    release_index: usize,
    detected_candidates: &[bae_ui::display_types::DetectedCandidate],
) -> Result<(), String> {
    let mut import_store = app.state.import();

    let release = detected_candidates
        .get(release_index)
        .ok_or_else(|| format!("Invalid release index: {}", release_index))?;
    let release_path = release.path.clone();

    // Switch to this candidate
    {
        let mut state = import_store.write();
        state.switch_candidate(Some(release_path.clone()));
        state.current_release_index = release_index;
    }

    // Only perform DiscID lookup once per candidate
    let already_attempted = import_store
        .read()
        .discid_lookup_attempted
        .contains(&release_path);
    if already_attempted {
        return Ok(());
    }

    import_store
        .write()
        .discid_lookup_attempted
        .insert(release_path.clone());

    let mb_discid = import_store
        .read()
        .get_metadata()
        .and_then(|m| m.mb_discid.clone());

    if let Some(mb_discid) = mb_discid {
        import_store.write().dispatch_to_candidate(
            &release_path,
            CandidateEvent::StartDiscIdLookup(mb_discid.clone()),
        );
        let result = lookup_discid(&mb_discid, &app.key_service).await;

        let event = match result {
            Ok(DiscIdLookupResult::NoMatches) => CandidateEvent::DiscIdLookupComplete {
                matches: vec![],
                error: None,
            },
            Ok(DiscIdLookupResult::SingleMatch(candidate)) => {
                let mut matches = vec![*candidate];
                check_candidates_for_duplicates(app, &mut matches).await;
                CandidateEvent::DiscIdLookupComplete {
                    matches,
                    error: None,
                }
            }
            Ok(DiscIdLookupResult::MultipleMatches(mut candidates)) => {
                check_candidates_for_duplicates(app, &mut candidates).await;
                CandidateEvent::DiscIdLookupComplete {
                    matches: candidates,
                    error: None,
                }
            }
            Err(error) => CandidateEvent::DiscIdLookupComplete {
                matches: vec![],
                error: Some(error),
            },
        };
        import_store
            .write()
            .dispatch_to_candidate(&release_path, event);
    } else {
        import_store.write().dispatch_to_candidate(
            &release_path,
            CandidateEvent::DiscIdLookupComplete {
                matches: vec![],
                error: None,
            },
        );
    }

    Ok(())
}

/// Check if there is unclean state for the current import source
pub fn has_unclean_state(app: &AppService) -> bool {
    let import_store = app.state.import();
    let state = import_store.read();
    match state.selected_import_source {
        ImportSource::Folder => !state.detected_candidates.is_empty(),
        ImportSource::Torrent => false, // TODO: implement torrent state check
        ImportSource::Cd => state.current_candidate_key.is_some(),
    }
}
