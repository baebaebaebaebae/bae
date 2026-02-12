use super::ParsedAlbum;
use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbTrack};
use crate::discogs::DiscogsClient;
use crate::musicbrainz::{lookup_release_by_id, MbReleaseResponse};
use crate::retry::retry_with_backoff;
use tracing::{info, warn};
use uuid::Uuid;

/// Fetch full MusicBrainz release with tracklist and parse into database models
///
/// If the MB release has Discogs URLs in relationships and a DiscogsClient is provided,
/// fetches Discogs data to populate both discogs_release and musicbrainz_release fields
/// in DbAlbum, enabling cross-source duplicate detection.
pub async fn fetch_and_parse_mb_release(
    release_id: &str,
    master_year: u32,
    discogs_client: Option<&DiscogsClient>,
) -> Result<ParsedAlbum, String> {
    let (_mb_release, external_urls, response) =
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

    map_mb_response_to_db(&response, master_year, discogs_release)
}

/// Map a typed MusicBrainz release response into database models (pure, no I/O)
///
/// discogs_release: Optional Discogs release data to populate both fields in DbAlbum
pub fn map_mb_response_to_db(
    response: &MbReleaseResponse,
    master_year: u32,
    discogs_release: Option<crate::discogs::DiscogsRelease>,
) -> Result<ParsedAlbum, String> {
    let mb_release = response.to_mb_release();
    let album = if let Some(ref discogs_rel) = discogs_release {
        let mut album =
            DbAlbum::from_mb_release(&mb_release, master_year, mb_release.is_compilation);
        album.discogs_release = Some(crate::db::DiscogsMasterRelease {
            master_id: discogs_rel.master_id.clone(),
            release_id: discogs_rel.id.clone(),
        });
        album
    } else {
        DbAlbum::from_mb_release(&mb_release, master_year, mb_release.is_compilation)
    };

    let db_release = DbRelease::from_mb_release(&album.id, &mb_release);

    let mut artists = Vec::new();
    let mut album_artists = Vec::new();

    for (position, credit) in response.artist_credit.iter().enumerate() {
        if let Some(artist_obj) = &credit.artist {
            let artist_name = artist_obj
                .name
                .as_deref()
                .unwrap_or("Unknown Artist")
                .to_string();

            let mb_artist_id = artist_obj.id.clone();

            let sort_name = artist_obj
                .sort_name
                .clone()
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

    for (medium_index, medium) in response.media.iter().enumerate() {
        let disc_number = Some((medium_index + 1) as i32);

        for track in &medium.tracks {
            let title = track
                .recording
                .as_ref()
                .and_then(|r| r.title.clone())
                .unwrap_or_else(|| "Unknown Track".to_string());

            let position = track.position.map(|p| p as i32);
            let track_number = position.or(Some(track_index + 1));

            let now = chrono::Utc::now();
            let db_track = DbTrack {
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
            tracks.push(db_track);
            track_index += 1;
        }
    }

    Ok((album, db_release, tracks, artists, album_artists))
}
