use crate::content_type::ContentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;
const IMPORT_STATUS_QUEUED: &str = "queued";
const IMPORT_STATUS_IMPORTING: &str = "importing";
const IMPORT_STATUS_COMPLETE: &str = "complete";
const IMPORT_STATUS_FAILED: &str = "failed";
/// Database models for bae storage system
///
/// This implements the storage strategy described in the README:
/// - Albums and tracks stored as metadata
/// - Files stored encrypted in cloud or local storage
///
/// Import status for albums and tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ImportStatus {
    Queued,
    Importing,
    Complete,
    Failed,
}
impl ImportStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportStatus::Queued => IMPORT_STATUS_QUEUED,
            ImportStatus::Importing => IMPORT_STATUS_IMPORTING,
            ImportStatus::Complete => IMPORT_STATUS_COMPLETE,
            ImportStatus::Failed => IMPORT_STATUS_FAILED,
        }
    }
}
/// Artist metadata
///
/// Represents an individual artist or band. Artists are linked to albums and tracks
/// via junction tables (album_artists, track_artists) to support:
/// - Multiple artists per album (collaborations)
/// - Different artists per track (compilations, features)
/// - Artist deduplication across imports
///
/// Supports multiple metadata sources:
/// - Discogs: discogs_artist_id for deduplication
/// - Bandcamp: bandcamp_artist_id for future integration
/// - Other sources can be added as needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbArtist {
    pub id: String,
    pub name: String,
    /// Sort name for alphabetical ordering (e.g., "Beatles, The")
    pub sort_name: Option<String>,
    /// Artist ID from Discogs (for deduplication across imports)
    pub discogs_artist_id: Option<String>,
    /// Artist ID from Bandcamp (for future multi-source support)
    pub bandcamp_artist_id: Option<String>,
    /// Artist ID from MusicBrainz (for deduplication across imports)
    pub musicbrainz_artist_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Links artists to albums (many-to-many)
///
/// Supports albums with multiple artists (e.g., collaborations).
/// Position field maintains the order of artists for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAlbumArtist {
    pub id: String,
    pub album_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist albums (0-indexed)
    pub position: i32,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
/// Links artists to tracks (many-to-many)
///
/// Supports tracks with multiple artists (features, remixes, etc.).
/// Role field distinguishes between main artist, featured artist, remixer, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTrackArtist {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist tracks (0-indexed)
    pub position: i32,
    /// Role: "main", "featuring", "remixer", etc.
    pub role: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
/// Discogs release information for an album.
///
/// Not all Discogs releases have a master — master_id is optional.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsMasterRelease {
    pub master_id: Option<String>,
    pub release_id: String,
}
/// MusicBrainz release information for an album
///
/// MusicBrainz has Release Groups (abstract albums) and Releases (specific versions).
/// Similar to Discogs master_id/release_id relationship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MusicBrainzRelease {
    pub release_group_id: String,
    pub release_id: String,
}
/// Album metadata - represents a logical album (the "master")
///
/// A logical album can have multiple physical releases (e.g., "1973 Original", "2016 Remaster").
/// This table stores the high-level album information that's common across all releases.
/// Specific release details and import status are tracked in the `releases` table.
///
/// Artists are linked via the `album_artists` junction table to support multiple artists.
///
/// Supports multiple metadata sources:
/// - Discogs: discogs_release links to the Discogs master release and its main release
/// - Bandcamp: bandcamp_album_id would link to the Bandcamp album
/// - Other sources can be added as needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// Discogs release information
    pub discogs_release: Option<DiscogsMasterRelease>,
    /// MusicBrainz release information
    pub musicbrainz_release: Option<MusicBrainzRelease>,
    /// Album ID from Bandcamp (optional, for future multi-source support)
    pub bandcamp_album_id: Option<String>,
    /// Release ID whose cover art is used for this album
    pub cover_release_id: Option<String>,
    /// True for "Various Artists" compilation albums
    pub is_compilation: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Release metadata - represents a specific version/pressing of an album
///
/// A release is a physical or digital version of a logical album.
/// Examples: "1973 Original Pressing", "2016 Remaster", "180g Vinyl", "Digital Release"
///
/// Files and tracks belong to releases (not albums), because:
/// - Users import specific releases, not abstract albums
/// - Each release has its own audio files and metadata
/// - Multiple releases of the same album can coexist in the library
///
/// The release_name field distinguishes between versions (e.g., "2016 Remaster").
/// If the user doesn't specify a release, we create one with release_name=None.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbRelease {
    pub id: String,
    /// Links to the logical album (DbAlbum)
    pub album_id: String,
    /// Human-readable release name (e.g., "2016 Remaster", "180g Vinyl")
    pub release_name: Option<String>,
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    /// Discogs release ID (optional)
    pub discogs_release_id: Option<String>,
    /// Bandcamp release ID (optional, for future multi-source support)
    pub bandcamp_release_id: Option<String>,
    /// Format (e.g., "CD", "Vinyl", "Digital")
    pub format: Option<String>,
    /// Record label
    pub label: Option<String>,
    /// Catalog number
    pub catalog_number: Option<String>,
    /// Country of release
    pub country: Option<String>,
    /// Barcode
    pub barcode: Option<String>,
    pub import_status: ImportStatus,
    /// Files are in `~/.bae/libraries/{uuid}/storage/ab/cd/{file_id}`
    pub managed_locally: bool,
    /// Files are in cloud home `storage/ab/cd/{file_id}`
    pub managed_in_cloud: bool,
    /// Base folder path for unmanaged files (path = unmanaged_path/original_filename).
    /// Mutually exclusive with managed_locally/managed_in_cloud.
    pub unmanaged_path: Option<String>,
    /// When true, this release is excluded from discovery network participation
    /// (no DHT announces, no attestation sharing).
    pub private: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Track metadata within a release
///
/// Represents a single track on a specific release. Tracks are linked to releases
/// (not logical albums) because track listings can vary between releases.
///
/// Track artists are linked via the `track_artists` junction table to support:
/// - Multiple artists per track (features, collaborations)
/// - Different artists than the album artist (compilations)
///
/// The discogs_position field stores the track position from metadata
/// (e.g., "A1", "1", "1-1" for vinyl sides).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbTrack {
    pub id: String,
    /// Links to the specific release (DbRelease), not the logical album
    pub release_id: String,
    pub title: String,
    /// Disc number (1-indexed) for multi-disc releases
    pub disc_number: Option<i32>,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Position from metadata source (e.g., "A1", "1", "1-1")
    pub discogs_position: Option<String>,
    pub import_status: ImportStatus,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
/// Which encryption key was used for a release file.
/// - `Master`: legacy, encrypted with the library master key directly
/// - `Derived`: per-release key derived via HKDF from the master key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionScheme {
    Master,
    Derived,
}

impl EncryptionScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            EncryptionScheme::Master => "master",
            EncryptionScheme::Derived => "derived",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "derived" => EncryptionScheme::Derived,
            _ => EncryptionScheme::Master,
        }
    }
}

/// Physical file belonging to a release
///
/// Stores original file information needed to reconstruct file structure for export
/// or BitTorrent seeding.
///
/// Files are linked to releases (not logical albums or tracks), because:
/// - Files are part of a specific release (e.g., "2016 Remaster" has different files than "1973 Original")
/// - Some files are metadata (cover.jpg, .cue sheets) not associated with any track
///
/// File location is determined by the parent release's storage flags:
/// - managed_locally: path derived from file_id via `storage_path()`
/// - managed_in_cloud: key derived from file_id via `storage_path()`
/// - unmanaged_path: `unmanaged_path/original_filename`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFile {
    pub id: String,
    /// Release this file belongs to
    pub release_id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub content_type: ContentType,
    /// Encryption nonce (24 bytes) for efficient range decryption.
    /// Only set when file is encrypted with chunked encryption.
    /// Stored at import time, used during seek to avoid fetching nonce from cloud.
    pub encryption_nonce: Option<Vec<u8>>,
    /// Which encryption scheme was used: master key directly, or HKDF-derived per-release key.
    pub encryption_scheme: EncryptionScheme,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
/// Audio format metadata for a track
///
/// Stores format information needed for playback. One record per track (1:1 with track).
///
/// **FLAC headers** are stored for CUE/FLAC tracks where we need to prepend them during playback
/// (track audio starts mid-file, decoder needs headers). Also stored for regular FLAC for seeking.
///
/// **Seektables** enable frame-accurate seeking. Built during import by scanning every FLAC frame
/// (~93ms precision, vs ~10s for embedded seektables). Both byte and sample offsets are track-relative
/// (byte 0, sample 0 = first frame of this track's audio data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAudioFormat {
    pub id: String,
    pub track_id: String,
    pub content_type: ContentType,
    pub flac_headers: Option<Vec<u8>>,
    pub needs_headers: bool,
    /// Start byte offset within the source file (for CUE/FLAC tracks).
    /// Calculated at import time using a dense seektable for frame-accurate positioning.
    pub start_byte_offset: Option<i64>,
    /// End byte offset within the source file (for CUE/FLAC tracks)
    pub end_byte_offset: Option<i64>,
    /// Pre-gap duration in milliseconds (for CUE/FLAC tracks with INDEX 00)
    /// When present, playback starts at INDEX 00 and shows negative time until INDEX 01
    pub pregap_ms: Option<i64>,
    /// Offset in samples from the start of extracted bytes to actual track content.
    /// Due to FLAC frame alignment, extracted bytes start at a frame boundary which may
    /// be up to ~4096 samples before the track's actual start. This offset tells the
    /// decoder how many samples to skip when playing.
    /// Stored as samples (not ms) to avoid rounding errors.
    pub frame_offset_samples: Option<i64>,
    /// Exact number of samples this track should contain (for gapless playback).
    /// After decoding, trim output to this count. Required because FLAC frames
    /// at track boundaries may extend past the logical track end.
    pub exact_sample_count: Option<i64>,
    /// Sample rate in Hz (for time-to-sample conversion during seek)
    pub sample_rate: i64,
    /// Bits per sample (16, 24, etc.)
    pub bits_per_sample: i64,
    /// Dense seektable for frame-accurate seeking.
    /// JSON array of {sample_number, byte_offset} entries built by scanning FLAC frames.
    /// Enables seeking: prepend headers + data from frame boundary.
    pub seektable_json: String,
    /// Byte offset where audio data starts in the file (after headers).
    /// Seektable byte offsets are relative to this position.
    pub audio_data_start: i64,
    /// FK to DbFile containing this track's audio data.
    pub file_id: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
impl DbArtist {
    /// Create an artist from Discogs artist data
    pub fn from_discogs_artist(discogs_artist_id: &str, name: &str) -> Self {
        let now = Utc::now();
        DbArtist {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            sort_name: None,
            discogs_artist_id: Some(discogs_artist_id.to_string()),
            bandcamp_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}
impl DbAlbumArtist {
    pub fn new(album_id: &str, artist_id: &str, position: i32) -> Self {
        let now = Utc::now();
        DbAlbumArtist {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            updated_at: now,
            created_at: now,
        }
    }
}
impl DbTrackArtist {
    pub fn new(track_id: &str, artist_id: &str, position: i32, role: Option<String>) -> Self {
        let now = Utc::now();
        DbTrackArtist {
            id: Uuid::new_v4().to_string(),
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            role,
            updated_at: now,
            created_at: now,
        }
    }
}
impl DbAlbum {
    #[cfg(test)]
    pub fn new_test(title: &str) -> Self {
        let now = chrono::Utc::now();
        DbAlbum {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            year: None,
            discogs_release: None,
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_release_id: None,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        }
    }
    /// Create a logical album from a Discogs release
    /// Note: Artists should be created separately and linked via DbAlbumArtist
    ///
    /// master_id and master_year are always provided for releases imported from Discogs.
    /// The master year is used for the album year (not the release year).
    pub fn from_discogs_release(
        release: &crate::discogs::DiscogsRelease,
        master_year: u32,
        is_compilation: bool,
    ) -> Self {
        let now = Utc::now();
        let discogs_release = DiscogsMasterRelease {
            master_id: release.master_id.clone(),
            release_id: release.id.clone(),
        };
        let artist_name = release
            .artists
            .first()
            .map(|a| a.name.as_str())
            .unwrap_or("");
        let is_compilation = is_compilation || is_various_artists(artist_name);
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: release.title.clone(),
            year: Some(master_year as i32),
            discogs_release: Some(discogs_release),
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_release_id: None,
            is_compilation,
            created_at: now,
            updated_at: now,
        }
    }
    pub fn from_mb_release(
        release: &crate::musicbrainz::MbRelease,
        master_year: u32,
        is_compilation: bool,
    ) -> Self {
        let now = Utc::now();
        let musicbrainz_release = crate::db::MusicBrainzRelease {
            release_group_id: release.release_group_id.clone(),
            release_id: release.release_id.clone(),
        };
        let year = release
            .first_release_date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()))
            .or(Some(master_year as i32));
        let is_compilation = is_compilation || is_various_artists(&release.artist);
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: release.title.clone(),
            year,
            discogs_release: None,
            musicbrainz_release: Some(musicbrainz_release),
            bandcamp_album_id: None,
            cover_release_id: None,
            is_compilation,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Check if an artist name indicates a "Various Artists" compilation
fn is_various_artists(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "various" || lower == "various artists"
}

impl DbRelease {
    #[cfg(test)]
    pub fn new_test(album_id: &str, release_id: &str) -> Self {
        let now = chrono::Utc::now();
        DbRelease {
            id: release_id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year: None,
            discogs_release_id: None,
            bandcamp_release_id: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            import_status: ImportStatus::Queued,
            managed_locally: false,
            managed_in_cloud: false,
            unmanaged_path: None,
            private: false,
            created_at: now,
            updated_at: now,
        }
    }
    /// Create a release from a Discogs release
    pub fn from_discogs_release(album_id: &str, release: &crate::discogs::DiscogsRelease) -> Self {
        let now = Utc::now();
        let format = if release.format.is_empty() {
            None
        } else {
            Some(release.format.join(", "))
        };
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year: release.year.map(|y| y as i32),
            discogs_release_id: Some(release.id.clone()),
            bandcamp_release_id: None,
            format,
            label: release.label.first().cloned(),
            catalog_number: release.catno.clone(),
            country: release.country.clone(),
            barcode: None,
            import_status: ImportStatus::Queued,
            managed_locally: false,
            managed_in_cloud: false,
            unmanaged_path: None,
            private: false,
            created_at: now,
            updated_at: now,
        }
    }
    pub fn from_mb_release(album_id: &str, release: &crate::musicbrainz::MbRelease) -> Self {
        let now = Utc::now();
        let year = release
            .date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()));
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year,
            discogs_release_id: None,
            bandcamp_release_id: None,
            format: release.format.clone(),
            label: release.label.clone(),
            catalog_number: release.catalog_number.clone(),
            country: release.country.clone(),
            barcode: release.barcode.clone(),
            import_status: ImportStatus::Queued,
            managed_locally: false,
            managed_in_cloud: false,
            unmanaged_path: None,
            private: false,
            created_at: now,
            updated_at: now,
        }
    }
}
impl DbTrack {
    #[cfg(test)]
    pub fn new_test(
        release_id: &str,
        track_id: &str,
        title: &str,
        track_number: Option<i32>,
    ) -> Self {
        let now = chrono::Utc::now();
        DbTrack {
            id: track_id.to_string(),
            release_id: release_id.to_string(),
            title: title.to_string(),
            disc_number: None,
            track_number,
            duration_ms: None,
            discogs_position: None,
            import_status: ImportStatus::Queued,
            updated_at: now,
            created_at: now,
        }
    }
    pub fn from_discogs_track(
        discogs_track: &crate::discogs::DiscogsTrack,
        release_id: &str,
        track_index: usize,
        disc_number: Option<i32>,
    ) -> Result<Self, String> {
        let now = Utc::now();
        Ok(DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: discogs_track.title.clone(),
            disc_number,
            track_number: Some((track_index + 1) as i32),
            duration_ms: None,
            discogs_position: Some(discogs_track.position.clone()),
            import_status: ImportStatus::Queued,
            updated_at: now,
            created_at: now,
        })
    }
}
impl DbFile {
    /// Create a file record for export/torrent metadata
    ///
    /// Files are linked to releases. Used for reconstructing original file structure
    /// during export or BitTorrent seeding.
    pub fn new(
        release_id: &str,
        original_filename: &str,
        file_size: i64,
        content_type: ContentType,
    ) -> Self {
        let now = Utc::now();
        DbFile {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            original_filename: original_filename.to_string(),
            file_size,
            content_type,
            encryption_nonce: None,
            encryption_scheme: EncryptionScheme::Master,
            updated_at: now,
            created_at: now,
        }
    }

    /// Set the encryption nonce for efficient encrypted range requests.
    /// The nonce is the first 24 bytes of the encrypted file.
    pub fn with_encryption_nonce(mut self, nonce: Vec<u8>) -> Self {
        self.encryption_nonce = Some(nonce);
        self
    }

    /// Set the encryption scheme (master or derived).
    pub fn with_encryption_scheme(mut self, scheme: EncryptionScheme) -> Self {
        self.encryption_scheme = scheme;
        self
    }

    /// Derive the local storage path for this file.
    pub fn local_storage_path(
        &self,
        library_dir: &crate::library_dir::LibraryDir,
    ) -> std::path::PathBuf {
        library_dir.join(crate::storage::storage_path(&self.id))
    }
}
impl DbAudioFormat {
    pub fn new(
        track_id: &str,
        content_type: ContentType,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        sample_rate: i64,
        bits_per_sample: i64,
        seektable_json: String,
        audio_data_start: i64,
    ) -> Self {
        Self::new_full(
            track_id,
            content_type,
            flac_headers,
            needs_headers,
            None,
            None,
            None,
            None,
            None,
            sample_rate,
            bits_per_sample,
            seektable_json,
            audio_data_start,
            None,
        )
    }

    pub fn new_with_byte_offsets(
        track_id: &str,
        content_type: ContentType,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        start_byte_offset: i64,
        end_byte_offset: i64,
        pregap_ms: Option<i64>,
        frame_offset_samples: Option<i64>,
        exact_sample_count: Option<i64>,
        sample_rate: i64,
        bits_per_sample: i64,
        seektable_json: String,
        audio_data_start: i64,
    ) -> Self {
        Self::new_full(
            track_id,
            content_type,
            flac_headers,
            needs_headers,
            Some(start_byte_offset),
            Some(end_byte_offset),
            pregap_ms,
            frame_offset_samples,
            exact_sample_count,
            sample_rate,
            bits_per_sample,
            seektable_json,
            audio_data_start,
            None,
        )
    }

    /// Set the file_id linking to DbFile
    pub fn with_file_id(mut self, file_id: &str) -> Self {
        self.file_id = Some(file_id.to_string());
        self
    }

    fn new_full(
        track_id: &str,
        content_type: ContentType,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        start_byte_offset: Option<i64>,
        end_byte_offset: Option<i64>,
        pregap_ms: Option<i64>,
        frame_offset_samples: Option<i64>,
        exact_sample_count: Option<i64>,
        sample_rate: i64,
        bits_per_sample: i64,
        seektable_json: String,
        audio_data_start: i64,
        file_id: Option<String>,
    ) -> Self {
        let now = Utc::now();
        DbAudioFormat {
            id: Uuid::new_v4().to_string(),
            track_id: track_id.to_string(),
            content_type,
            flac_headers,
            needs_headers,
            start_byte_offset,
            end_byte_offset,
            pregap_ms,
            frame_offset_samples,
            exact_sample_count,
            sample_rate,
            bits_per_sample,
            seektable_json,
            audio_data_start,
            file_id,
            updated_at: now,
            created_at: now,
        }
    }
}
/// Torrent import metadata for a release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTorrent {
    pub id: String,
    pub release_id: String,
    pub info_hash: String,
    pub magnet_link: Option<String>,
    pub torrent_name: String,
    pub total_size_bytes: i64,
    pub piece_length: i32,
    pub num_pieces: i32,
    pub is_seeding: bool,
    pub created_at: DateTime<Utc>,
}
/// Maps torrent pieces to bae chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTorrentPieceMapping {
    pub id: String,
    pub torrent_id: String,
    pub piece_index: i32,
    pub chunk_ids: String,
    pub start_byte_in_first_chunk: i64,
    pub end_byte_in_last_chunk: i64,
}
impl DbTorrent {
    pub fn new(
        release_id: &str,
        info_hash: &str,
        magnet_link: Option<String>,
        torrent_name: &str,
        total_size_bytes: i64,
        piece_length: i32,
        num_pieces: i32,
    ) -> Self {
        DbTorrent {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            info_hash: info_hash.to_string(),
            magnet_link,
            torrent_name: torrent_name.to_string(),
            total_size_bytes,
            piece_length,
            num_pieces,
            is_seeding: false,
            created_at: Utc::now(),
        }
    }
}
impl DbTorrentPieceMapping {
    pub fn new(
        torrent_id: &str,
        piece_index: i32,
        chunk_ids: Vec<String>,
        start_byte_in_first_chunk: i64,
        end_byte_in_last_chunk: i64,
    ) -> Result<Self, serde_json::Error> {
        Ok(DbTorrentPieceMapping {
            id: Uuid::new_v4().to_string(),
            torrent_id: torrent_id.to_string(),
            piece_index,
            chunk_ids: serde_json::to_string(&chunk_ids)?,
            start_byte_in_first_chunk,
            end_byte_in_last_chunk,
        })
    }
    pub fn chunk_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.chunk_ids)
    }
}
const IMPORT_OP_STATUS_PREPARING: &str = "preparing";
const IMPORT_OP_STATUS_IMPORTING: &str = "importing";
const IMPORT_OP_STATUS_COMPLETE: &str = "complete";
const IMPORT_OP_STATUS_FAILED: &str = "failed";
/// Status of an import operation (distinct from release/track ImportStatus)
///
/// Tracks the lifecycle of an import from button click through completion:
/// - Preparing: Phase 0 work in ImportHandle (parsing, validation, DB setup)
/// - Importing: Phase 1 work in ImportService (file processing, encryption, upload)
/// - Complete: Successfully finished
/// - Failed: Error occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ImportOperationStatus {
    Preparing,
    Importing,
    Complete,
    Failed,
}
impl ImportOperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportOperationStatus::Preparing => IMPORT_OP_STATUS_PREPARING,
            ImportOperationStatus::Importing => IMPORT_OP_STATUS_IMPORTING,
            ImportOperationStatus::Complete => IMPORT_OP_STATUS_COMPLETE,
            ImportOperationStatus::Failed => IMPORT_OP_STATUS_FAILED,
        }
    }
}
/// Tracks an import operation from button click through completion
///
/// Created when user clicks Import, before any database records exist.
/// Provides a stable ID for progress subscriptions during phase 0.
/// Linked to release_id after phase 0 completes and release is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbImport {
    pub id: String,
    pub status: ImportOperationStatus,
    /// Linked after phase 0 when release is created
    pub release_id: Option<String>,
    /// Album title for display before release exists
    pub album_title: String,
    /// Artist name for display
    pub artist_name: String,
    /// Source folder path
    pub folder_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Error message if status is Failed
    pub error_message: Option<String>,
}
impl DbImport {
    pub fn new(id: &str, album_title: &str, artist_name: &str, folder_path: &str) -> Self {
        let now = Utc::now().timestamp();
        DbImport {
            id: id.to_string(),
            status: ImportOperationStatus::Preparing,
            release_id: None,
            album_title: album_title.to_string(),
            artist_name: artist_name.to_string(),
            folder_path: folder_path.to_string(),
            created_at: now,
            updated_at: now,
            error_message: None,
        }
    }
}
/// Type discriminator for library images
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryImageType {
    Cover,
    Artist,
}

impl LibraryImageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LibraryImageType::Cover => "cover",
            LibraryImageType::Artist => "artist",
        }
    }
}

impl std::str::FromStr for LibraryImageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cover" => Ok(LibraryImageType::Cover),
            "artist" => Ok(LibraryImageType::Artist),
            other => Err(format!("Unknown library image type: {}", other)),
        }
    }
}

/// bae-managed metadata image (cover art, artist photo).
/// File lives at a deterministic path derived from type + id:
/// - Cover: covers/{id}
/// - Artist: artists/{id}
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
    pub image_type: LibraryImageType,
    pub content_type: ContentType,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// "local", "musicbrainz", "discogs"
    pub source: String,
    /// MB: CAA image ID, Discogs: URL, local: "release://{path}"
    pub source_url: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Share Grants
// ============================================================================

/// An accepted share grant stored in the local DB.
///
/// This represents a grant the user received from someone else, giving access
/// to one release in a remote library.
#[derive(Debug, Clone)]
pub struct DbShareGrant {
    pub id: String,
    pub from_library_id: String,
    pub from_user_pubkey: String,
    pub release_id: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub wrapped_payload: Vec<u8>,
    pub expires: Option<String>,
    pub signature: String,
    pub accepted_at: Option<String>,
    pub created_at: String,
    /// Hex-encoded 32-byte release decryption key, populated on accept.
    pub release_key_hex: Option<String>,
    /// S3 access key from the unwrapped payload, populated on accept.
    pub s3_access_key: Option<String>,
    /// S3 secret key from the unwrapped payload, populated on accept.
    pub s3_secret_key: Option<String>,
}

// ============================================================================
// Attestations
// ============================================================================

/// A stored attestation linking a MusicBrainz release ID to a BitTorrent infohash.
#[derive(Debug, Clone)]
pub struct DbAttestation {
    pub id: String,
    pub mbid: String,
    pub infohash: String,
    pub content_hash: String,
    pub format: String,
    pub author_pubkey: String,
    pub timestamp: String,
    pub signature: String,
    pub created_at: String,
}

// ============================================================================
// Library Search Result Types
// ============================================================================

/// Combined search results across artists, albums, and tracks
#[derive(Debug, Clone)]
pub struct LibrarySearchResults {
    pub artists: Vec<ArtistSearchResult>,
    pub albums: Vec<AlbumSearchResult>,
    pub tracks: Vec<TrackSearchResult>,
}

/// Artist search result with album count
#[derive(Debug, Clone)]
pub struct ArtistSearchResult {
    pub id: String,
    pub name: String,
    pub album_count: i64,
}

/// Album search result with primary artist name
#[derive(Debug, Clone)]
pub struct AlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub cover_release_id: Option<String>,
    pub artist_name: String,
}

/// Track search result with album and artist info
#[derive(Debug, Clone)]
pub struct TrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}
