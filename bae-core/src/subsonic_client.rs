use serde::Deserialize;

use crate::subsonic::md5_hex;

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "bae";

/// A client for consuming Subsonic-compatible APIs (Navidrome, other bae instances, etc).
pub struct SubsonicClient {
    server_url: String,
    username: String,
    password: String,
    http: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum SubsonicClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server error (code {code}): {message}")]
    Server { code: u32, message: String },
    #[error("unexpected response format")]
    Parse,
}

// -- Response envelope types --

#[derive(Debug, Deserialize)]
struct ResponseEnvelope {
    #[serde(rename = "subsonic-response")]
    subsonic_response: ResponseInner,
}

#[derive(Debug, Deserialize)]
struct ResponseInner {
    status: String,
    #[allow(dead_code)]
    version: Option<String>,
    #[serde(flatten)]
    data: serde_json::Value,
}

// -- Client-side data types (Deserialize, separate from server Serialize types) --

#[derive(Debug, Deserialize, PartialEq)]
pub struct ClientArtist {
    pub id: String,
    pub name: String,
    #[serde(rename = "albumCount", default)]
    pub album_count: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ClientArtistDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub album: Vec<ClientAlbum>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ClientAlbum {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(rename = "artistId", default)]
    pub artist_id: Option<String>,
    #[serde(rename = "songCount", default)]
    pub song_count: u32,
    #[serde(default)]
    pub duration: u32,
    pub year: Option<i32>,
    pub genre: Option<String>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub song: Option<Vec<ClientSong>>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ClientSong {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(rename = "albumId", default)]
    pub album_id: Option<String>,
    #[serde(rename = "artistId", default)]
    pub artist_id: Option<String>,
    pub track: Option<i32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
    pub size: Option<i64>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    pub suffix: Option<String>,
    pub duration: Option<i32>,
    #[serde(rename = "bitRate")]
    pub bit_rate: Option<i32>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ClientSearchResult {
    #[serde(default)]
    pub artist: Vec<ClientArtist>,
    #[serde(default)]
    pub album: Vec<ClientAlbum>,
    #[serde(default)]
    pub song: Vec<ClientSong>,
}

impl SubsonicClient {
    pub fn new(server_url: String, username: String, password: String) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            username,
            password,
            http: reqwest::Client::new(),
        }
    }

    /// Build a full URL with Subsonic auth query params (token-salt method).
    fn build_url(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> String {
        let salt = generate_salt();
        let token = md5_hex(&format!("{}{}", self.password, salt));

        let mut url = format!("{}{}", self.server_url, endpoint);
        url.push_str(&format!(
            "?u={}&t={}&s={}&v={}&c={}&f=json",
            urlencoding::encode(&self.username),
            token,
            salt,
            API_VERSION,
            CLIENT_NAME,
        ));

        for (key, value) in extra_params {
            url.push('&');
            url.push_str(key);
            url.push('=');
            url.push_str(&urlencoding::encode(value));
        }

        url
    }

    /// Fetch a URL and parse the Subsonic response envelope, returning the inner data.
    async fn request(&self, url: &str) -> Result<serde_json::Value, SubsonicClientError> {
        let resp = self.http.get(url).send().await?.error_for_status()?;
        let envelope: ResponseEnvelope = resp.json().await?;
        let inner = envelope.subsonic_response;

        if inner.status != "ok" {
            let error = inner.data.get("error");
            let code = error
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u32;
            let message = error
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            return Err(SubsonicClientError::Server { code, message });
        }

        Ok(inner.data)
    }

    pub async fn ping(&self) -> Result<(), SubsonicClientError> {
        let url = self.build_url("/rest/ping", &[]);
        self.request(&url).await?;
        Ok(())
    }

    pub async fn get_artists(&self) -> Result<Vec<ClientArtist>, SubsonicClientError> {
        let url = self.build_url("/rest/getArtists", &[]);
        let data = self.request(&url).await?;

        // Response: {"artists": {"index": [{"name": "A", "artist": [...]}]}}
        let indices = data
            .get("artists")
            .and_then(|a| a.get("index"))
            .and_then(|i| i.as_array());

        let Some(indices) = indices else {
            return Ok(Vec::new());
        };

        let mut artists = Vec::new();
        for index in indices {
            if let Some(arr) = index.get("artist").and_then(|a| a.as_array()) {
                for artist_val in arr {
                    let artist: ClientArtist = serde_json::from_value(artist_val.clone())
                        .map_err(|_| SubsonicClientError::Parse)?;
                    artists.push(artist);
                }
            }
        }

        Ok(artists)
    }

    pub async fn get_artist(&self, id: &str) -> Result<ClientArtistDetail, SubsonicClientError> {
        let url = self.build_url("/rest/getArtist", &[("id", id)]);
        let data = self.request(&url).await?;

        let artist_val = data.get("artist").ok_or(SubsonicClientError::Parse)?;
        serde_json::from_value(artist_val.clone()).map_err(|_| SubsonicClientError::Parse)
    }

    pub async fn get_album(&self, id: &str) -> Result<ClientAlbum, SubsonicClientError> {
        let url = self.build_url("/rest/getAlbum", &[("id", id)]);
        let data = self.request(&url).await?;

        let album_val = data.get("album").ok_or(SubsonicClientError::Parse)?;
        serde_json::from_value(album_val.clone()).map_err(|_| SubsonicClientError::Parse)
    }

    pub async fn get_album_list(
        &self,
        list_type: &str,
        size: u32,
        offset: u32,
    ) -> Result<Vec<ClientAlbum>, SubsonicClientError> {
        let size_str = size.to_string();
        let offset_str = offset.to_string();
        let url = self.build_url(
            "/rest/getAlbumList2",
            &[
                ("type", list_type),
                ("size", &size_str),
                ("offset", &offset_str),
            ],
        );
        let data = self.request(&url).await?;

        // Response: {"albumList2": {"album": [...]}}  (note: getAlbumList2 uses "albumList2")
        // Some servers use "albumList" instead, so try both.
        // Servers may omit the "album" key entirely when the list is empty.
        let album_arr = data
            .get("albumList2")
            .or_else(|| data.get("albumList"))
            .and_then(|al| al.get("album"))
            .and_then(|a| a.as_array());

        match album_arr {
            Some(arr) => arr
                .iter()
                .map(|v| serde_json::from_value(v.clone()).map_err(|_| SubsonicClientError::Parse))
                .collect(),
            None => Ok(Vec::new()),
        }
    }

    /// Build a streaming URL for a song. Does not make a network request.
    pub fn stream_url(&self, id: &str) -> String {
        self.build_url("/rest/stream", &[("id", id)])
    }

    /// Build a cover art URL. Does not make a network request.
    pub fn get_cover_art_url(&self, id: &str, size: Option<u32>) -> String {
        match size {
            Some(s) => {
                let size_str = s.to_string();
                self.build_url("/rest/getCoverArt", &[("id", id), ("size", &size_str)])
            }
            None => self.build_url("/rest/getCoverArt", &[("id", id)]),
        }
    }

    pub async fn search(&self, query: &str) -> Result<ClientSearchResult, SubsonicClientError> {
        let url = self.build_url("/rest/search3", &[("query", query)]);
        let data = self.request(&url).await?;

        match data.get("searchResult3") {
            Some(result_val) => {
                serde_json::from_value(result_val.clone()).map_err(|_| SubsonicClientError::Parse)
            }
            None => Ok(ClientSearchResult {
                artist: Vec::new(),
                album: Vec::new(),
                song: Vec::new(),
            }),
        }
    }
}

/// Generate a random alphanumeric salt string.
fn generate_salt() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..16)
        .map(|_| {
            let idx = rng.random_range(0..36u8);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_has_correct_structure() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.build_url("/rest/ping", &[]);

        assert!(url.starts_with("http://localhost:4533/rest/ping?"));
        assert!(url.contains("u=admin"));
        assert!(url.contains("v=1.16.1"));
        assert!(url.contains("c=bae"));
        assert!(url.contains("f=json"));
        assert!(url.contains("t="));
        assert!(url.contains("s="));
    }

    #[test]
    fn build_url_includes_extra_params() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.build_url("/rest/getAlbum", &[("id", "album-123")]);

        assert!(url.contains("id=album-123"));
    }

    #[test]
    fn build_url_encodes_special_characters() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "user name".into(),
            "pass".into(),
        );
        let url = client.build_url("/rest/search3", &[("query", "hello world")]);

        assert!(url.contains("u=user%20name"));
        assert!(url.contains("query=hello%20world"));
    }

    #[test]
    fn build_url_strips_trailing_slash_from_server() {
        let client = SubsonicClient::new(
            "http://localhost:4533/".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.build_url("/rest/ping", &[]);

        assert!(url.starts_with("http://localhost:4533/rest/ping?"));
    }

    #[test]
    fn build_url_token_matches_md5_of_password_plus_salt() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "secret".into(),
        );
        let url = client.build_url("/rest/ping", &[]);

        // Extract t= and s= from the URL
        let params: std::collections::HashMap<String, String> = url
            .split_once('?')
            .unwrap()
            .1
            .split('&')
            .filter_map(|p| {
                let (k, v) = p.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect();

        let token = params.get("t").expect("missing t param");
        let salt = params.get("s").expect("missing s param");

        let expected = md5_hex(&format!("secret{}", salt));
        assert_eq!(token, &expected);
    }

    #[test]
    fn stream_url_returns_url_without_network_call() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.stream_url("song-42");

        assert!(url.starts_with("http://localhost:4533/rest/stream?"));
        assert!(url.contains("id=song-42"));
    }

    #[test]
    fn cover_art_url_with_size() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.get_cover_art_url("album-1", Some(300));

        assert!(url.contains("/rest/getCoverArt?"));
        assert!(url.contains("id=album-1"));
        assert!(url.contains("size=300"));
    }

    #[test]
    fn cover_art_url_without_size() {
        let client = SubsonicClient::new(
            "http://localhost:4533".into(),
            "admin".into(),
            "pass".into(),
        );
        let url = client.get_cover_art_url("album-1", None);

        assert!(url.contains("/rest/getCoverArt?"));
        assert!(url.contains("id=album-1"));
        assert!(!url.contains("size="));
    }

    // -- Response parsing tests --

    fn wrap_ok(extra: serde_json::Value) -> String {
        let mut inner = serde_json::json!({
            "status": "ok",
            "version": "1.16.1",
            "type": "navidrome",
            "serverVersion": "0.52.0"
        });
        if let (Some(base), Some(ext)) = (inner.as_object_mut(), extra.as_object()) {
            for (k, v) in ext {
                base.insert(k.clone(), v.clone());
            }
        }
        serde_json::json!({ "subsonic-response": inner }).to_string()
    }

    /// Parse a response envelope, asserting status == "ok" and returning the inner data.
    fn parse_envelope(json: &str) -> Result<serde_json::Value, SubsonicClientError> {
        let envelope: ResponseEnvelope =
            serde_json::from_str(json).map_err(|_| SubsonicClientError::Parse)?;
        let inner = envelope.subsonic_response;

        if inner.status != "ok" {
            let error = inner.data.get("error");
            let code = error
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u32;
            let message = error
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            return Err(SubsonicClientError::Server { code, message });
        }

        Ok(inner.data)
    }

    #[test]
    fn parse_ping_response() {
        let json = wrap_ok(serde_json::json!({}));
        let data = parse_envelope(&json).unwrap();
        // Ping has no meaningful data, just status=ok
        assert!(data.is_object());
    }

    #[test]
    fn parse_error_response() {
        let json = serde_json::json!({
            "subsonic-response": {
                "status": "failed",
                "version": "1.16.1",
                "error": {
                    "code": 40,
                    "message": "Wrong username or password"
                }
            }
        })
        .to_string();

        let err = parse_envelope(&json).unwrap_err();
        match err {
            SubsonicClientError::Server { code, message } => {
                assert_eq!(code, 40);
                assert_eq!(message, "Wrong username or password");
            }
            other => panic!("expected Server error, got {:?}", other),
        }
    }

    #[test]
    fn parse_artists_response() {
        let json = wrap_ok(serde_json::json!({
            "artists": {
                "index": [
                    {
                        "name": "B",
                        "artist": [
                            {"id": "a1", "name": "Beatles", "albumCount": 12},
                            {"id": "a2", "name": "Bach", "albumCount": 3}
                        ]
                    },
                    {
                        "name": "M",
                        "artist": [
                            {"id": "a3", "name": "Mozart", "albumCount": 5}
                        ]
                    }
                ]
            }
        }));

        let data = parse_envelope(&json).unwrap();
        let indices = data["artists"]["index"].as_array().unwrap();

        let mut artists: Vec<ClientArtist> = Vec::new();
        for index in indices {
            for artist_val in index["artist"].as_array().unwrap() {
                artists.push(serde_json::from_value(artist_val.clone()).unwrap());
            }
        }

        assert_eq!(artists.len(), 3);
        assert_eq!(artists[0].name, "Beatles");
        assert_eq!(artists[0].album_count, 12);
        assert_eq!(artists[2].name, "Mozart");
    }

    #[test]
    fn parse_artist_detail_response() {
        let json = wrap_ok(serde_json::json!({
            "artist": {
                "id": "a1",
                "name": "Beatles",
                "album": [
                    {"id": "al1", "name": "Abbey Road", "songCount": 17, "duration": 2834, "year": 1969},
                    {"id": "al2", "name": "Let It Be", "songCount": 12, "duration": 2100, "year": 1970}
                ]
            }
        }));

        let data = parse_envelope(&json).unwrap();
        let detail: ClientArtistDetail = serde_json::from_value(data["artist"].clone()).unwrap();

        assert_eq!(detail.id, "a1");
        assert_eq!(detail.name, "Beatles");
        assert_eq!(detail.album.len(), 2);
        assert_eq!(detail.album[0].name, "Abbey Road");
        assert_eq!(detail.album[0].year, Some(1969));
    }

    #[test]
    fn parse_album_response() {
        let json = wrap_ok(serde_json::json!({
            "album": {
                "id": "al1",
                "name": "Abbey Road",
                "artist": "Beatles",
                "artistId": "a1",
                "songCount": 2,
                "duration": 500,
                "year": 1969,
                "coverArt": "al-al1",
                "song": [
                    {
                        "id": "s1",
                        "title": "Come Together",
                        "album": "Abbey Road",
                        "artist": "Beatles",
                        "albumId": "al1",
                        "artistId": "a1",
                        "track": 1,
                        "year": 1969,
                        "duration": 259,
                        "bitRate": 320,
                        "size": 10383072,
                        "contentType": "audio/flac",
                        "suffix": "flac",
                        "path": "Beatles/Abbey Road/01 - Come Together.flac"
                    },
                    {
                        "id": "s2",
                        "title": "Something",
                        "album": "Abbey Road",
                        "artist": "Beatles",
                        "albumId": "al1",
                        "artistId": "a1",
                        "track": 2,
                        "year": 1969,
                        "duration": 182,
                        "bitRate": 320,
                        "size": 7284000,
                        "contentType": "audio/flac",
                        "suffix": "flac",
                        "path": "Beatles/Abbey Road/02 - Something.flac"
                    }
                ]
            }
        }));

        let data = parse_envelope(&json).unwrap();
        let album: ClientAlbum = serde_json::from_value(data["album"].clone()).unwrap();

        assert_eq!(album.id, "al1");
        assert_eq!(album.name, "Abbey Road");
        assert_eq!(album.artist, Some("Beatles".to_string()));
        assert_eq!(album.song_count, 2);
        assert_eq!(album.year, Some(1969));

        let songs = album.song.unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].title, "Come Together");
        assert_eq!(songs[0].track, Some(1));
        assert_eq!(songs[0].duration, Some(259));
        assert_eq!(songs[1].title, "Something");
    }

    #[test]
    fn parse_album_list_response() {
        let json = wrap_ok(serde_json::json!({
            "albumList2": {
                "album": [
                    {"id": "al1", "name": "Abbey Road", "songCount": 17, "duration": 2834},
                    {"id": "al2", "name": "Let It Be", "songCount": 12, "duration": 2100}
                ]
            }
        }));

        let data = parse_envelope(&json).unwrap();
        let album_arr = data["albumList2"]["album"].as_array().unwrap();
        let albums: Vec<ClientAlbum> = album_arr
            .iter()
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .collect();

        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].name, "Abbey Road");
        assert_eq!(albums[1].name, "Let It Be");
    }

    #[test]
    fn parse_search_response() {
        let json = wrap_ok(serde_json::json!({
            "searchResult3": {
                "artist": [
                    {"id": "a1", "name": "Beatles", "albumCount": 12}
                ],
                "album": [
                    {"id": "al1", "name": "Abbey Road", "songCount": 17, "duration": 2834}
                ],
                "song": [
                    {"id": "s1", "title": "Come Together", "album": "Abbey Road", "artist": "Beatles", "duration": 259}
                ]
            }
        }));

        let data = parse_envelope(&json).unwrap();
        let result: ClientSearchResult =
            serde_json::from_value(data["searchResult3"].clone()).unwrap();

        assert_eq!(result.artist.len(), 1);
        assert_eq!(result.artist[0].name, "Beatles");
        assert_eq!(result.album.len(), 1);
        assert_eq!(result.album[0].name, "Abbey Road");
        assert_eq!(result.song.len(), 1);
        assert_eq!(result.song[0].title, "Come Together");
    }

    #[test]
    fn parse_search_with_empty_results() {
        let json = wrap_ok(serde_json::json!({
            "searchResult3": {}
        }));

        let data = parse_envelope(&json).unwrap();
        let result: ClientSearchResult =
            serde_json::from_value(data["searchResult3"].clone()).unwrap();

        assert!(result.artist.is_empty());
        assert!(result.album.is_empty());
        assert!(result.song.is_empty());
    }

    #[test]
    fn generate_salt_is_16_chars_alphanumeric() {
        for _ in 0..10 {
            let salt = generate_salt();
            assert_eq!(salt.len(), 16);
            assert!(salt.chars().all(|c| c.is_ascii_alphanumeric()));
        }
    }
}
