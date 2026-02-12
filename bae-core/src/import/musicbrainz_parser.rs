use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbTrack};
use crate::discogs::DiscogsClient;
use crate::musicbrainz::lookup_release_by_id;
use crate::retry::retry_with_backoff;
use tracing::{info, warn};
use uuid::Uuid;
/// Result of parsing a MusicBrainz release into database entities
pub type ParsedMbAlbum = (
    DbAlbum,
    DbRelease,
    Vec<DbTrack>,
    Vec<DbArtist>,
    Vec<DbAlbumArtist>,
);
/// Fetch full MusicBrainz release with tracklist and parse into database models
///
/// If the MB release has Discogs URLs in relationships and a DiscogsClient is provided,
/// fetches Discogs data to populate both discogs_release and musicbrainz_release fields
/// in DbAlbum, enabling cross-source duplicate detection.
///
pub async fn fetch_and_parse_mb_release(
    release_id: &str,
    master_year: u32,
    discogs_client: Option<&DiscogsClient>,
) -> Result<ParsedMbAlbum, String> {
    let (mb_release, external_urls, json) =
        retry_with_backoff(3, "MusicBrainz release fetch", || {
            lookup_release_by_id(release_id)
        })
        .await
        .map_err(|e| format!("Failed to fetch MusicBrainz release: {}", e))?;
    let discogs_release = match (&discogs_client, &external_urls.discogs_release_url) {
        (Some(client), Some(discogs_url)) => {
            if let Some(id) = discogs_url.split('/').next_back() {
                info!(
                    "Found Discogs release URL: {}, fetching release {}",
                    discogs_url, id
                );
                match client.get_release(id).await {
                    Ok(release) => Some(release),
                    Err(e) => {
                        warn!("Failed to fetch Discogs release {}: {}", id, e);
                        None
                    }
                }
            } else {
                None
            }
        }
        _ => None,
    };
    parse_mb_release_from_json(&json, &mb_release, master_year, discogs_release)
}
/// Parse MusicBrainz release JSON into database models
///
/// discogs_release: Optional Discogs release data to populate both fields in DbAlbum
fn parse_mb_release_from_json(
    json: &serde_json::Value,
    mb_release: &crate::musicbrainz::MbRelease,
    master_year: u32,
    discogs_release: Option<crate::discogs::DiscogsRelease>,
) -> Result<ParsedMbAlbum, String> {
    let album = if let Some(ref discogs_rel) = discogs_release {
        let mut album =
            DbAlbum::from_mb_release(mb_release, master_year, mb_release.is_compilation);
        album.discogs_release = Some(crate::db::DiscogsMasterRelease {
            master_id: discogs_rel.master_id.clone(),
            release_id: discogs_rel.id.clone(),
        });
        album
    } else {
        DbAlbum::from_mb_release(mb_release, master_year, mb_release.is_compilation)
    };
    let db_release = DbRelease::from_mb_release(&album.id, mb_release);
    let mut artists = Vec::new();
    let mut album_artists = Vec::new();
    if let Some(artist_credits) = json.get("artist-credit").and_then(|ac| ac.as_array()) {
        for (position, credit) in artist_credits.iter().enumerate() {
            if let Some(artist_obj) = credit.get("artist") {
                let artist_name = artist_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Artist")
                    .to_string();
                let mb_artist_id = artist_obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let sort_name = artist_obj
                    .get("sort-name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| artist_name.clone());
                let discogs_artist_id = discogs_release.as_ref().and_then(|dr| {
                    dr.artists
                        .iter()
                        .find(|da| da.name.eq_ignore_ascii_case(&artist_name))
                        .map(|da| da.id.clone())
                });
                let artist = DbArtist {
                    id: Uuid::new_v4().to_string(),
                    name: artist_name,
                    sort_name: Some(sort_name),
                    discogs_artist_id,
                    bandcamp_artist_id: None,
                    musicbrainz_artist_id: mb_artist_id,

                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                let album_artist = DbAlbumArtist::new(&album.id, &artist.id, position as i32);
                artists.push(artist);
                album_artists.push(album_artist);
            }
        }
    }
    if artists.is_empty() {
        let artist_name = mb_release.artist.clone();
        let artist = DbArtist {
            id: Uuid::new_v4().to_string(),
            name: artist_name.clone(),
            sort_name: Some(artist_name),
            discogs_artist_id: None,
            bandcamp_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let album_artist = DbAlbumArtist::new(&album.id, &artist.id, 0);
        artists.push(artist);
        album_artists.push(album_artist);
    }
    let mut tracks = Vec::new();
    let mut track_index = 0;
    if let Some(media_array) = json.get("media").and_then(|m| m.as_array()) {
        for (medium_index, medium) in media_array.iter().enumerate() {
            let disc_number = Some((medium_index + 1) as i32);
            if let Some(tracks_array) = medium.get("tracks").and_then(|t| t.as_array()) {
                for track_json in tracks_array {
                    if let Some(recording) = track_json.get("recording") {
                        let title = recording
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown Track")
                            .to_string();
                        let position = track_json
                            .get("position")
                            .and_then(|v| v.as_i64())
                            .map(|p| p as i32);
                        let track_number = position.or_else(|| Some(track_index + 1));
                        let now = chrono::Utc::now();
                        let track = DbTrack {
                            id: Uuid::new_v4().to_string(),
                            release_id: db_release.id.clone(),
                            title,
                            disc_number,
                            track_number,
                            duration_ms: None,
                            discogs_position: position.map(|p| p.to_string()),
                            import_status: crate::db::ImportStatus::Queued,
                            updated_at: now,
                            created_at: now,
                        };
                        tracks.push(track);
                        track_index += 1;
                    }
                }
            }
        }
    }
    Ok((album, db_release, tracks, artists, album_artists))
}
