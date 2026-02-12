use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, info, warn};

/// Shared HTTP client for all MusicBrainz requests.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("bae/1.0 +https://github.com/bae-fm/bae")
            .build()
            .expect("Failed to create HTTP client")
    })
}

/// Rate limiter ensuring at least 1 second between MusicBrainz API requests.
fn rate_limiter() -> &'static Mutex<Instant> {
    static LIMITER: OnceLock<Mutex<Instant>> = OnceLock::new();
    LIMITER.get_or_init(|| Mutex::new(Instant::now() - Duration::from_secs(1)))
}

async fn wait_for_rate_limit() {
    let mut last_request = rate_limiter().lock().await;
    let elapsed = last_request.elapsed();
    if elapsed < Duration::from_secs(1) {
        tokio::time::sleep(Duration::from_secs(1) - elapsed).await;
    }
    *last_request = Instant::now();
}

// ============================================================================
// Serde response types for MusicBrainz API
// ============================================================================

/// A URL relation from MusicBrainz (used across release and release-group responses)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbRelation {
    pub url: Option<MbUrlResource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbUrlResource {
    pub resource: Option<String>,
}

/// Artist credit entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistCredit {
    pub name: Option<String>,
    pub artist: Option<MbArtistRef>,
}

/// Reference to a MusicBrainz artist within an artist-credit
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbArtistRef {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "sort-name")]
    pub sort_name: Option<String>,
}

/// Label info entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabelInfo {
    pub label: Option<MbLabel>,
    #[serde(rename = "catalog-number")]
    pub catalog_number: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbLabel {
    pub name: Option<String>,
}

/// Release group as embedded in a release response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseGroupRef {
    pub id: Option<String>,
    #[serde(rename = "first-release-date")]
    pub first_release_date: Option<String>,
    #[serde(rename = "secondary-types", default)]
    pub secondary_types: Vec<String>,
    #[serde(default)]
    pub relations: Option<Vec<MbRelation>>,
}

impl MbReleaseGroupRef {
    pub fn is_compilation(&self) -> bool {
        self.secondary_types
            .iter()
            .any(|t| t.eq_ignore_ascii_case("compilation"))
    }
}

/// A recording within a track
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbRecording {
    pub title: Option<String>,
}

/// A track within a medium
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbTrack {
    pub position: Option<i64>,
    pub number: Option<String>,
    pub title: Option<String>,
    pub length: Option<u64>,
    pub recording: Option<MbRecording>,
}

/// A medium (disc) within a release
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbMedium {
    pub format: Option<String>,
    #[serde(default)]
    pub tracks: Vec<MbTrack>,
}

/// A full release as returned by the MB release lookup API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MbReleaseResponse {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    pub artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    pub release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    pub label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    pub media: Vec<MbMedium>,
    #[serde(default)]
    pub relations: Vec<MbRelation>,
}

impl MbReleaseResponse {
    /// Convert to the domain-level MbRelease type
    pub fn to_mb_release(&self) -> MbRelease {
        let artist = self
            .artist_credit
            .first()
            .and_then(|ac| ac.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());

        let first_release_date = self
            .release_group
            .as_ref()
            .and_then(|rg| rg.first_release_date.clone())
            .filter(|s| !s.is_empty());

        let release_group_id = self
            .release_group
            .as_ref()
            .and_then(|rg| rg.id.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let format = self.media.first().and_then(|m| m.format.clone());

        let (label, catalog_number) = self
            .label_info
            .first()
            .map(|li| {
                (
                    li.label.as_ref().and_then(|l| l.name.clone()),
                    li.catalog_number.clone().filter(|s| !s.is_empty()),
                )
            })
            .unwrap_or((None, None));

        let is_compilation = self
            .release_group
            .as_ref()
            .is_some_and(|rg| rg.is_compilation());

        MbRelease {
            release_id: self.id.clone(),
            release_group_id,
            title: self.title.clone(),
            artist,
            date: self.date.clone(),
            first_release_date,
            format,
            country: self.country.clone(),
            label,
            catalog_number,
            barcode: self.barcode.clone().filter(|s| !s.is_empty()),
            is_compilation,
        }
    }

    /// Extract ExternalUrls from release relations and release-group relations
    fn extract_external_urls(&self) -> ExternalUrls {
        let mut urls = ExternalUrls {
            discogs_master_url: None,
            discogs_release_url: None,
            bandcamp_url: None,
        };

        // Extract from release-level relations
        extract_urls_from_relations(&self.relations, &mut urls);

        // Extract from release-group relations (if present inline)
        if let Some(rg) = &self.release_group {
            if let Some(rg_relations) = &rg.relations {
                extract_urls_from_relations(rg_relations, &mut urls);
            }
        }

        urls
    }

    /// Count total tracks across all media
    pub fn track_count(&self) -> usize {
        self.media.iter().map(|m| m.tracks.len()).sum()
    }
}

/// Response from the disc ID lookup endpoint
#[derive(Debug, Clone, Deserialize)]
struct DiscIdResponse {
    #[serde(default)]
    releases: Vec<DiscIdRelease>,
}

/// A release within a disc ID lookup response (has slightly different shape from full release)
#[derive(Debug, Clone, Deserialize)]
struct DiscIdRelease {
    id: Option<String>,
    title: Option<String>,
    date: Option<String>,
    country: Option<String>,
    barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    media: Vec<MbMedium>,
    #[serde(default)]
    relations: Vec<MbRelation>,
}

impl DiscIdRelease {
    fn to_mb_release(&self) -> Option<MbRelease> {
        let id = self.id.as_ref()?;
        let title = self.title.as_ref()?;
        let release_group_id = self.release_group.as_ref().and_then(|rg| rg.id.clone())?;

        let artist = self
            .artist_credit
            .first()
            .and_then(|ac| ac.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());

        let first_release_date = self
            .release_group
            .as_ref()
            .and_then(|rg| rg.first_release_date.clone())
            .filter(|s| !s.is_empty());

        let format = self.media.first().and_then(|m| m.format.clone());

        let (label, catalog_number) = self
            .label_info
            .first()
            .map(|li| {
                (
                    li.label.as_ref().and_then(|l| l.name.clone()),
                    li.catalog_number.clone().filter(|s| !s.is_empty()),
                )
            })
            .unwrap_or((None, None));

        let is_compilation = self
            .release_group
            .as_ref()
            .is_some_and(|rg| rg.is_compilation());

        Some(MbRelease {
            release_id: id.clone(),
            release_group_id,
            title: title.clone(),
            artist,
            date: self.date.clone(),
            first_release_date,
            format,
            country: self.country.clone(),
            label,
            catalog_number,
            barcode: self.barcode.clone().filter(|s| !s.is_empty()),
            is_compilation,
        })
    }
}

/// Response from the release search endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
struct SearchResponse {
    #[serde(default)]
    releases: Vec<SearchRelease>,
    error: Option<String>,
}

/// A release in search results (less data than full lookup)
#[derive(Debug, Clone, Deserialize, Serialize)]
struct SearchRelease {
    id: Option<String>,
    title: Option<String>,
    date: Option<String>,
    country: Option<String>,
    barcode: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbArtistCredit>,
    #[serde(rename = "release-group")]
    release_group: Option<MbReleaseGroupRef>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<MbLabelInfo>,
}

impl SearchRelease {
    fn to_mb_release(&self) -> Option<MbRelease> {
        let id = self.id.as_ref()?;
        let title = self.title.as_ref()?;

        let release_group_id = self
            .release_group
            .as_ref()
            .and_then(|rg| rg.id.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let artist = self
            .artist_credit
            .first()
            .and_then(|ac| ac.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());

        let (label, catalog_number) = self
            .label_info
            .first()
            .map(|li| {
                (
                    li.label.as_ref().and_then(|l| l.name.clone()),
                    li.catalog_number.clone(),
                )
            })
            .unwrap_or((None, None));

        let is_compilation = self
            .release_group
            .as_ref()
            .is_some_and(|rg| rg.is_compilation());

        Some(MbRelease {
            release_id: id.clone(),
            release_group_id,
            title: title.clone(),
            artist,
            date: self.date.clone(),
            first_release_date: None,
            format: None,
            country: self.country.clone(),
            label,
            catalog_number,
            barcode: self.barcode.clone(),
            is_compilation,
        })
    }
}

/// Release group response (for separate fetch with url-rels)
#[derive(Debug, Clone, Deserialize)]
struct ReleaseGroupResponse {
    #[serde(default)]
    relations: Vec<MbRelation>,
}

/// Extract external URLs from a list of relations into the target struct
fn extract_urls_from_relations(relations: &[MbRelation], urls: &mut ExternalUrls) {
    for relation in relations {
        let Some(url_obj) = &relation.url else {
            continue;
        };
        let Some(resource) = &url_obj.resource else {
            continue;
        };

        if resource.contains("discogs.com/master/") && urls.discogs_master_url.is_none() {
            urls.discogs_master_url = Some(resource.clone());
        } else if resource.contains("discogs.com/release/") && urls.discogs_release_url.is_none() {
            urls.discogs_release_url = Some(resource.clone());
        } else if resource.contains("bandcamp.com") && urls.bandcamp_url.is_none() {
            urls.bandcamp_url = Some(resource.clone());
        }
    }
}

// ============================================================================
// Domain types (public API, unchanged)
// ============================================================================

/// MusicBrainz release information
#[derive(Debug, Clone, PartialEq)]
pub struct MbRelease {
    pub release_id: String,
    pub release_group_id: String,
    pub title: String,
    pub artist: String,
    pub date: Option<String>,
    pub first_release_date: Option<String>,
    pub format: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
    pub is_compilation: bool,
}

/// External URLs extracted from MusicBrainz relationships
#[derive(Debug, Clone)]
pub struct ExternalUrls {
    pub discogs_master_url: Option<String>,
    pub discogs_release_url: Option<String>,
    pub bandcamp_url: Option<String>,
}

#[derive(Debug, Error)]
pub enum MusicBrainzError {
    #[error("MusicBrainz API error: {0}")]
    Api(String),
    #[error("No release found for DISCID: {0}")]
    NotFound(String),
}

// ============================================================================
// API functions
// ============================================================================

/// Lookup releases by MusicBrainz DiscID
pub async fn lookup_by_discid(
    discid: &str,
) -> Result<(Vec<MbRelease>, ExternalUrls), MusicBrainzError> {
    info!("MusicBrainz: Looking up DiscID '{}'", discid);
    let base_url = reqwest::Url::parse("https://musicbrainz.org/ws/2/discid/")
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse base URL: {}", e)))?;
    let url = base_url
        .join(discid)
        .map_err(|e| MusicBrainzError::Api(format!("Failed to construct DiscID URL: {}", e)))?;
    let mut url_with_params = url.clone();
    url_with_params.set_query(Some(
        "inc=recordings+artist-credits+release-groups+url-rels+labels",
    ));
    debug!("MusicBrainz API request: {}", url_with_params);

    wait_for_rate_limit().await;

    let response = http_client()
        .get(url_with_params.as_str())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        warn!(
            "MusicBrainz API error response ({}): {}",
            status, error_text
        );

        if status == 404 {
            return Err(MusicBrainzError::NotFound(discid.to_string()));
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status {}: {}",
            status, error_text
        )));
    }

    let disc_response: DiscIdResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    let mut releases = Vec::new();
    let mut external_urls = ExternalUrls {
        discogs_master_url: None,
        discogs_release_url: None,
        bandcamp_url: None,
    };

    for release in &disc_response.releases {
        if let Some(mb_release) = release.to_mb_release() {
            releases.push(mb_release);

            // Only extract URLs from first release that has them
            if external_urls.discogs_master_url.is_none() {
                extract_urls_from_relations(&release.relations, &mut external_urls);
            }
        }
    }

    if releases.is_empty() {
        return Err(MusicBrainzError::NotFound(discid.to_string()));
    }

    info!(
        "MusicBrainz found {} release(s) for DiscID {}",
        releases.len(),
        discid
    );

    if external_urls.discogs_master_url.is_some() || external_urls.discogs_release_url.is_some() {
        info!("  Found Discogs URL in relationships");
    }

    Ok((releases, external_urls))
}

/// Fetch a release-group with its URL relationships
async fn fetch_release_group_with_relations(
    release_group_id: &str,
) -> Result<ReleaseGroupResponse, MusicBrainzError> {
    let url = format!(
        "https://musicbrainz.org/ws/2/release-group/{}?inc=url-rels",
        release_group_id
    );
    debug!("Fetching release-group with relations: {}", url);

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))
}

/// Lookup a specific release by MusicBrainz release ID.
///
/// Returns the domain-level MbRelease, extracted ExternalUrls, and the full typed
/// response (for downstream track parsing and UI display).
pub async fn lookup_release_by_id(
    release_id: &str,
) -> Result<(MbRelease, ExternalUrls, MbReleaseResponse), MusicBrainzError> {
    info!("MusicBrainz: Looking up release ID '{}'", release_id);
    let url = format!(
        "https://musicbrainz.org/ws/2/release/{}?inc=recordings+artist-credits+release-groups+release-group-rels+url-rels+labels+media",
        release_id,
    );
    debug!("MusicBrainz API request: {}", url);

    wait_for_rate_limit().await;

    let response = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        if response.status() == 404 {
            return Err(MusicBrainzError::NotFound(release_id.to_string()));
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status: {}",
            response.status()
        )));
    }

    let mb_response: MbReleaseResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    #[cfg(debug_assertions)]
    {
        let temp_path = std::env::temp_dir().join("musicbrainz_release_response.json");
        if let Ok(json_str) = serde_json::to_string_pretty(&mb_response) {
            let _ = std::fs::write(&temp_path, json_str);
            debug!("MusicBrainz release response written to {:?}", temp_path);
        }
    }

    let mb_release = mb_response.to_mb_release();
    let mut external_urls = mb_response.extract_external_urls();

    debug!(
        "MusicBrainz release response: {} - {} ({} relations), release_id: {}",
        mb_release.artist,
        mb_release.title,
        mb_response.relations.len(),
        release_id
    );

    if let Some(resource) = &external_urls.discogs_master_url {
        info!("Found Discogs master URL: {}", resource);
    }
    if let Some(resource) = &external_urls.discogs_release_url {
        info!("Found Discogs release URL: {}", resource);
    }

    // If release-group relations weren't included inline, fetch them separately
    if external_urls.discogs_master_url.is_none() && external_urls.discogs_release_url.is_none() {
        let has_rg_relations = mb_response
            .release_group
            .as_ref()
            .is_some_and(|rg| rg.relations.is_some());

        if !has_rg_relations {
            if let Some(rg_id) = mb_response
                .release_group
                .as_ref()
                .and_then(|rg| rg.id.as_deref())
            {
                debug!(
                    "Release-group relations not found, fetching release-group {} separately",
                    rg_id
                );

                if let Ok(rg_response) = fetch_release_group_with_relations(rg_id).await {
                    extract_urls_from_relations(&rg_response.relations, &mut external_urls);

                    if let Some(resource) = &external_urls.discogs_master_url {
                        info!("Found Discogs master URL on release-group: {}", resource);
                    }
                    if let Some(resource) = &external_urls.discogs_release_url {
                        info!("Found Discogs release URL on release-group: {}", resource);
                    }
                }
            }
        }
    }

    Ok((mb_release, external_urls, mb_response))
}

// ============================================================================
// Search
// ============================================================================

/// Parameters for searching MusicBrainz releases
#[derive(Debug, Clone, Default)]
pub struct ReleaseSearchParams {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub barcode: Option<String>,
    pub format: Option<String>,
    pub country: Option<String>,
}

impl ReleaseSearchParams {
    /// Check if at least one field is filled
    pub fn has_any_field(&self) -> bool {
        self.artist.is_some()
            || self.album.is_some()
            || self.year.is_some()
            || self.label.is_some()
            || self.catalog_number.is_some()
            || self.barcode.is_some()
            || self.format.is_some()
            || self.country.is_some()
    }

    /// Build Lucene query string from filled fields
    fn build_query(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref artist) = self.artist {
            if !artist.trim().is_empty() {
                parts.push(format!("artist:\"{}\"", artist.trim()));
            }
        }
        if let Some(ref album) = self.album {
            if !album.trim().is_empty() {
                parts.push(format!("release:\"{}\"", album.trim()));
            }
        }
        if let Some(ref year) = self.year {
            if !year.trim().is_empty() {
                parts.push(format!("date:{}", year.trim()));
            }
        }
        if let Some(ref label) = self.label {
            if !label.trim().is_empty() {
                parts.push(format!("label:\"{}\"", label.trim()));
            }
        }
        if let Some(ref catno) = self.catalog_number {
            if !catno.trim().is_empty() {
                parts.push(format!("catno:\"{}\"", catno.trim()));
            }
        }
        if let Some(ref barcode) = self.barcode {
            if !barcode.trim().is_empty() {
                parts.push(format!("barcode:{}", barcode.trim()));
            }
        }
        if let Some(ref format) = self.format {
            if !format.trim().is_empty() {
                parts.push(format!("format:\"{}\"", format.trim()));
            }
        }
        if let Some(ref country) = self.country {
            if !country.trim().is_empty() {
                parts.push(format!("country:\"{}\"", country.trim()));
            }
        }
        parts.join(" AND ")
    }
}

/// Clean album name for search by removing common metadata patterns
pub fn clean_album_name_for_search(album: &str) -> String {
    use regex::Regex;
    let mut cleaned = album.to_string();
    let bracket_pattern = Regex::new(r"\s*\[([^\]]+)\]\s*").unwrap();
    cleaned = bracket_pattern.replace_all(&cleaned, " ").to_string();
    let year_pattern = Regex::new(r"\s*\((\d{4})\)\s*$").unwrap();
    cleaned = year_pattern.replace_all(&cleaned, "").to_string();
    let disc_pattern = Regex::new(r"(?i)\s*\((Disc|CD)\s*\d+\)\s*$").unwrap();
    cleaned = disc_pattern.replace_all(&cleaned, "").to_string();
    let edition_pattern =
        Regex::new(r"(?i)\s*\((Remaster(ed)?|Deluxe|Limited|Special|Expanded)(\s+Edition)?\)\s*$")
            .unwrap();
    cleaned = edition_pattern.replace_all(&cleaned, "").to_string();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract search tokens from folder metadata for the search pills UI
///
/// Combines artist, cleaned album title, year, and folder tokens into a
/// deduplicated list of tokens for display as clickable pills.
pub fn extract_search_tokens(metadata: &crate::import::FolderMetadata) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add_token = |s: &str| {
        let trimmed = s.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_lowercase()) {
            tokens.push(trimmed.to_string());
        }
    };
    if let Some(ref artist) = metadata.artist {
        add_token(artist);
    }
    if let Some(ref album) = metadata.album {
        let cleaned = clean_album_name_for_search(album);
        add_token(&cleaned);
    }
    if let Some(year) = metadata.year {
        add_token(&year.to_string());
    }
    for token in &metadata.folder_tokens {
        add_token(token);
    }
    tokens
}

/// Search MusicBrainz for releases using structured parameters
pub async fn search_releases_with_params(
    params: &ReleaseSearchParams,
) -> Result<Vec<MbRelease>, MusicBrainzError> {
    if !params.has_any_field() {
        return Err(MusicBrainzError::Api(
            "At least one search field must be provided".to_string(),
        ));
    }
    let query = params.build_query();
    info!("MusicBrainz: Searching with params: {:?}", params);
    info!("   Query: {}", query);
    let url = "https://musicbrainz.org/ws/2/release";
    debug!(
        "MusicBrainz API request: {}?query={}&limit=25&inc=recordings+artist-credits+release-groups+labels+media+url-rels",
        url, query
    );

    wait_for_rate_limit().await;

    let response = http_client()
        .get(url)
        .query(&[
            ("query", query.as_str()),
            ("limit", "25"),
            (
                "inc",
                "recordings+artist-credits+release-groups+labels+media+url-rels",
            ),
        ])
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        warn!(
            "MusicBrainz API error response ({}): {}",
            status, error_text
        );

        if status == 404 {
            return Ok(Vec::new());
        }
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz API returned status {}: {}",
            status, error_text
        )));
    }

    let search_response: SearchResponse = response
        .json()
        .await
        .map_err(|e| MusicBrainzError::Api(format!("Failed to parse JSON: {}", e)))?;

    #[cfg(debug_assertions)]
    {
        let temp_path = std::env::temp_dir().join("musicbrainz_search_response.json");
        if let Ok(json_str) = serde_json::to_string_pretty(&search_response) {
            let _ = std::fs::write(&temp_path, json_str);
            debug!("MusicBrainz search response written to {:?}", temp_path);
        }
    }

    if let Some(ref error_msg) = search_response.error {
        warn!("MusicBrainz API returned error: {}", error_msg);
        return Err(MusicBrainzError::Api(format!(
            "MusicBrainz error: {}",
            error_msg
        )));
    }

    let releases: Vec<MbRelease> = search_response
        .releases
        .iter()
        .filter_map(|r| r.to_mb_release())
        .collect();

    info!("Found {} release(s)", releases.len());
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_album_name() {
        assert_eq!(
            clean_album_name_for_search("Some Album (2000) [Test Label 823 359-2, 2001]",),
            "Some Album",
        );
        assert_eq!(
            clean_album_name_for_search("Another Album (Disc 2)"),
            "Another Album",
        );
        assert_eq!(
            clean_album_name_for_search("Third Album (Remastered)"),
            "Third Album"
        );
        assert_eq!(
            clean_album_name_for_search("Fourth Album (Deluxe Edition)"),
            "Fourth Album"
        );
    }

    #[test]
    fn test_release_search_params_build_query() {
        let params = ReleaseSearchParams {
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            year: Some("2000".to_string()),
            ..Default::default()
        };
        assert_eq!(
            params.build_query(),
            "artist:\"Test Artist\" AND release:\"Test Album\" AND date:2000",
        );
        let params2 = ReleaseSearchParams {
            artist: Some("Another Artist".to_string()),
            catalog_number: Some("TL-1234".to_string()),
            ..Default::default()
        };
        assert_eq!(
            params2.build_query(),
            "artist:\"Another Artist\" AND catno:\"TL-1234\""
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_rate_limiter_enforces_spacing() {
        // First call should return immediately
        let start = Instant::now();
        wait_for_rate_limit().await;
        let first_elapsed = start.elapsed();
        assert!(
            first_elapsed < Duration::from_millis(100),
            "First call should be near-instant, took {:?}",
            first_elapsed
        );

        // Second call should wait ~1 second
        let start = Instant::now();
        wait_for_rate_limit().await;
        let second_elapsed = start.elapsed();
        assert!(
            second_elapsed >= Duration::from_millis(900),
            "Second call should wait ~1s, only waited {:?}",
            second_elapsed
        );
    }

    #[test]
    fn test_release_group_is_compilation() {
        let rg = MbReleaseGroupRef {
            id: Some("test".to_string()),
            first_release_date: None,
            secondary_types: vec!["Compilation".to_string()],
            relations: None,
        };
        assert!(rg.is_compilation());

        let rg_no = MbReleaseGroupRef {
            id: Some("test".to_string()),
            first_release_date: None,
            secondary_types: vec!["Live".to_string()],
            relations: None,
        };
        assert!(!rg_no.is_compilation());

        let rg_empty = MbReleaseGroupRef {
            id: Some("test".to_string()),
            first_release_date: None,
            secondary_types: vec![],
            relations: None,
        };
        assert!(!rg_empty.is_compilation());
    }

    #[test]
    fn test_mb_release_response_to_mb_release() {
        let response = MbReleaseResponse {
            id: "release-123".to_string(),
            title: "Test Album".to_string(),
            date: Some("2020-01-15".to_string()),
            country: Some("US".to_string()),
            barcode: Some("012345678901".to_string()),
            artist_credit: vec![MbArtistCredit {
                name: Some("Test Artist".to_string()),
                artist: Some(MbArtistRef {
                    id: Some("artist-456".to_string()),
                    name: Some("Test Artist".to_string()),
                    sort_name: Some("Artist, Test".to_string()),
                }),
            }],
            release_group: Some(MbReleaseGroupRef {
                id: Some("rg-789".to_string()),
                first_release_date: Some("2020-01-15".to_string()),
                secondary_types: vec![],
                relations: None,
            }),
            label_info: vec![MbLabelInfo {
                label: Some(MbLabel {
                    name: Some("Test Label".to_string()),
                }),
                catalog_number: Some("TL-001".to_string()),
            }],
            media: vec![MbMedium {
                format: Some("CD".to_string()),
                tracks: vec![],
            }],
            relations: vec![],
        };

        let mb_release = response.to_mb_release();
        assert_eq!(mb_release.release_id, "release-123");
        assert_eq!(mb_release.title, "Test Album");
        assert_eq!(mb_release.artist, "Test Artist");
        assert_eq!(mb_release.release_group_id, "rg-789");
        assert_eq!(mb_release.format.as_deref(), Some("CD"));
        assert_eq!(mb_release.label.as_deref(), Some("Test Label"));
        assert_eq!(mb_release.catalog_number.as_deref(), Some("TL-001"));
        assert!(!mb_release.is_compilation);
    }

    #[test]
    fn test_mb_release_response_track_count() {
        let response = MbReleaseResponse {
            id: "r1".to_string(),
            title: "T".to_string(),
            date: None,
            country: None,
            barcode: None,
            artist_credit: vec![],
            release_group: None,
            label_info: vec![],
            media: vec![
                MbMedium {
                    format: None,
                    tracks: vec![
                        MbTrack {
                            position: Some(1),
                            number: None,
                            title: Some("Track 1".to_string()),
                            length: None,
                            recording: None,
                        },
                        MbTrack {
                            position: Some(2),
                            number: None,
                            title: Some("Track 2".to_string()),
                            length: None,
                            recording: None,
                        },
                    ],
                },
                MbMedium {
                    format: None,
                    tracks: vec![MbTrack {
                        position: Some(1),
                        number: None,
                        title: Some("Track 1 Disc 2".to_string()),
                        length: None,
                        recording: None,
                    }],
                },
            ],
            relations: vec![],
        };

        assert_eq!(response.track_count(), 3);
    }

    #[test]
    fn test_extract_urls_from_relations() {
        let relations = vec![
            MbRelation {
                url: Some(MbUrlResource {
                    resource: Some("https://www.discogs.com/master/12345".to_string()),
                }),
            },
            MbRelation {
                url: Some(MbUrlResource {
                    resource: Some("https://www.discogs.com/release/67890".to_string()),
                }),
            },
            MbRelation {
                url: Some(MbUrlResource {
                    resource: Some("https://artist.bandcamp.com/album/test".to_string()),
                }),
            },
            MbRelation { url: None },
        ];

        let mut urls = ExternalUrls {
            discogs_master_url: None,
            discogs_release_url: None,
            bandcamp_url: None,
        };

        extract_urls_from_relations(&relations, &mut urls);

        assert_eq!(
            urls.discogs_master_url.as_deref(),
            Some("https://www.discogs.com/master/12345")
        );
        assert_eq!(
            urls.discogs_release_url.as_deref(),
            Some("https://www.discogs.com/release/67890")
        );
        assert_eq!(
            urls.bandcamp_url.as_deref(),
            Some("https://artist.bandcamp.com/album/test")
        );
    }

    #[test]
    fn test_deserialize_mb_release_response() {
        let json = r#"{
            "id": "f9469bd8-a413-43f1-bee3-e3baabfb91cc",
            "title": "Super Hits of the 70s",
            "date": "2002",
            "country": null,
            "barcode": "8711638222024",
            "artist-credit": [{
                "name": "All Star Cover Band",
                "artist": {
                    "id": "53ebb100-5cfb-42e7-9ae3-453464420840",
                    "name": "All Star Cover Band",
                    "sort-name": "All Star Cover Band"
                }
            }],
            "release-group": {
                "id": "ded0036e-243a-4ae4-8c65-7ec37aae4bd9",
                "first-release-date": "2002",
                "secondary-types": [],
                "secondary-type-ids": []
            },
            "label-info": [{
                "catalog-number": "3822202",
                "label": { "name": "Galaxy Music" }
            }],
            "media": [{
                "format": "CD",
                "tracks": [
                    { "position": 1, "title": "Dancing Queen", "length": 216000 },
                    { "position": 2, "title": "Rivers of Babylon", "length": 241000 }
                ]
            }],
            "relations": [{
                "url": { "resource": "https://www.discogs.com/release/67890" }
            }]
        }"#;

        let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "f9469bd8-a413-43f1-bee3-e3baabfb91cc");
        assert_eq!(response.title, "Super Hits of the 70s");
        assert_eq!(response.date.as_deref(), Some("2002"));
        assert!(response.country.is_none());
        assert_eq!(response.barcode.as_deref(), Some("8711638222024"));
        assert_eq!(response.artist_credit.len(), 1);
        assert_eq!(
            response.artist_credit[0].name.as_deref(),
            Some("All Star Cover Band")
        );
        assert_eq!(response.media.len(), 1);
        assert_eq!(response.media[0].tracks.len(), 2);
        assert_eq!(
            response.media[0].tracks[0].title.as_deref(),
            Some("Dancing Queen")
        );
        assert_eq!(response.track_count(), 2);
        assert_eq!(response.label_info.len(), 1);
        assert_eq!(
            response.label_info[0].catalog_number.as_deref(),
            Some("3822202")
        );
        assert_eq!(response.relations.len(), 1);
    }

    #[test]
    fn test_deserialize_mb_release_response_minimal() {
        // Minimal response with only required fields — all optional arrays absent
        let json = r#"{
            "id": "abc-123",
            "title": "Minimal Release"
        }"#;

        let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "abc-123");
        assert_eq!(response.title, "Minimal Release");
        assert!(response.date.is_none());
        assert!(response.artist_credit.is_empty());
        assert!(response.media.is_empty());
        assert!(response.relations.is_empty());
        assert_eq!(response.track_count(), 0);
    }
}
