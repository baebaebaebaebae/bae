//! Type conversion helpers between bae-core and bae-ui display types.

use bae_core::discogs::DiscogsRelease;
use bae_core::import::{MatchCandidate, MatchSource};
use bae_ui::display_types::{
    AudioContentInfo, CandidateTrack, CategorizedFileInfo, FolderMetadata as DisplayFolderMetadata,
    MatchCandidate as DisplayMatchCandidate, MatchSourceType,
};

/// Convert bae-core FolderMetadata to display type
pub fn to_display_metadata(m: &bae_core::import::FolderMetadata) -> DisplayFolderMetadata {
    DisplayFolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: bae_core::musicbrainz::extract_search_tokens(m),
    }
}

/// Convert display FolderMetadata back to core type (for ranking functions)
pub fn from_display_metadata(m: &DisplayFolderMetadata) -> bae_core::import::FolderMetadata {
    bae_core::import::FolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: m.folder_tokens.clone(),
    }
}

/// Convert bae-core MatchCandidate to display type
pub fn to_display_candidate(candidate: &MatchCandidate) -> DisplayMatchCandidate {
    let (
        source_type,
        format,
        country,
        label,
        catalog_number,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
    ) = match &candidate.source {
        MatchSource::MusicBrainz(release) => (
            MatchSourceType::MusicBrainz,
            release.format.clone(),
            release.country.clone(),
            release.label.clone(),
            release.catalog_number.clone(),
            release.first_release_date.clone(),
            Some(release.release_id.clone()),
            Some(release.release_group_id.clone()),
            None,
            None,
        ),
        MatchSource::Discogs(result) => (
            MatchSourceType::Discogs,
            result.format.as_ref().map(|v| v.join(", ")),
            result.country.clone(),
            result.label.as_ref().and_then(|v| v.first().cloned()),
            result.catno.clone(),
            None,
            None,
            None,
            Some(result.id.to_string()),
            result.master_id.map(|id| id.to_string()),
        ),
    };

    DisplayMatchCandidate {
        title: candidate.title(),
        artist: match &candidate.source {
            MatchSource::MusicBrainz(r) => r.artist.clone(),
            MatchSource::Discogs(r) => r.title.split(" - ").next().unwrap_or("").to_string(),
        },
        year: candidate.year(),
        cover_url: candidate.cover_art_url(),
        cover_fetch_failed: false,
        format,
        country,
        label,
        catalog_number,
        source_type,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
        existing_album_id: None,
        tracks: vec![],
    }
}

/// Extract tracks from a Discogs release for UI display.
///
/// Filters out heading entries (those with empty position).
pub fn extract_tracks_from_discogs(release: &DiscogsRelease) -> Vec<CandidateTrack> {
    release
        .tracklist
        .iter()
        .filter(|t| !t.position.is_empty())
        .map(|t| CandidateTrack {
            position: t.position.clone(),
            title: t.title.clone(),
            duration: t.duration.clone(),
        })
        .collect()
}

/// Extract tracks from a typed MusicBrainz release response for UI display.
///
/// Iterates media[].tracks[], extracting position, title, and length (ms -> mm:ss).
pub fn extract_tracks_from_mb_response(
    response: &bae_core::musicbrainz::MbReleaseResponse,
) -> Vec<CandidateTrack> {
    response
        .media
        .iter()
        .flat_map(|medium| &medium.tracks)
        .map(|track| {
            let position = track
                .position
                .map(|p| p.to_string())
                .or_else(|| track.number.clone())
                .unwrap_or_default();

            let title = track.title.clone().unwrap_or_default();

            let duration = track.length.map(|ms| {
                let total_secs = ms / 1000;
                let mins = total_secs / 60;
                let secs = total_secs % 60;
                format!("{}:{:02}", mins, secs)
            });

            CandidateTrack {
                position,
                title,
                duration,
            }
        })
        .collect()
}

/// Count the number of local audio files from categorized file info.
///
/// For CUE/FLAC pairs, sums the track_count of each pair.
/// For individual track files, returns the number of files.
pub fn count_local_audio_files(files: &CategorizedFileInfo) -> usize {
    match &files.audio {
        AudioContentInfo::CueFlacPairs(pairs) => pairs.iter().map(|p| p.track_count).sum(),
        AudioContentInfo::TrackFiles(tracks) => tracks.len(),
    }
}

/// Count the number of tracks in a Discogs release.
///
/// Filters out non-track entries (headings/index tracks have empty position).
pub fn count_discogs_release_tracks(release: &bae_core::discogs::DiscogsRelease) -> usize {
    release
        .tracklist
        .iter()
        .filter(|t| !t.position.is_empty())
        .count()
}

/// Convert scanned file to display FileInfo
fn scanned_to_file_info(
    f: &bae_core::import::folder_scanner::ScannedFile,
    imgs: &bae_core::image_server::ImageServerHandle,
) -> bae_ui::display_types::FileInfo {
    let ext_lower = f
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = ext_lower.to_uppercase();
    let name = f
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let path = f.path.to_string_lossy().to_string();
    let display_url = imgs.local_file_url(&f.path);

    bae_ui::display_types::FileInfo {
        name,
        path,
        size: f.size,
        format,
        display_url,
    }
}

/// Convert CategorizedFiles from core to display type
pub fn categorized_files_from_scanned(
    files: &bae_core::import::CategorizedFiles,
    imgs: &bae_core::image_server::ImageServerHandle,
) -> CategorizedFileInfo {
    use bae_core::import::folder_scanner::AudioContent;
    use bae_ui::display_types::{CueFlacPairInfo, FileInfo};

    let audio = match &files.audio {
        AudioContent::CueFlacPairs(pairs) => {
            let display_pairs: Vec<CueFlacPairInfo> = pairs
                .iter()
                .map(|p| CueFlacPairInfo {
                    cue_name: p
                        .cue_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    cue_path: p.cue_file.path.to_string_lossy().to_string(),
                    flac_name: p
                        .audio_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    total_size: p.cue_file.size + p.audio_file.size,
                    track_count: p.track_count,
                })
                .collect();
            AudioContentInfo::CueFlacPairs(display_pairs)
        }
        AudioContent::TrackFiles(tracks) => {
            let mut display_tracks: Vec<FileInfo> = tracks
                .iter()
                .map(|t| scanned_to_file_info(t, imgs))
                .collect();
            display_tracks.sort_by(|a, b| a.name.cmp(&b.name));
            AudioContentInfo::TrackFiles(display_tracks)
        }
    };

    let mut artwork: Vec<FileInfo> = files
        .artwork
        .iter()
        .map(|f| scanned_to_file_info(f, imgs))
        .collect();
    artwork.sort_by(|a, b| a.name.cmp(&b.name));

    let mut documents: Vec<FileInfo> = files
        .documents
        .iter()
        .map(|f| scanned_to_file_info(f, imgs))
        .collect();
    documents.sort_by(|a, b| a.name.cmp(&b.name));

    CategorizedFileInfo {
        audio,
        artwork,
        documents,
        bad_audio_count: files.bad_audio_count,
        bad_image_count: files.bad_image_count,
    }
}
