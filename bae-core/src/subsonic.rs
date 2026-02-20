use crate::library::LibraryError;
use crate::library::SharedLibraryManager;
use crate::library_dir::LibraryDir;
use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};
/// Subsonic API server state
#[derive(Clone)]
pub struct SubsonicState {
    pub library_manager: SharedLibraryManager,
    pub encryption_service: Option<crate::encryption::EncryptionService>,
    pub library_dir: LibraryDir,
    pub key_service: crate::keys::KeyService,
    pub auth: SubsonicAuth,
}

/// Subsonic authentication configuration
#[derive(Clone)]
pub struct SubsonicAuth {
    pub enabled: bool,
    pub username: Option<String>,
    /// Raw password (stored securely in keyring; kept in memory for token auth verification)
    pub password: Option<String>,
}
/// Common query parameters for Subsonic API
#[derive(Debug, Deserialize)]
pub struct SubsonicQuery {
    /// Username
    #[serde(default)]
    pub u: Option<String>,
    /// Password (plaintext or hex-encoded with "enc:" prefix)
    #[serde(default)]
    pub p: Option<String>,
    /// Authentication token: md5(password + salt)
    #[serde(default)]
    pub t: Option<String>,
    /// Salt for token-based auth
    #[serde(default)]
    pub s: Option<String>,
}
/// Standard Subsonic API response envelope
#[derive(Debug, Serialize)]
pub struct SubsonicResponse<T> {
    #[serde(rename = "subsonic-response")]
    pub subsonic_response: SubsonicResponseInner<T>,
}
#[derive(Debug, Serialize)]
pub struct SubsonicResponseInner<T> {
    pub status: String,
    pub version: String,
    #[serde(flatten)]
    pub data: T,
}
/// Error response for Subsonic API
#[derive(Debug, Serialize)]
pub struct SubsonicError {
    pub code: u32,
    pub message: String,
}
/// License info (always valid for open source)
#[derive(Debug, Serialize)]
pub struct License {
    pub valid: bool,
    pub email: String,
    pub key: String,
}
/// Artist info for browsing
#[derive(Debug, Serialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(rename = "albumCount")]
    pub album_count: u32,
}
/// Album info for browsing
#[derive(Debug, Serialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    #[serde(rename = "songCount")]
    pub song_count: u32,
    pub duration: u32,
    pub year: Option<i32>,
    pub genre: Option<String>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
}
/// Song/track info for browsing
#[derive(Debug, Serialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub album: String,
    pub artist: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub track: Option<i32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
    pub size: Option<i64>,
    #[serde(rename = "contentType")]
    pub content_type: String,
    pub suffix: String,
    pub duration: Option<i32>,
    #[serde(rename = "bitRate")]
    pub bit_rate: Option<i32>,
    pub path: String,
}
/// Artists index response
#[derive(Debug, Serialize)]
pub struct ArtistsResponse {
    pub artists: ArtistsIndex,
}
#[derive(Debug, Serialize)]
pub struct ArtistsIndex {
    pub index: Vec<ArtistIndex>,
}
#[derive(Debug, Serialize)]
pub struct ArtistIndex {
    pub name: String,
    pub artist: Vec<Artist>,
}
/// Albums response
#[derive(Debug, Serialize)]
pub struct AlbumListResponse {
    #[serde(rename = "albumList")]
    pub album_list: AlbumList,
}
#[derive(Debug, Serialize)]
pub struct AlbumList {
    pub album: Vec<Album>,
}
/// Create the Subsonic API router
pub fn create_router(
    library_manager: SharedLibraryManager,
    encryption_service: Option<crate::encryption::EncryptionService>,
    library_dir: LibraryDir,
    key_service: crate::keys::KeyService,
    auth: SubsonicAuth,
) -> Router {
    let state = SubsonicState {
        library_manager,
        encryption_service,
        library_dir,
        key_service,
        auth: auth.clone(),
    };
    let auth = Arc::new(auth);
    Router::new()
        .route("/rest/ping", get(ping))
        .route("/rest/getLicense", get(get_license))
        .route("/rest/getArtists", get(get_artists))
        .route("/rest/getAlbumList", get(get_album_list))
        .route("/rest/getAlbum", get(get_album))
        .route("/rest/getCoverArt", get(get_cover_art))
        .route("/rest/stream", get(stream_song))
        .layer(middleware::from_fn(move |req, next| {
            let auth = auth.clone();
            auth_middleware(auth, req, next)
        }))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
/// Compute the MD5 hex digest of a string.
pub(crate) fn md5_hex(input: &str) -> String {
    use md5::Digest;
    let hash = md5::Md5::digest(input.as_bytes());
    hex::encode(hash)
}

/// Validate Subsonic authentication credentials against the configured auth.
///
/// Returns Ok(()) on success, or an error message on failure.
///
/// Supports two auth modes per the Subsonic API spec:
/// - Password: `p` param (plaintext or hex-encoded with "enc:" prefix)
/// - Token+salt: `t` = md5(password + salt), `s` = salt
///
/// Token auth requires the server to know the raw password (not just its hash),
/// which is why we store the raw password in the keyring rather than an MD5 hash.
pub fn validate_auth(auth: &SubsonicAuth, query: &SubsonicQuery) -> Result<(), &'static str> {
    if !auth.enabled {
        return Ok(());
    }

    let expected_username = match &auth.username {
        Some(u) => u,
        None => return Err("Server authentication is misconfigured"),
    };

    let expected_password = match &auth.password {
        Some(p) => p,
        None => return Err("Server authentication is misconfigured"),
    };

    let username = match &query.u {
        Some(u) => u,
        None => return Err("Wrong username or password"),
    };

    if username != expected_username {
        return Err("Wrong username or password");
    }

    // Token-based auth: client sends t = md5(password + salt), s = salt
    if let (Some(token), Some(salt)) = (&query.t, &query.s) {
        let expected_token = md5_hex(&format!("{}{}", expected_password, salt));
        if token == &expected_token {
            return Ok(());
        }

        return Err("Wrong username or password");
    }

    // Password-based auth: p = password (optionally hex-encoded with "enc:" prefix)
    if let Some(password) = &query.p {
        let raw_password = if let Some(hex_encoded) = password.strip_prefix("enc:") {
            match hex::decode(hex_encoded) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(_) => return Err("Wrong username or password"),
            }
        } else {
            password.clone()
        };

        if raw_password == *expected_password {
            return Ok(());
        }

        return Err("Wrong username or password");
    }

    Err("Wrong username or password")
}

/// Axum middleware that checks Subsonic authentication on every request.
async fn auth_middleware(auth: Arc<SubsonicAuth>, req: Request, next: Next) -> Response {
    if !auth.enabled {
        return next.run(req).await;
    }

    // Parse query string for auth params
    let query_string = req.uri().query().unwrap_or("");
    let query: SubsonicQuery = match serde_urlencoded::from_str(query_string) {
        Ok(q) => q,
        Err(_) => {
            return auth_error_response("Missing authentication parameters");
        }
    };

    if let Err(message) = validate_auth(&auth, &query) {
        return auth_error_response(message);
    }

    next.run(req).await
}

/// Build a Subsonic error response for authentication failures.
fn auth_error_response(message: &str) -> Response {
    let error = SubsonicError {
        code: 40,
        message: message.to_string(),
    };
    let response = SubsonicResponse {
        subsonic_response: SubsonicResponseInner {
            status: "failed".to_string(),
            version: "1.16.1".to_string(),
            data: serde_json::json!({ "error": error }),
        },
    };
    (StatusCode::UNAUTHORIZED, Json(response)).into_response()
}

/// Ping endpoint - basic connectivity test
/// Ping endpoint - params required by Subsonic API spec but not used for simple health check
async fn ping(Query(_params): Query<SubsonicQuery>) -> impl IntoResponse {
    let response = SubsonicResponse {
        subsonic_response: SubsonicResponseInner {
            status: "ok".to_string(),
            version: "1.16.1".to_string(),
            data: serde_json::json!({}),
        },
    };
    Json(response)
}
/// Get license info - always return valid for open source
/// params required by Subsonic API spec but not used in this endpoint
async fn get_license(Query(_params): Query<SubsonicQuery>) -> impl IntoResponse {
    let license = License {
        valid: true,
        email: "opensource@bae.music".to_string(),
        key: "bae-open-source".to_string(),
    };
    let response = SubsonicResponse {
        subsonic_response: SubsonicResponseInner {
            status: "ok".to_string(),
            version: "1.16.1".to_string(),
            data: serde_json::json!({ "license" : license }),
        },
    };
    Json(response)
}
/// Get artists index
/// params required by Subsonic API spec but not currently validated
async fn get_artists(
    Query(_params): Query<SubsonicQuery>,
    State(state): State<SubsonicState>,
) -> impl IntoResponse {
    match load_artists(&state.library_manager).await {
        Ok(artists_response) => {
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "ok".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!(artists_response),
                },
            };
            Json(response).into_response()
        }
        Err(e) => {
            let error = SubsonicError {
                code: 0,
                message: format!("Failed to load artists: {}", e),
            };
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "failed".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!({ "error" : error }),
                },
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
        }
    }
}
/// Get album list
/// params required by Subsonic API spec but not currently validated
async fn get_album_list(
    Query(_params): Query<SubsonicQuery>,
    State(state): State<SubsonicState>,
) -> impl IntoResponse {
    match load_albums(&state.library_manager).await {
        Ok(album_response) => {
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "ok".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!(album_response),
                },
            };
            Json(response).into_response()
        }
        Err(e) => {
            let error = SubsonicError {
                code: 0,
                message: format!("Failed to load albums: {}", e),
            };
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "failed".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!({ "error" : error }),
                },
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
        }
    }
}
/// Get album with tracks
async fn get_album(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SubsonicState>,
) -> impl IntoResponse {
    let album_id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            let error = SubsonicError {
                code: 10,
                message: "Required parameter 'id' missing".to_string(),
            };
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "failed".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!({ "error" : error }),
                },
            };
            return (StatusCode::BAD_REQUEST, Json(response)).into_response();
        }
    };
    match load_album_with_songs(&state.library_manager, &album_id).await {
        Ok(album_response) => {
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "ok".to_string(),
                    version: "1.16.1".to_string(),
                    data: album_response,
                },
            };
            Json(response).into_response()
        }
        Err(e) => {
            let error = SubsonicError {
                code: 70,
                message: format!("Album not found: {}", e),
            };
            let response = SubsonicResponse {
                subsonic_response: SubsonicResponseInner {
                    status: "failed".to_string(),
                    version: "1.16.1".to_string(),
                    data: serde_json::json!({ "error" : error }),
                },
            };
            (StatusCode::NOT_FOUND, Json(response)).into_response()
        }
    }
}
/// Get cover art for an album
async fn get_cover_art(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SubsonicState>,
) -> impl IntoResponse {
    let album_id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing id parameter").into_response();
        }
    };

    // Look up the album to find its cover_release_id
    let albums = match state.library_manager.get().get_albums(&[]).await {
        Ok(albums) => albums,
        Err(e) => {
            error!("Failed to load albums for cover art: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let db_album = match albums.into_iter().find(|a| a.id == album_id) {
        Some(album) => album,
        None => {
            return (StatusCode::NOT_FOUND, "Album not found").into_response();
        }
    };

    let release_id = match db_album.cover_release_id {
        Some(id) => id,
        None => {
            return (StatusCode::NOT_FOUND, "No cover art available").into_response();
        }
    };

    let image_path = state.library_dir.image_path(&release_id);

    match tokio::fs::read(&image_path).await {
        Ok(data) => {
            let content_type = state
                .library_manager
                .get()
                .get_library_image(&release_id, &crate::db::LibraryImageType::Cover)
                .await
                .ok()
                .flatten()
                .map(|img| img.content_type.as_str().to_string())
                .unwrap_or_else(|| "image/jpeg".to_string());

            (
                StatusCode::OK,
                [("Content-Type", content_type.as_str())],
                data,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Cover art file not found").into_response(),
    }
}

/// What kind of audio source we resolved for a track.
enum TrackAudioSource {
    /// Local unencrypted file with no byte-range processing needed.
    /// Can be streamed directly from disk without loading into memory.
    DirectFile {
        path: PathBuf,
        content_type: crate::content_type::ContentType,
        original_filename: String,
    },
    /// Processed audio data (decrypted, byte-range sliced, headers prepended).
    /// Already in memory, serve as a single body.
    Buffered {
        data: Vec<u8>,
        content_type: crate::content_type::ContentType,
        original_filename: String,
    },
}

/// Pre-fetched DB data needed to stream a track.
struct TrackLookup {
    audio_format: crate::db::DbAudioFormat,
    release: crate::db::DbRelease,
    audio_file: crate::db::DbFile,
}

/// Fetch all DB data needed to stream a track (audio_format, release, file).
async fn lookup_track(
    library_manager: &SharedLibraryManager,
    track_id: &str,
) -> Result<TrackLookup, Box<dyn std::error::Error + Send + Sync>> {
    let audio_format = library_manager
        .get()
        .get_audio_format_by_track_id(track_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("No audio format found for track {}", track_id))?;

    let track = library_manager
        .get()
        .get_track(track_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Track not found: {}", track_id))?;

    let release = library_manager
        .get()
        .database()
        .get_release_by_id(&track.release_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Release not found: {}", track.release_id))?;

    let audio_file = match &audio_format.file_id {
        Some(file_id) => library_manager
            .get()
            .get_file_by_id(file_id)
            .await
            .map_err(|e| format!("Database error: {}", e))?
            .ok_or_else(|| format!("Audio file not found: {}", file_id))?,
        None => {
            warn!(
                "Track {} has no file_id, falling back to release file scan",
                track_id
            );

            let files = library_manager
                .get()
                .get_files_for_release(&track.release_id)
                .await
                .map_err(|e| format!("Database error: {}", e))?;

            files
                .into_iter()
                .find(|f| f.content_type.is_audio())
                .ok_or_else(|| format!("No audio file found for track {}", track_id))?
        }
    };

    Ok(TrackLookup {
        audio_format,
        release,
        audio_file,
    })
}

/// Derive the filesystem path for a file based on its release's storage flags.
fn resolve_file_path(
    file: &crate::db::DbFile,
    release: &crate::db::DbRelease,
    library_dir: &LibraryDir,
) -> Option<PathBuf> {
    if release.managed_locally {
        Some(file.local_storage_path(library_dir))
    } else {
        release
            .unmanaged_path
            .as_ref()
            .map(|p| std::path::Path::new(p).join(&file.original_filename))
    }
}

/// Resolve how to serve a track's audio: either stream directly from a local
/// file or fall back to the full buffer-and-process pipeline.
async fn resolve_track_audio(
    state: &SubsonicState,
    track_id: &str,
) -> Result<TrackAudioSource, Box<dyn std::error::Error + Send + Sync>> {
    let lookup = lookup_track(&state.library_manager, track_id).await?;

    let is_encrypted = lookup.release.managed_locally && state.encryption_service.is_some();
    let needs_byte_slicing = lookup.audio_format.start_byte_offset.is_some()
        && lookup.audio_format.end_byte_offset.is_some();
    let needs_headers =
        lookup.audio_format.needs_headers && lookup.audio_format.flac_headers.is_some();
    let original_filename = lookup.audio_file.original_filename.clone();

    // Fast path: local, unencrypted, no processing needed -> stream from disk
    if !is_encrypted && !needs_byte_slicing && !needs_headers {
        let source_path =
            resolve_file_path(&lookup.audio_file, &lookup.release, &state.library_dir);
        if let Some(source_path) = source_path {
            debug!(
                "Fast path: streaming directly from {}",
                source_path.display()
            );
            return Ok(TrackAudioSource::DirectFile {
                path: source_path,
                content_type: lookup.audio_format.content_type,
                original_filename,
            });
        }
    }

    // Slow path: need decryption, byte-range slicing, or header prepend
    debug!(
        "Buffered path: encrypted={}, byte_slicing={}, headers={}",
        is_encrypted, needs_byte_slicing, needs_headers
    );

    let (data, content_type) = buffer_track_audio(state, lookup).await?;
    Ok(TrackAudioSource::Buffered {
        data,
        content_type,
        original_filename,
    })
}

/// Sanitize a filename for use in Content-Disposition headers.
/// Replaces characters that break the quoted-string production (RFC 6266).
fn sanitize_content_disposition_filename(name: &str) -> String {
    name.replace(['\\', '"'], "_")
}

/// Stream a song - read and decrypt audio file from storage.
async fn stream_song(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SubsonicState>,
) -> Response {
    let song_id = match params.get("id") {
        Some(id) => id.clone(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing song ID").into_response();
        }
    };

    let is_download = params.get("download").map(|v| v == "true").unwrap_or(false);

    info!("Streaming request for song ID: {}", song_id);

    match resolve_track_audio(&state, &song_id).await {
        Ok(TrackAudioSource::DirectFile {
            path,
            content_type,
            original_filename,
        }) => match tokio::fs::File::open(&path).await {
            Ok(file) => {
                let metadata = file.metadata().await;
                let stream = ReaderStream::new(file);
                let body = Body::from_stream(stream);

                let mut builder = Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", content_type.as_str())
                    .header("Accept-Ranges", "bytes");

                if let Ok(meta) = metadata {
                    builder = builder.header("Content-Length", meta.len().to_string());
                }

                if is_download {
                    builder = builder.header(
                        "Content-Disposition",
                        format!(
                            "attachment; filename=\"{}\"",
                            sanitize_content_disposition_filename(&original_filename),
                        ),
                    );
                }

                builder.body(body).unwrap()
            }
            Err(e) => {
                error!("Failed to open file {:?}: {}", path, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to open file: {}", e),
                )
                    .into_response()
            }
        },
        Ok(TrackAudioSource::Buffered {
            data,
            content_type,
            original_filename,
        }) => {
            let content_length = data.len().to_string();

            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type.as_str())
                .header("Content-Length", content_length)
                .header("Accept-Ranges", "bytes");

            if is_download {
                builder = builder.header(
                    "Content-Disposition",
                    format!(
                        "attachment; filename=\"{}\"",
                        sanitize_content_disposition_filename(&original_filename),
                    ),
                );
            }

            builder.body(Body::from(data)).unwrap()
        }
        Err(e) => {
            error!("Streaming error for song {}: {}", song_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Streaming error: {}", e),
            )
                .into_response()
        }
    }
}
/// Load artists from database and group by first letter
async fn load_artists(
    library_manager: &SharedLibraryManager,
) -> Result<ArtistsResponse, LibraryError> {
    let albums = library_manager.get().get_albums(&[]).await?;
    let mut artist_map: HashMap<String, HashMap<String, u32>> = HashMap::new();
    for album in &albums {
        let artists = library_manager
            .get()
            .get_artists_for_album(&album.id)
            .await?;
        for artist in artists {
            let first_letter = artist
                .name
                .chars()
                .next()
                .unwrap_or('A')
                .to_uppercase()
                .to_string();
            let artist_map_entry = artist_map.entry(first_letter).or_default();
            *artist_map_entry.entry(artist.name).or_insert(0) += 1;
        }
    }
    let mut indices = Vec::new();
    for (letter, artists) in artist_map {
        let artist_list: Vec<Artist> = artists
            .into_iter()
            .map(|(name, count)| Artist {
                id: format!("artist_{}", name.replace(' ', "_")),
                name,
                album_count: count,
            })
            .collect();
        if !artist_list.is_empty() {
            indices.push(ArtistIndex {
                name: letter,
                artist: artist_list,
            });
        }
    }
    indices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ArtistsResponse {
        artists: ArtistsIndex { index: indices },
    })
}
/// Load albums from database
async fn load_albums(
    library_manager: &SharedLibraryManager,
) -> Result<AlbumListResponse, LibraryError> {
    let db_albums = library_manager.get().get_albums(&[]).await?;
    let mut albums = Vec::new();
    for db_album in db_albums {
        let tracks = library_manager.get().get_tracks(&db_album.id).await?;
        let artists = library_manager
            .get()
            .get_artists_for_album(&db_album.id)
            .await?;
        let artist_name = if artists.is_empty() {
            "Unknown Artist".to_string()
        } else {
            artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let cover_art = if db_album.cover_release_id.is_some() {
            Some(db_album.id.clone())
        } else {
            None
        };

        albums.push(Album {
            id: db_album.id.clone(),
            name: db_album.title,
            artist: artist_name.clone(),
            artist_id: format!("artist_{}", artist_name.replace(' ', "_")),
            song_count: tracks.len() as u32,
            duration: 0,
            year: db_album.year,
            genre: None,
            cover_art,
        });
    }
    Ok(AlbumListResponse {
        album_list: AlbumList { album: albums },
    })
}
/// Load album with its songs
async fn load_album_with_songs(
    library_manager: &SharedLibraryManager,
    album_id: &str,
) -> Result<serde_json::Value, LibraryError> {
    let albums = library_manager.get().get_albums(&[]).await?;
    let db_album = albums
        .into_iter()
        .find(|a| a.id == album_id)
        .ok_or_else(|| LibraryError::Import("Album not found".to_string()))?;
    let tracks = library_manager.get().get_tracks(album_id).await?;
    let album_artists = library_manager
        .get()
        .get_artists_for_album(&db_album.id)
        .await?;
    let album_artist_name = if album_artists.is_empty() {
        "Unknown Artist".to_string()
    } else {
        album_artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut songs = Vec::new();
    for track in tracks {
        let track_artists = library_manager
            .get()
            .get_artists_for_track(&track.id)
            .await?;
        let track_artist_name = if track_artists.is_empty() {
            album_artist_name.clone()
        } else {
            track_artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let song_cover_art = if db_album.cover_release_id.is_some() {
            Some(db_album.id.clone())
        } else {
            None
        };

        let track_content_type = library_manager
            .get()
            .get_audio_format_by_track_id(&track.id)
            .await?
            .map(|af| af.content_type)
            .unwrap_or(crate::content_type::ContentType::Flac);

        songs.push(Song {
            id: track.id,
            title: track.title,
            album: db_album.title.clone(),
            artist: track_artist_name.clone(),
            album_id: db_album.id.clone(),
            artist_id: format!("artist_{}", track_artist_name.replace(' ', "_")),
            track: track.track_number,
            year: db_album.year,
            genre: None,
            cover_art: song_cover_art,
            size: None,
            content_type: track_content_type.as_str().to_string(),
            suffix: track_content_type.file_extension().to_string(),
            duration: track.duration_ms.map(|ms| (ms / 1000) as i32),
            bit_rate: None,
            path: format!("{}/{}", album_artist_name, db_album.title),
        });
    }

    let album_cover_art = if db_album.cover_release_id.is_some() {
        Some(db_album.id.clone())
    } else {
        None
    };

    let album = Album {
        id: db_album.id.clone(),
        name: db_album.title,
        artist: album_artist_name.clone(),
        artist_id: format!("artist_{}", album_artist_name.replace(' ', "_")),
        song_count: songs.len() as u32,
        duration: songs.iter().map(|s| s.duration.unwrap_or(0) as u32).sum(),
        year: db_album.year,
        genre: None,
        cover_art: album_cover_art,
    };
    Ok(serde_json::json!(
        { "album" : { "id" : album.id, "name" : album.name, "artist" : album.artist,
        "artistId" : album.artist_id, "songCount" : album.song_count, "duration" :
        album.duration, "year" : album.year, "coverArt" : album.cover_art, "song" :
        songs } }
    ))
}
/// Stream track audio - read file and decrypt if needed.
/// Returns audio data and its content type.
pub async fn stream_track_audio(
    state: &SubsonicState,
    track_id: &str,
) -> Result<(Vec<u8>, crate::content_type::ContentType), Box<dyn std::error::Error + Send + Sync>> {
    let lookup = lookup_track(&state.library_manager, track_id).await?;
    buffer_track_audio(state, lookup).await
}

/// Read, decrypt, slice, and assemble track audio from pre-fetched DB data.
async fn buffer_track_audio(
    state: &SubsonicState,
    lookup: TrackLookup,
) -> Result<(Vec<u8>, crate::content_type::ContentType), Box<dyn std::error::Error + Send + Sync>> {
    let TrackLookup {
        audio_format,
        release,
        audio_file,
    } = lookup;

    info!("Loading audio for file: {}", audio_file.id);

    // Derive file path from release storage flags
    let source_path = resolve_file_path(&audio_file, &release, &state.library_dir)
        .ok_or("Cannot determine file path for audio file")?;

    debug!("Reading from local file: {}", source_path.display());
    let file_data = tokio::fs::read(&source_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Decrypt with per-release derived key
    let is_encrypted = release.managed_locally && state.encryption_service.is_some();
    let decrypted = if is_encrypted {
        let enc = state
            .encryption_service
            .as_ref()
            .ok_or("Cannot stream encrypted files: encryption not configured")?;
        let release_enc = enc.derive_release_encryption(&release.id);
        release_enc
            .decrypt(&file_data)
            .map_err(|e| format!("Failed to decrypt file: {}", e))?
    } else {
        file_data
    };

    // For CUE/FLAC tracks, slice to the track's byte range within the shared file
    let track_data = match (audio_format.start_byte_offset, audio_format.end_byte_offset) {
        (Some(start), Some(end)) => {
            let start = start as usize;
            let end = end as usize;

            debug!(
                "Slicing to track byte range: {}..{} ({} bytes of {} total)",
                start,
                end,
                end - start,
                decrypted.len()
            );

            decrypted
                .get(start..end)
                .ok_or_else(|| {
                    format!(
                        "Byte range {}..{} out of bounds for {} byte file",
                        start,
                        end,
                        decrypted.len()
                    )
                })?
                .to_vec()
        }
        _ => decrypted,
    };

    // For CUE/FLAC tracks, prepend headers if needed
    let audio_data = if audio_format.needs_headers {
        if let Some(ref headers) = audio_format.flac_headers {
            debug!("Prepending FLAC headers: {} bytes", headers.len());
            let mut complete_audio = headers.clone();
            complete_audio.extend_from_slice(&track_data);
            complete_audio
        } else {
            track_data
        }
    } else {
        track_data
    };

    info!(
        "Successfully loaded {} bytes of audio data",
        audio_data.len()
    );
    Ok((audio_data, audio_format.content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_enabled(username: &str, password: &str) -> SubsonicAuth {
        SubsonicAuth {
            enabled: true,
            username: Some(username.to_string()),
            password: Some(password.to_string()),
        }
    }

    fn auth_disabled() -> SubsonicAuth {
        SubsonicAuth {
            enabled: false,
            username: None,
            password: None,
        }
    }

    #[test]
    fn auth_disabled_passes_through() {
        let auth = auth_disabled();
        let query = SubsonicQuery {
            u: None,
            p: None,
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_ok());
    }

    #[test]
    fn auth_valid_plaintext_password() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: Some("secret123".to_string()),
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_ok());
    }

    #[test]
    fn auth_invalid_plaintext_password() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: Some("wrong".to_string()),
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_err());
    }

    #[test]
    fn auth_valid_hex_encoded_password() {
        let auth = auth_enabled("admin", "secret123");
        // "secret123" in hex
        let hex_password = hex::encode("secret123");
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: Some(format!("enc:{}", hex_password)),
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_ok());
    }

    #[test]
    fn auth_valid_token_and_salt() {
        let auth = auth_enabled("admin", "secret123");
        let salt = "randomsalt";
        let token = md5_hex(&format!("secret123{}", salt));
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: None,
            t: Some(token),
            s: Some(salt.to_string()),
        };
        assert!(validate_auth(&auth, &query).is_ok());
    }

    #[test]
    fn auth_invalid_token() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: None,
            t: Some("badtoken".to_string()),
            s: Some("somesalt".to_string()),
        };
        assert!(validate_auth(&auth, &query).is_err());
    }

    #[test]
    fn auth_wrong_username() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: Some("hacker".to_string()),
            p: Some("secret123".to_string()),
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_err());
    }

    #[test]
    fn auth_missing_credentials() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: None,
            p: None,
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_err());
    }

    #[test]
    fn auth_missing_password_and_token() {
        let auth = auth_enabled("admin", "secret123");
        let query = SubsonicQuery {
            u: Some("admin".to_string()),
            p: None,
            t: None,
            s: None,
        };
        assert!(validate_auth(&auth, &query).is_err());
    }

    #[test]
    fn md5_hex_produces_correct_hash() {
        // Known MD5 hash for "password"
        assert_eq!(md5_hex("password"), "5f4dcc3b5aa765d61d8327deb882cf99");
    }
}
