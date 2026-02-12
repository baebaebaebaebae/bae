//! Search orchestration: MusicBrainz + Discogs search, ranking, and cover art checking.

use super::conversion::{from_display_metadata, to_display_candidate};
use bae_core::discogs::client::DiscogsSearchParams;
use bae_core::discogs::DiscogsClient;
use bae_core::import::{MatchCandidate, MatchSource};
use bae_core::keys::KeyService;
use bae_core::musicbrainz::{search_releases_with_params, ReleaseSearchParams};
use bae_ui::display_types::{
    FolderMetadata as DisplayFolderMetadata, MatchCandidate as DisplayMatchCandidate, SearchSource,
};
use reqwest::redirect;
use tracing::info;

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

const MAX_SEARCH_RETRIES: u32 = 3;
const COVER_CHECK_RETRIES: u32 = 2;

/// Get or create the Discogs client using the KeyService.
pub fn get_discogs_client(key_service: &KeyService) -> Result<DiscogsClient, String> {
    match key_service.get_discogs_key() {
        Some(key) => Ok(DiscogsClient::new(key)),
        None => Err(
            "Discogs API key not configured. Go to Settings → Discogs to add your key.".to_string(),
        ),
    }
}

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
pub(super) async fn search_mb_and_rank(
    params: ReleaseSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let releases =
        bae_core::retry::retry_with_backoff(MAX_SEARCH_RETRIES, "MusicBrainz search", || {
            search_releases_with_params(&params)
        })
        .await
        .map_err(|e| format!("MusicBrainz search failed: {}", e))?;

    info!("MusicBrainz search returned {} result(s)", releases.len());
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
pub(super) async fn search_discogs_and_rank(
    client: &DiscogsClient,
    params: DiscogsSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let results = bae_core::retry::retry_with_backoff(MAX_SEARCH_RETRIES, "Discogs search", || {
        client.search_with_params(&params)
    })
    .await
    .map_err(|e| format!("Discogs search failed: {}", e))?;

    info!("Discogs search returned {} result(s)", results.len());
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
            info!("MusicBrainz general search: {:?}", params);
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
            info!("Discogs general search: {:?}", params);
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
            info!("MusicBrainz catalog number search: {:?}", params);
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
            info!("Discogs catalog number search: {:?}", params);
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
            info!("MusicBrainz barcode search: {:?}", params);
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
            info!("Discogs barcode search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}
