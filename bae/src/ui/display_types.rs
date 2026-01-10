//! Conversions from DB types to bae-ui display types

use crate::db::{
    DbAlbum, DbArtist, DbFile, DbImage, DbRelease, DbTrack, ImageSource, ImportStatus,
};
use crate::playback::PlaybackState;
use crate::ui::image_url;

// Re-export bae-ui types so existing code continues to work
pub use bae_ui::{
    Album, Artist, File, Image, PlaybackDisplay, QueueItem, Release, Track, TrackImportState,
};

impl From<DbAlbum> for Album {
    fn from(db: DbAlbum) -> Self {
        let cover_url = db
            .cover_image_id
            .as_ref()
            .map(|id| image_url(id))
            .or(db.cover_art_url);

        Album {
            id: db.id,
            title: db.title,
            year: db.year,
            cover_url,
            is_compilation: db.is_compilation,
        }
    }
}

impl From<&DbAlbum> for Album {
    fn from(db: &DbAlbum) -> Self {
        let cover_url = db
            .cover_image_id
            .as_ref()
            .map(|id| image_url(id))
            .or_else(|| db.cover_art_url.clone());

        Album {
            id: db.id.clone(),
            title: db.title.clone(),
            year: db.year,
            cover_url,
            is_compilation: db.is_compilation,
        }
    }
}

impl From<DbArtist> for Artist {
    fn from(db: DbArtist) -> Self {
        Artist {
            id: db.id,
            name: db.name,
        }
    }
}

impl From<&DbArtist> for Artist {
    fn from(db: &DbArtist) -> Self {
        Artist {
            id: db.id.clone(),
            name: db.name.clone(),
        }
    }
}

impl From<DbTrack> for Track {
    fn from(db: DbTrack) -> Self {
        let is_available = db.import_status == ImportStatus::Complete;
        Track {
            id: db.id,
            title: db.title,
            track_number: db.track_number,
            disc_number: db.disc_number,
            duration_ms: db.duration_ms,
            is_available,
            import_state: if is_available {
                TrackImportState::Complete
            } else {
                TrackImportState::None
            },
        }
    }
}

impl From<&DbTrack> for Track {
    fn from(db: &DbTrack) -> Self {
        let is_available = db.import_status == ImportStatus::Complete;
        Track {
            id: db.id.clone(),
            title: db.title.clone(),
            track_number: db.track_number,
            disc_number: db.disc_number,
            duration_ms: db.duration_ms,
            is_available,
            import_state: if is_available {
                TrackImportState::Complete
            } else {
                TrackImportState::None
            },
        }
    }
}

impl From<&PlaybackState> for PlaybackDisplay {
    fn from(state: &PlaybackState) -> Self {
        match state {
            PlaybackState::Stopped => PlaybackDisplay::Stopped,
            PlaybackState::Loading { track_id } => PlaybackDisplay::Loading {
                track_id: track_id.clone(),
            },
            PlaybackState::Playing {
                track,
                position,
                duration,
                ..
            } => PlaybackDisplay::Playing {
                track_id: track.id.clone(),
                position_ms: position.as_millis() as u64,
                duration_ms: duration.map(|d| d.as_millis() as u64).unwrap_or(0),
            },
            PlaybackState::Paused {
                track,
                position,
                duration,
                ..
            } => PlaybackDisplay::Paused {
                track_id: track.id.clone(),
                position_ms: position.as_millis() as u64,
                duration_ms: duration.map(|d| d.as_millis() as u64).unwrap_or(0),
            },
        }
    }
}

impl From<&DbRelease> for Release {
    fn from(db: &DbRelease) -> Self {
        Release {
            id: db.id.clone(),
            album_id: db.album_id.clone(),
            release_name: db.release_name.clone(),
            year: db.year,
            format: db.format.clone(),
            label: db.label.clone(),
            catalog_number: db.catalog_number.clone(),
            country: db.country.clone(),
            barcode: db.barcode.clone(),
            discogs_release_id: db.discogs_release_id.clone(),
            musicbrainz_release_id: None,
        }
    }
}

impl From<&DbFile> for File {
    fn from(db: &DbFile) -> Self {
        File {
            id: db.id.clone(),
            filename: db.original_filename.clone(),
            file_size: db.file_size,
            format: db.format.clone(),
        }
    }
}

impl From<&DbImage> for Image {
    fn from(db: &DbImage) -> Self {
        Image {
            id: db.id.clone(),
            filename: db.filename.clone(),
            is_cover: db.is_cover,
            source: match db.source {
                ImageSource::Local => "Local".to_string(),
                ImageSource::MusicBrainz => "MusicBrainz".to_string(),
                ImageSource::Discogs => "Discogs".to_string(),
            },
        }
    }
}
