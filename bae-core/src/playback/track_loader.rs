use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorage;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::{PcmSource, PlaybackError};
use crate::sync::shared_release::SharedRelease;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Resolved shared release storage: S3 client, encryption service, and storage key.
pub struct SharedReleaseStorage {
    pub storage: Arc<dyn CloudStorage>,
    pub encryption: Arc<EncryptionService>,
    pub storage_key: String,
}

/// Build an S3 client and encryption service from a resolved shared release.
pub async fn create_shared_release_storage(
    shared: SharedRelease,
    file_id: &str,
) -> Result<SharedReleaseStorage, PlaybackError> {
    let access_key = shared.s3_access_key.ok_or_else(|| {
        PlaybackError::cloud(crate::cloud_storage::CloudStorageError::Config(
            "Shared release missing S3 access key".into(),
        ))
    })?;
    let secret_key = shared.s3_secret_key.ok_or_else(|| {
        PlaybackError::cloud(crate::cloud_storage::CloudStorageError::Config(
            "Shared release missing S3 secret key".into(),
        ))
    })?;

    let s3_config = crate::cloud_storage::S3Config {
        bucket_name: shared.bucket,
        region: shared.region,
        access_key_id: access_key,
        secret_access_key: secret_key,
        endpoint_url: shared.endpoint,
    };
    let storage = crate::cloud_storage::S3CloudStorage::new_with_bucket_creation(s3_config, false)
        .await
        .map_err(PlaybackError::cloud)?;
    let storage: Arc<dyn CloudStorage> = Arc::new(storage);
    let encryption = Arc::new(EncryptionService::from_key(shared.release_key));
    let storage_key = crate::storage::storage_path(file_id);

    Ok(SharedReleaseStorage {
        storage,
        encryption,
        storage_key,
    })
}

/// Read a track's audio file and decode to PCM for playback.
///
/// For managed-locally tracks, reads from the library directory.
/// For unmanaged tracks, reads from the unmanaged_path.
/// For cloud storage, downloads and decrypts the file.
/// Returns decoded PCM audio ready for cpal output.
pub async fn load_track_audio(
    track_id: &str,
    library_manager: &LibraryManager,
    library_dir: &crate::library_dir::LibraryDir,
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

    // Look up release to determine storage mode
    let release = library_manager
        .database()
        .get_release_by_id(&track.release_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Release", &track.release_id))?;

    let file_data = if let Some(storage) = storage {
        // Remote storage passed in - download (and decrypt if needed)
        debug!("Downloading from cloud storage");

        // Check cache first
        let cache_key = format!("file:{}", audio_file.id);
        let encrypted_data = match cache.get(&cache_key).await {
            Ok(Some(cached_data)) => {
                debug!("Cache hit for file: {}", audio_file.id);
                cached_data
            }
            Ok(None) | Err(_) => {
                debug!("Cache miss - downloading file: {}", audio_file.id);
                let storage_key = crate::storage::storage_path(&audio_file.id);
                let data = storage
                    .download(&storage_key)
                    .await
                    .map_err(PlaybackError::cloud)?;

                if let Err(e) = cache.put(&cache_key, &data).await {
                    warn!("Failed to cache file (non-fatal): {}", e);
                }
                data
            }
        };

        // Decrypt if encryption service is available (managed files are encrypted)
        if let Some(enc) = encryption_service {
            let enc = enc.clone();
            tokio::task::spawn_blocking(move || {
                enc.decrypt(&encrypted_data).map_err(PlaybackError::decrypt)
            })
            .await
            .map_err(PlaybackError::task)??
        } else {
            encrypted_data
        }
    } else {
        // No cloud storage passed in — check if this is a shared release
        let shared = match crate::sync::shared_release::resolve_release(
            library_manager.database(),
            &track.release_id,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "Failed to resolve shared release for {}: {e}",
                    track.release_id
                );
                None
            }
        };

        if let Some(shared) = shared {
            let srs = create_shared_release_storage(shared, &audio_file.id).await?;

            debug!("Downloading shared release file: {}", srs.storage_key);

            // Check cache first
            let cache_key = format!("file:{}", audio_file.id);
            let encrypted_data = match cache.get(&cache_key).await {
                Ok(Some(cached_data)) => {
                    debug!("Cache hit for shared file: {}", audio_file.id);
                    cached_data
                }
                Ok(None) | Err(_) => {
                    debug!("Cache miss - downloading shared file: {}", audio_file.id);
                    let data = srs
                        .storage
                        .download(&srs.storage_key)
                        .await
                        .map_err(PlaybackError::cloud)?;

                    if let Err(e) = cache.put(&cache_key, &data).await {
                        warn!("Failed to cache file (non-fatal): {}", e);
                    }
                    data
                }
            };

            // Shared releases are always encrypted with the per-release key
            let enc = srs.encryption;
            tokio::task::spawn_blocking(move || {
                enc.decrypt(&encrypted_data).map_err(PlaybackError::decrypt)
            })
            .await
            .map_err(PlaybackError::task)??
        } else {
            // Local file - derive path from release storage flags
            let source_path = if release.managed_locally {
                audio_file.local_storage_path(library_dir)
            } else if let Some(ref unmanaged_path) = release.unmanaged_path {
                std::path::Path::new(unmanaged_path).join(&audio_file.original_filename)
            } else {
                return Err(PlaybackError::not_found("file location", track_id));
            };

            debug!("Reading from local file: {}", source_path.display());
            tokio::fs::read(&source_path)
                .await
                .map_err(|e| PlaybackError::io(format!("Failed to read file: {}", e)))?
        }
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
