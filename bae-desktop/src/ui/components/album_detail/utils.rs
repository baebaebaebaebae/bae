use bae_core::library::{LibraryError, SharedLibraryManager};

/// Converts an empty string to None, otherwise wraps the string in Some
pub fn maybe_not_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Get track IDs for an album's first release, sorted by track number.
/// Returns track IDs ready to be passed to playback.play_album().
pub async fn get_album_track_ids(
    library_manager: &SharedLibraryManager,
    album_id: &str,
) -> Result<Vec<String>, LibraryError> {
    let releases = library_manager
        .get()
        .get_releases_for_album(album_id)
        .await?;
    if releases.is_empty() {
        return Ok(Vec::new());
    }
    let first_release = &releases[0];
    let mut tracks = library_manager.get().get_tracks(&first_release.id).await?;
    tracks.sort_by(|a, b| match (a.track_number, b.track_number) {
        (Some(a_num), Some(b_num)) => a_num.cmp(&b_num),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Ok(tracks.iter().map(|t| t.id.clone()).collect())
}
