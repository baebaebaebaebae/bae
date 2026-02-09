//! Import workflow helpers
//!
//! Standalone async functions that operate on AppService state for import operations.
//! These replace the methods from ImportContext.

use crate::ui::app_service::AppService;
use bae_core::discogs::client::DiscogsSearchParams;
use bae_core::discogs::{DiscogsClient, DiscogsRelease};
use bae_core::import::{
    cover_art, detect_folder_contents, CoverSelection, DetectedCandidate as CoreDetectedCandidate,
    ImportProgress, ImportRequest, MatchCandidate, MatchSource, ScanEvent,
};
use bae_core::keys::KeyService;
use bae_core::musicbrainz::{
    lookup_by_discid, search_releases_with_params, ExternalUrls, MbRelease, ReleaseSearchParams,
};
use bae_ui::display_types::{
    AudioContentInfo, CategorizedFileInfo, FolderMetadata as DisplayFolderMetadata,
    MatchCandidate as DisplayMatchCandidate, MatchSourceType, SearchSource, SelectedCover,
};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use dioxus::prelude::*;
use reqwest::redirect;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

// ============================================================================
// Conversion helpers (from old import_context/state.rs)
// ============================================================================

/// Convert bae-core FolderMetadata to display type
pub fn to_display_metadata(m: &bae_core::import::FolderMetadata) -> DisplayFolderMetadata {
    DisplayFolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: bae_core::musicbrainz::extract_search_tokens(m),
    }
}

/// Convert display FolderMetadata back to core type (for ranking functions)
pub fn from_display_metadata(m: &DisplayFolderMetadata) -> bae_core::import::FolderMetadata {
    bae_core::import::FolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: m.folder_tokens.clone(),
    }
}

/// Convert bae-core MatchCandidate to display type
pub fn to_display_candidate(candidate: &MatchCandidate) -> DisplayMatchCandidate {
    let (
        source_type,
        format,
        country,
        label,
        catalog_number,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
    ) = match &candidate.source {
        MatchSource::MusicBrainz(release) => (
            MatchSourceType::MusicBrainz,
            release.format.clone(),
            release.country.clone(),
            release.label.clone(),
            release.catalog_number.clone(),
            release.first_release_date.clone(),
            Some(release.release_id.clone()),
            Some(release.release_group_id.clone()),
            None,
            None,
        ),
        MatchSource::Discogs(result) => (
            MatchSourceType::Discogs,
            result.format.as_ref().map(|v| v.join(", ")),
            result.country.clone(),
            result.label.as_ref().and_then(|v| v.first().cloned()),
            result.catno.clone(),
            None,
            None,
            None,
            Some(result.id.to_string()),
            result.master_id.map(|id| id.to_string()),
        ),
    };

    DisplayMatchCandidate {
        title: candidate.title(),
        artist: match &candidate.source {
            MatchSource::MusicBrainz(r) => r.artist.clone(),
            MatchSource::Discogs(r) => r.title.split(" - ").next().unwrap_or("").to_string(),
        },
        year: candidate.year(),
        cover_url: candidate.cover_art_url(),
        cover_fetch_failed: false,
        format,
        country,
        label,
        catalog_number,
        source_type,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
        existing_album_id: None,
    }
}

// ============================================================================
// Discogs client helper
// ============================================================================

/// Get or create the Discogs client using the KeyService.
pub fn get_discogs_client(key_service: &KeyService) -> Result<DiscogsClient, String> {
    match key_service.get_discogs_key() {
        Some(key) => Ok(DiscogsClient::new(key)),
        None => Err(
            "Discogs API key not configured. Go to Settings → Discogs to add your key.".to_string(),
        ),
    }
}

// ============================================================================
// Detection helpers
// ============================================================================

pub enum DiscIdLookupResult {
    NoMatches,
    SingleMatch(Box<DisplayMatchCandidate>),
    MultipleMatches(Vec<DisplayMatchCandidate>),
}

/// Detect local metadata and files for a candidate before it is shown in the UI.
pub fn detect_candidate_locally(
    candidate: &CoreDetectedCandidate,
    port: u16,
) -> Result<(CategorizedFileInfo, DisplayFolderMetadata), String> {
    let files = categorized_files_from_scanned(&candidate.files, port);

    info!(
        "Detecting metadata for candidate: {} ({:?})",
        candidate.name, candidate.path
    );

    let folder_contents = detect_folder_contents(candidate.path.clone())
        .map_err(|e| format!("Failed to detect folder contents: {}", e))?;
    let core_metadata = folder_contents.metadata;

    info!(
        "Detected metadata: artist={:?}, album={:?}, year={:?}, mb_discid={:?}",
        core_metadata.artist, core_metadata.album, core_metadata.year, core_metadata.mb_discid
    );

    let metadata = to_display_metadata(&core_metadata);

    Ok((files, metadata))
}

/// Lookup a MusicBrainz release by DiscID.
pub async fn lookup_discid(
    mb_discid: &str,
    key_service: &KeyService,
) -> Result<DiscIdLookupResult, String> {
    info!("🎵 Looking up MB DiscID: {}", mb_discid);

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
    key_service: &KeyService,
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
        DiscIdLookupResult::SingleMatch(Box::new(display_candidates.into_iter().next().unwrap()))
    } else {
        DiscIdLookupResult::MultipleMatches(display_candidates)
    }
}

// ============================================================================
// Duplicate detection helper
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
// Search helpers
// ============================================================================

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

const MAX_SEARCH_RETRIES: u32 = 3;
const COVER_CHECK_RETRIES: u32 = 2;

/// Build a reqwest client for Cover Art Archive checks.
/// Disables redirects so we can read the 307 Location header without following it.
pub fn build_caa_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(2))
        .user_agent("bae/0.0.0-dev (https://github.com/nichochar/bae)")
        .build()
        .expect("failed to build CAA client")
}

/// Check whether cover art exists for a MusicBrainz release.
///
/// Returns `(Some(url), false)` if the CAA has art (307 with Location header),
/// `(None, false)` if no art exists (404), or `(None, true)` on network/server error
/// after retries.
pub async fn check_cover_art(client: &reqwest::Client, release_id: &str) -> (Option<String>, bool) {
    let url = format!(
        "https://coverartarchive.org/release/{}/front-250",
        release_id
    );

    for attempt in 0..=COVER_CHECK_RETRIES {
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == 307 {
                    // Art exists — extract the redirect URL
                    let location = resp
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    return (location.or(Some(url)), false);
                } else if status == 404 {
                    // No art — not an error, just missing
                    return (None, false);
                }
                // 5xx or unexpected status — retry
            }
            Err(_) => {
                // Network error — retry
            }
        }

        if attempt < COVER_CHECK_RETRIES {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    // All retries exhausted
    (None, true)
}

/// Check cover art for a batch of MB release IDs concurrently.
/// Returns a vec of (cover_url, cover_fetch_failed) in the same order.
async fn check_cover_art_batch(release_ids: &[Option<&str>]) -> Vec<(Option<String>, bool)> {
    let client = build_caa_client();
    let futures: Vec<_> = release_ids
        .iter()
        .map(|id| {
            let client = &client;
            async move {
                match id {
                    Some(rid) if !rid.is_empty() => check_cover_art(client, rid).await,
                    _ => (None, false),
                }
            }
        })
        .collect();
    futures::future::join_all(futures).await
}

/// Search MusicBrainz and rank results
async fn search_mb_and_rank(
    params: ReleaseSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let releases =
        bae_core::retry::retry_with_backoff(MAX_SEARCH_RETRIES, "MusicBrainz search", || {
            search_releases_with_params(&params)
        })
        .await
        .map_err(|e| format!("MusicBrainz search failed: {}", e))?;

    info!("✓ MusicBrainz search returned {} result(s)", releases.len());
    let candidates = if let Some(ref meta) = metadata {
        use bae_core::import::rank_mb_matches;
        rank_mb_matches(meta, releases)
    } else {
        releases
            .into_iter()
            .map(|release| MatchCandidate {
                source: MatchSource::MusicBrainz(release),
                confidence: 50.0,
                match_reasons: vec!["Manual search result".to_string()],
                cover_art_url: None,
            })
            .collect()
    };

    // Check CAA for cover art existence concurrently.
    // 307 → use redirect URL, 404 → no art, error → mark failed for retry.
    let release_ids: Vec<Option<&str>> = candidates
        .iter()
        .map(|c| match &c.source {
            MatchSource::MusicBrainz(r) if c.cover_art_url.is_none() => Some(r.release_id.as_str()),
            _ => None,
        })
        .collect();

    let cover_results = check_cover_art_batch(&release_ids).await;

    let mut display: Vec<DisplayMatchCandidate> =
        candidates.iter().map(to_display_candidate).collect();

    for (d, (url, failed)) in display.iter_mut().zip(cover_results) {
        if d.cover_url.is_none() {
            d.cover_url = url;
            d.cover_fetch_failed = failed;
        }
    }

    Ok(display)
}

/// Search Discogs and rank results
async fn search_discogs_and_rank(
    client: &DiscogsClient,
    params: DiscogsSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let results = bae_core::retry::retry_with_backoff(MAX_SEARCH_RETRIES, "Discogs search", || {
        client.search_with_params(&params)
    })
    .await
    .map_err(|e| format!("Discogs search failed: {}", e))?;

    info!("✓ Discogs search returned {} result(s)", results.len());
    let candidates: Vec<MatchCandidate> = if let Some(ref meta) = metadata {
        use bae_core::import::rank_discogs_matches;
        rank_discogs_matches(meta, results)
    } else {
        results
            .into_iter()
            .map(|result| {
                let cover_art_url = result
                    .cover_image
                    .clone()
                    .or_else(|| result.thumb.clone())
                    .map(|url| bae_core::network::upgrade_to_https(&url));
                MatchCandidate {
                    source: MatchSource::Discogs(result),
                    confidence: 50.0,
                    match_reasons: vec!["Manual search result".to_string()],
                    cover_art_url,
                }
            })
            .collect()
    };

    Ok(candidates.iter().map(to_display_candidate).collect())
}

/// General search by artist, album, year, label
pub async fn search_general(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    artist: String,
    album: String,
    year: String,
    label: String,
    key_service: &KeyService,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: non_empty(artist),
                album: non_empty(album),
                year: non_empty(year),
                label: non_empty(label),
                catalog_number: None,
                barcode: None,
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz general search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client(key_service)?;
            let params = DiscogsSearchParams {
                artist: non_empty(artist),
                release_title: non_empty(album),
                year: non_empty(year),
                label: non_empty(label),
                catno: None,
                barcode: None,
                format: None,
                country: None,
            };
            info!("🔍 Discogs general search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

/// Search by catalog number only
pub async fn search_by_catalog_number(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    catalog_number: String,
    key_service: &KeyService,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: None,
                album: None,
                year: None,
                label: None,
                catalog_number: Some(catalog_number),
                barcode: None,
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz catalog number search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client(key_service)?;
            let params = DiscogsSearchParams {
                artist: None,
                release_title: None,
                year: None,
                label: None,
                catno: Some(catalog_number),
                barcode: None,
                format: None,
                country: None,
            };
            info!("🔍 Discogs catalog number search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

/// Search by barcode only
pub async fn search_by_barcode(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    barcode: String,
    key_service: &KeyService,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: None,
                album: None,
                year: None,
                label: None,
                catalog_number: None,
                barcode: Some(barcode),
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz barcode search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client(key_service)?;
            let params = DiscogsSearchParams {
                artist: None,
                release_title: None,
                year: None,
                label: None,
                catno: None,
                barcode: Some(barcode),
                format: None,
                country: None,
            };
            info!("🔍 Discogs barcode search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

// ============================================================================
// Import helpers
// ============================================================================

/// Fetch full Discogs release details for import
async fn fetch_discogs_release(
    release_id: &str,
    master_id: Option<&str>,
    key_service: &KeyService,
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

/// Confirm a match candidate and start the import workflow.
pub async fn confirm_and_start_import(
    app: &AppService,
    candidate: DisplayMatchCandidate,
    import_source: ImportSource,
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
                let release_id = candidate
                    .discogs_release_id
                    .as_ref()
                    .ok_or_else(|| "Missing Discogs release ID".to_string())?;

                let discogs_release = fetch_discogs_release(
                    release_id,
                    candidate.discogs_master_id.as_deref(),
                    &app.key_service,
                )
                .await?;

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

// ============================================================================
// Scan event consumption
// ============================================================================

/// Consume folder scan events and update import state
pub async fn consume_scan_events(app: AppService, mut rx: broadcast::Receiver<ScanEvent>) {
    loop {
        let mut import_store = app.state.import();
        let existing_paths: HashSet<String> = {
            let state = import_store.read();
            state
                .detected_candidates
                .iter()
                .map(|c| c.path.clone())
                .collect()
        };

        let mut first_selected_index = None;

        loop {
            match rx.recv().await {
                Ok(ScanEvent::Candidate(candidate)) => {
                    let key = candidate.path.to_string_lossy().to_string();
                    if existing_paths.contains(&key) {
                        continue;
                    }

                    let (files, metadata) =
                        match detect_candidate_locally(&candidate, app.image_server_port) {
                            Ok(result) => result,
                            Err(e) => {
                                warn!(
                                    "Skipping candidate {} due to detection failure: {}",
                                    candidate.name, e
                                );
                                continue;
                            }
                        };

                    // Convert to display type
                    let display_candidate = bae_ui::display_types::DetectedCandidate {
                        name: candidate.name.clone(),
                        path: key.clone(),
                        status: bae_ui::display_types::DetectedCandidateStatus::Pending,
                    };

                    {
                        let mut state = import_store.write();
                        state.init_state_machine(&key, files, metadata);
                        state.detected_candidates.push(display_candidate);

                        if state.current_candidate_key.is_none() {
                            let index = state.detected_candidates.len() - 1;
                            state.switch_candidate(Some(key));
                            state.current_release_index = index;
                            first_selected_index = Some(index);
                        }
                    }
                }
                Ok(ScanEvent::Error(error)) => {
                    warn!("Scan error: {}", error);
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Ok(ScanEvent::Finished) => {
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Scan event receiver lagged, missed {} events", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    import_store.write().is_scanning_candidates = false;
                    return;
                }
            }
        }

        // After scan completes, load the first selected release if any
        if let Some(index) = first_selected_index {
            let detected = import_store.read().detected_candidates.clone();
            if let Err(e) = load_selected_release(&app, index, &detected).await {
                warn!("Failed to load selected release: {}", e);
            }
        }
    }
}

// ============================================================================
// Navigation helpers
// ============================================================================

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

// ============================================================================
// File categorization helper
// ============================================================================

/// Convert scanned file to display FileInfo
fn scanned_to_file_info(
    f: &bae_core::import::folder_scanner::ScannedFile,
    port: u16,
) -> bae_ui::display_types::FileInfo {
    let ext_lower = f
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = ext_lower.to_uppercase();
    let name = f
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let path = f.path.to_string_lossy().to_string();
    let display_url = bae_core::image_server::local_file_url(port, &f.path);

    bae_ui::display_types::FileInfo {
        name,
        path,
        size: f.size,
        format,
        display_url,
    }
}

/// Convert CategorizedFiles from core to display type
pub fn categorized_files_from_scanned(
    files: &bae_core::import::CategorizedFiles,
    port: u16,
) -> CategorizedFileInfo {
    use bae_core::import::folder_scanner::AudioContent;
    use bae_ui::display_types::{CueFlacPairInfo, FileInfo};

    let audio = match &files.audio {
        AudioContent::CueFlacPairs(pairs) => {
            let display_pairs: Vec<CueFlacPairInfo> = pairs
                .iter()
                .map(|p| CueFlacPairInfo {
                    cue_name: p
                        .cue_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    cue_path: p.cue_file.path.to_string_lossy().to_string(),
                    flac_name: p
                        .audio_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    total_size: p.cue_file.size + p.audio_file.size,
                    track_count: p.track_count,
                })
                .collect();
            AudioContentInfo::CueFlacPairs(display_pairs)
        }
        AudioContent::TrackFiles(tracks) => {
            let mut display_tracks: Vec<FileInfo> = tracks
                .iter()
                .map(|t| scanned_to_file_info(t, port))
                .collect();
            display_tracks.sort_by(|a, b| a.name.cmp(&b.name));
            AudioContentInfo::TrackFiles(display_tracks)
        }
    };

    let mut artwork: Vec<FileInfo> = files
        .artwork
        .iter()
        .map(|f| scanned_to_file_info(f, port))
        .collect();
    artwork.sort_by(|a, b| a.name.cmp(&b.name));

    let mut documents: Vec<FileInfo> = files
        .documents
        .iter()
        .map(|f| scanned_to_file_info(f, port))
        .collect();
    documents.sort_by(|a, b| a.name.cmp(&b.name));

    CategorizedFileInfo {
        audio,
        artwork,
        documents,
        bad_audio_count: files.bad_audio_count,
        bad_image_count: files.bad_image_count,
    }
}
