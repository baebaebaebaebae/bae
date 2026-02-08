//! Conversions from DB types to bae-ui display types

use crate::ui::cover_url;
use bae_core::db::{DbAlbum, DbArtist, DbFile, DbImage, DbRelease, DbTrack, ImportStatus};

// Re-export bae-ui types so existing code continues to work
pub use bae_ui::{Album, Artist, File, Image, Release, Track, TrackImportState};

pub fn album_from_db_ref(db: &DbAlbum) -> Album {
    let cover_url = db
        .cover_release_id
        .as_ref()
        .map(|release_id| cover_url(release_id))
        .or_else(|| db.cover_art_url.clone());

    Album {
        id: db.id.clone(),
        title: db.title.clone(),
        year: db.year,
        cover_url,
        is_compilation: db.is_compilation,
        date_added: db.created_at,
    }
}

pub fn artist_from_db_ref(db: &DbArtist) -> Artist {
    let image_url = db
        .image_path
        .as_ref()
        .map(|_| format!("bae://artist-image/{}", db.id));
    Artist {
        id: db.id.clone(),
        name: db.name.clone(),
        image_url,
    }
}

pub fn track_from_db_ref(db: &DbTrack) -> Track {
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

pub fn file_from_db_ref(db: &DbFile) -> File {
    File {
        id: db.id.clone(),
        filename: db.original_filename.clone(),
        file_size: db.file_size,
        format: db.format.clone(),
    }
}

pub fn image_from_db_ref(db: &DbImage) -> Image {
    Image {
        id: db.id.clone(),
        filename: db.filename.clone(),
        is_cover: db.is_cover,
        source: db.source.as_str().to_string(),
        url: format!("bae://image/{}", db.id),
    }
}

pub fn release_from_db_ref(db: &DbRelease) -> Release {
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
