use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorage;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::{PcmSource, PlaybackError};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Read a track's audio file and decode to PCM for playback.
///
/// For tracks with source_path set, reads from local file.
/// For cloud storage, downloads and decrypts the file.
/// Returns decoded PCM audio ready for cpal output.
pub async fn load_track_audio(
    track_id: &str,
    library_manager: &LibraryManager,
    storage: Option<Arc<dyn CloudStorage>>,
    cache: &CacheManager,
    encryption_service: Option<&EncryptionService>,
) -> Result<Arc<PcmSource>, PlaybackError> {
    info!("Loading audio for track: {}", track_id);

    let audio_format = library_manager
        .get_audio_format_by_track_id(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Audio format", track_id))?;

    let track = library_manager
        .get_track(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Track", track_id))?;

    // Find the file for this track
    let files = library_manager
        .get_files_for_release(&track.release_id)
        .await
        .map_err(PlaybackError::database)?;

    // For CUE/FLAC: find the FLAC file (there should be one large file)
    // For one-file-per-track: find the file that matches this track
    let audio_file = files
        .iter()
        .find(|f| f.content_type == crate::content_type::ContentType::Flac)
        .ok_or_else(|| PlaybackError::not_found("Audio file", track_id))?;

    // Read the file data from source_path
    let source_path = audio_file
        .source_path
        .as_ref()
        .ok_or_else(|| PlaybackError::not_found("source_path", track_id))?;

    let file_data = if let Some(storage) = storage {
        // Remote storage - download (and decrypt if needed)
        let storage_profile = library_manager
            .get_storage_profile_for_release(&track.release_id)
            .await
            .map_err(PlaybackError::database)?;

        debug!("Downloading from cloud: {}", source_path);

        // Check cache first
        let cache_key = format!("file:{}", audio_file.id);
        let encrypted_data = match cache.get(&cache_key).await {
            Ok(Some(cached_data)) => {
                debug!("Cache hit for file: {}", audio_file.id);
                cached_data
            }
            Ok(None) | Err(_) => {
                debug!("Cache miss - downloading file: {}", audio_file.id);
                let data = storage
                    .download(source_path)
                    .await
                    .map_err(PlaybackError::cloud)?;

                if let Err(e) = cache.put(&cache_key, &data).await {
                    warn!("Failed to cache file (non-fatal): {}", e);
                }
                data
            }
        };

        // Decrypt if profile has encryption enabled
        if storage_profile.map(|p| p.encrypted).unwrap_or(false) {
            let encryption_service = encryption_service
                .ok_or_else(|| {
                    PlaybackError::decrypt(crate::encryption::EncryptionError::KeyManagement(
                        "Cannot play encrypted files: encryption not configured".into(),
                    ))
                })?
                .clone();
            tokio::task::spawn_blocking(move || {
                encryption_service
                    .decrypt(&encrypted_data)
                    .map_err(PlaybackError::decrypt)
            })
            .await
            .map_err(PlaybackError::task)??
        } else {
            encrypted_data
        }
    } else {
        // Local file - read directly from source_path
        debug!("Reading from local file: {}", source_path);
        tokio::fs::read(source_path)
            .await
            .map_err(|e| PlaybackError::io(format!("Failed to read file: {}", e)))?
    };

    debug!("Read {} bytes of audio data", file_data.len());

    // For CUE/FLAC tracks, we need to extract just this track's portion
    // and prepend headers if needed
    let audio_data = if audio_format.needs_headers {
        if let Some(ref headers) = audio_format.flac_headers {
            debug!("CUE/FLAC track: prepending headers for decode");
            // For CUE/FLAC, we'd need to extract the track's byte range
            // This requires the seektable and start/end times
            // For now, use the whole file (will be enhanced later)
            let mut temp_flac = headers.clone();
            temp_flac.extend_from_slice(&file_data);
            temp_flac
        } else {
            warn!("Audio format needs headers but none provided");
            file_data
        }
    } else {
        file_data
    };

    debug!("Decoding {} bytes of audio data to PCM", audio_data.len());
    let decoded = decode_audio_to_pcm(&audio_data).await?;

    info!(
        "Successfully decoded track {}: {} samples, {}Hz, {} channels",
        track_id,
        decoded.samples.len(),
        decoded.sample_rate,
        decoded.channels
    );

    Ok(Arc::new(PcmSource::new(
        decoded.samples,
        decoded.sample_rate,
        decoded.channels,
        decoded.bits_per_sample,
    )))
}

/// Decode audio data to PCM using FFmpeg
pub(crate) async fn decode_audio_to_pcm(
    audio_data: &[u8],
) -> Result<crate::audio_codec::DecodedAudio, PlaybackError> {
    let audio_data = audio_data.to_vec();
    tokio::task::spawn_blocking(move || crate::audio_codec::decode_audio(&audio_data, None, None))
        .await
        .map_err(PlaybackError::task)?
        .map_err(PlaybackError::flac)
}
