//! Storage reader utilities for creating storage clients from profiles
use crate::cloud_storage::{CloudStorage, CloudStorageError, S3CloudStorage};
use crate::db::{DbStorageProfile, StorageLocation};
use crate::encryption::{encrypted_range_for_plaintext, EncryptionService, CHUNK_SIZE};
use crate::playback::SharedStreamingBuffer;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{debug, info};

/// Create a storage reader from a profile.
///
/// For cloud profiles: creates S3CloudStorage from profile credentials
/// For local profiles: returns LocalFileStorage that reads from disk
pub async fn create_storage_reader(
    profile: &DbStorageProfile,
) -> Result<Arc<dyn CloudStorage>, CloudStorageError> {
    debug!(
        "Creating storage reader for profile '{}' (id={}, location={:?})",
        profile.name, profile.id, profile.location
    );

    match profile.location {
        StorageLocation::Cloud => {
            let s3_config = profile.to_s3_config().ok_or_else(|| {
                CloudStorageError::Config("Missing S3 credentials in profile".into())
            })?;
            let client = S3CloudStorage::new(s3_config).await?;
            Ok(Arc::new(client))
        }
        StorageLocation::Local => Ok(Arc::new(LocalFileStorage)),
    }
}

/// Local file storage that reads files from disk paths.
pub struct LocalFileStorage;

#[async_trait::async_trait]
impl CloudStorage for LocalFileStorage {
    async fn upload(&self, path: &str, data: &[u8]) -> Result<String, CloudStorageError> {
        tokio::fs::write(path, data).await?;
        Ok(path.to_string())
    }

    async fn download(&self, path: &str) -> Result<Vec<u8>, CloudStorageError> {
        tokio::fs::read(path).await.map_err(CloudStorageError::Io)
    }

    async fn download_range(
        &self,
        path: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, CloudStorageError> {
        if start >= end {
            return Err(CloudStorageError::Download(format!(
                "Invalid range: start ({}) >= end ({})",
                start, end
            )));
        }

        let mut file = tokio::fs::File::open(path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;

        let len = (end - start) as usize;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer).await?;

        Ok(buffer)
    }

    async fn delete(&self, path: &str) -> Result<(), CloudStorageError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(CloudStorageError::Io)
    }
}

/// Default chunk size for streaming downloads (256KB)
pub const STREAMING_CHUNK_SIZE: usize = 256 * 1024;

/// Download a file in chunks, feeding data to a streaming buffer.
///
/// This enables streaming playback by:
/// 1. Fetching data incrementally (not waiting for full download)
/// 2. Feeding each chunk to the buffer as it arrives
/// 3. Marking EOF when download completes
///
/// For local files, reads in chunks. For cloud, uses HTTP range requests.
pub async fn download_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
    file_size: Option<u64>,
) -> Result<(), CloudStorageError> {
    download_to_streaming_buffer_with_range(storage, path, buffer, 0, file_size).await
}

/// Download a byte range in chunks, feeding data to a streaming buffer.
///
/// Useful for CUE/FLAC tracks where we only need a portion of the file.
pub async fn download_to_streaming_buffer_with_range(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
    start_offset: u64,
    end_offset: Option<u64>,
) -> Result<(), CloudStorageError> {
    let chunk_size = STREAMING_CHUNK_SIZE as u64;
    let mut current_pos = start_offset;

    info!(
        "Starting streaming download: {} from offset {} (end: {:?})",
        path, start_offset, end_offset
    );

    loop {
        // Calculate chunk boundaries
        let chunk_start = current_pos;
        let chunk_end = if let Some(end) = end_offset {
            (current_pos + chunk_size).min(end)
        } else {
            current_pos + chunk_size
        };

        // Check if we've reached the end
        if let Some(end) = end_offset {
            if chunk_start >= end {
                break;
            }
        }

        // Download this chunk
        let chunk_data = match storage.download_range(path, chunk_start, chunk_end).await {
            Ok(data) => data,
            Err(CloudStorageError::Download(msg)) if msg.contains("range") => {
                // Range request failed, might be at EOF for unknown-size files
                debug!("Range request failed (likely EOF): {}", msg);
                break;
            }
            Err(e) => {
                buffer.cancel();
                return Err(e);
            }
        };

        if chunk_data.is_empty() {
            // No more data
            break;
        }

        debug!(
            "Downloaded chunk {}-{}: {} bytes",
            chunk_start,
            chunk_end,
            chunk_data.len()
        );

        // Feed to buffer
        buffer.append(&chunk_data);

        current_pos += chunk_data.len() as u64;

        // Check if we got less than requested (EOF)
        if chunk_data.len() < chunk_size as usize {
            break;
        }
    }

    buffer.mark_eof();

    info!(
        "Streaming download complete: {} ({} bytes total)",
        path,
        current_pos - start_offset
    );

    Ok(())
}

/// Download an encrypted file in chunks, decrypting and feeding to streaming buffer.
///
/// Handles encryption chunk alignment automatically:
/// - Downloads in multiples of encrypted chunk size
/// - Decrypts each chunk as it arrives
/// - Feeds plaintext to the buffer
pub async fn download_encrypted_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
    encryption: &EncryptionService,
    plaintext_start: u64,
    plaintext_end: Option<u64>,
) -> Result<(), CloudStorageError> {
    // First, we need the file header (nonce) which is always at the start
    // The nonce is 24 bytes
    const NONCE_SIZE: u64 = 24;

    // For streaming encrypted files, we download the nonce first,
    // then download encrypted chunks, decrypt, and feed to buffer.

    // Calculate how many plaintext bytes we want per iteration
    // Use a multiple of CHUNK_SIZE for efficiency
    let plaintext_chunk_size = CHUNK_SIZE as u64 * 4; // 256KB plaintext per iteration

    let mut plaintext_pos = plaintext_start;
    let final_end = plaintext_end.unwrap_or(u64::MAX);

    info!(
        "Starting encrypted streaming download: {} from {} to {:?}",
        path, plaintext_start, plaintext_end
    );

    // Download the file header (nonce) first - we need this for all decryption
    let header = storage.download_range(path, 0, NONCE_SIZE).await?;
    if header.len() < NONCE_SIZE as usize {
        buffer.cancel();
        return Err(CloudStorageError::Download(
            "File too short for encryption header".into(),
        ));
    }

    loop {
        if plaintext_pos >= final_end {
            break;
        }

        // Calculate the plaintext range for this iteration
        let chunk_plaintext_end = (plaintext_pos + plaintext_chunk_size).min(final_end);

        // Calculate the encrypted range needed for this plaintext range
        let (enc_start, enc_end) =
            encrypted_range_for_plaintext(plaintext_pos, chunk_plaintext_end);

        // Download the encrypted data (including header for decrypt_range)
        // Note: encrypted_range_for_plaintext returns ranges that always start at 0 (for nonce)
        let encrypted_data = storage.download_range(path, enc_start, enc_end).await?;

        if encrypted_data.is_empty() {
            break;
        }

        // Decrypt to get plaintext
        let plaintext = encryption
            .decrypt_range(&encrypted_data, plaintext_pos, chunk_plaintext_end)
            .map_err(|e| CloudStorageError::Download(format!("Decryption failed: {}", e)))?;

        if plaintext.is_empty() {
            break;
        }

        debug!(
            "Decrypted {} plaintext bytes (pos {}-{})",
            plaintext.len(),
            plaintext_pos,
            chunk_plaintext_end
        );

        // Feed to buffer
        buffer.append(&plaintext);

        plaintext_pos += plaintext.len() as u64;

        // Check if we got less than expected (EOF)
        if plaintext.len() < plaintext_chunk_size as usize {
            break;
        }
    }

    buffer.mark_eof();

    info!(
        "Encrypted streaming download complete: {} ({} plaintext bytes)",
        path,
        plaintext_pos - plaintext_start
    );

    Ok(())
}
