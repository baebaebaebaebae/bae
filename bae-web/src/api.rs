use bae_ui::display_types::{Album, Artist, Release, Track, TrackImportState};
use bae_ui::stores::AlbumDetailState;
use serde::Deserialize;
use std::collections::HashMap;

/// Subsonic API response envelope
#[derive(Deserialize)]
struct SubsonicEnvelope {
    #[serde(rename = "subsonic-response")]
    subsonic_response: SubsonicInner,
}

#[derive(Deserialize)]
struct SubsonicInner {
    #[serde(rename = "albumList")]
    album_list: Option<AlbumListData>,
    album: Option<AlbumWithSongs>,
}

#[derive(Deserialize)]
struct AlbumListData {
    album: Vec<SubsonicAlbum>,
}

#[derive(Deserialize)]
struct SubsonicAlbum {
    id: String,
    name: String,
    artist: Option<String>,
    #[serde(rename = "artistId")]
    artist_id: Option<String>,
    year: Option<i32>,
    #[serde(rename = "coverArt")]
    cover_art: Option<String>,
}

#[derive(Deserialize)]
struct AlbumWithSongs {
    id: String,
    name: String,
    artist: Option<String>,
    #[serde(rename = "artistId")]
    artist_id: Option<String>,
    year: Option<i32>,
    #[serde(rename = "coverArt")]
    cover_art: Option<String>,
    song: Option<Vec<SubsonicSong>>,
}

#[derive(Deserialize)]
struct SubsonicSong {
    id: String,
    title: String,
    track: Option<i32>,
    duration: Option<i32>,
}

// -- Cloud share types (decrypted from meta.enc) --

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct CloudShareMeta {
    pub album_name: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover_image_key: Option<String>,
    pub tracks: Vec<CloudShareTrack>,
    pub release_key_b64: String,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct CloudShareTrack {
    pub number: Option<i32>,
    pub title: String,
    pub duration_secs: Option<i64>,
    pub file_key: String,
    pub format: String,
}

fn cover_url_for(cover_art: &Option<String>) -> Option<String> {
    cover_art
        .as_ref()
        .map(|id| format!("/rest/getCoverArt?id={}", id))
}

/// Fetch all albums from the subsonic API
pub async fn fetch_albums() -> Result<(Vec<Album>, HashMap<String, Vec<Artist>>), String> {
    let resp = reqwest::get("/rest/getAlbumList")
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let envelope: SubsonicEnvelope = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

    let subsonic_albums = envelope
        .subsonic_response
        .album_list
        .map(|al| al.album)
        .unwrap_or_default();

    let mut albums = Vec::with_capacity(subsonic_albums.len());
    let mut artists_by_album = HashMap::new();

    for sa in subsonic_albums {
        let artist_name = sa
            .artist
            .clone()
            .unwrap_or_else(|| "Unknown Artist".to_string());
        let artist_id = sa
            .artist_id
            .clone()
            .unwrap_or_else(|| format!("artist_{}", artist_name.replace(' ', "_")));

        artists_by_album.insert(
            sa.id.clone(),
            vec![Artist {
                id: artist_id,
                name: artist_name,
                image_url: None,
            }],
        );

        albums.push(Album {
            id: sa.id,
            title: sa.name,
            year: sa.year,
            cover_url: cover_url_for(&sa.cover_art),
            is_compilation: false,
            date_added: chrono::Utc::now(),
        });
    }

    Ok((albums, artists_by_album))
}

/// Fetch a single album with tracks from the subsonic API
pub async fn fetch_album(album_id: &str) -> Result<AlbumDetailState, String> {
    let url = format!("/rest/getAlbum?id={}", album_id);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let envelope: SubsonicEnvelope = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

    let sa = envelope
        .subsonic_response
        .album
        .ok_or_else(|| "No album in response".to_string())?;

    let artist_name = sa
        .artist
        .clone()
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let artist_id = sa
        .artist_id
        .clone()
        .unwrap_or_else(|| format!("artist_{}", artist_name.replace(' ', "_")));

    let album = Album {
        id: sa.id.clone(),
        title: sa.name,
        year: sa.year,
        cover_url: cover_url_for(&sa.cover_art),
        is_compilation: false,
        date_added: chrono::Utc::now(),
    };

    let artists = vec![Artist {
        id: artist_id,
        name: artist_name,
        image_url: None,
    }];

    let songs = sa.song.unwrap_or_default();
    let tracks: Vec<Track> = songs
        .into_iter()
        .map(|s| Track {
            id: s.id,
            title: s.title,
            track_number: s.track,
            disc_number: Some(1),
            duration_ms: s.duration.map(|d| d as i64 * 1000),
            is_available: true,
            import_state: TrackImportState::Complete,
        })
        .collect();

    let track_count = tracks.len();
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    let track_disc_info: Vec<(Option<i32>, String)> = tracks
        .iter()
        .map(|t| (t.disc_number, t.id.clone()))
        .collect();

    // Create a synthetic release so the detail view has something to show
    let release_id = format!("{}-release", sa.id);
    let releases = vec![Release {
        id: release_id.clone(),
        album_id: sa.id,
        release_name: None,
        year: album.year,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        discogs_release_id: None,
        musicbrainz_release_id: None,
        managed_locally: false,
        managed_in_cloud: false,
        unmanaged_path: None,
    }];

    Ok(AlbumDetailState {
        album: Some(album),
        artists,
        tracks,
        track_count,
        track_ids,
        track_disc_info,
        releases,
        files: vec![],
        images: vec![],
        selected_release_id: Some(release_id),
        loading: false,
        error: None,
        import_progress: None,
        import_error: None,
        managed_locally: false,
        managed_in_cloud: false,
        is_unmanaged: false,
        transfer_progress: None,
        transfer_error: None,
        remote_covers: vec![],
        loading_remote_covers: false,
        share_grant_json: None,
        share_error: None,
        share_link_copied: false,
    })
}

/// Fetch encrypted share metadata from bae-proxy.
pub async fn fetch_share_meta_encrypted(share_id: &str) -> Result<Vec<u8>, String> {
    let url = format!("/share/{share_id}/meta");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Share not found.".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("Server error: {}", resp.status()));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read error: {e}"))
}

/// Fetch an encrypted file via bae-proxy share route.
pub async fn fetch_share_file(share_id: &str, file_key: &str) -> Result<Vec<u8>, String> {
    let url = format!("/share/{share_id}/file/{file_key}");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("File not found.".to_string());
    }
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("Access denied.".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("Server error: {}", resp.status()));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read error: {e}"))
}
