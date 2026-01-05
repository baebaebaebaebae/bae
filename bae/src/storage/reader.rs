//! Storage reader utilities for creating storage clients from profiles
use crate::cloud_storage::{CloudStorage, CloudStorageError, S3CloudStorage};
use crate::db::{DbStorageProfile, StorageLocation};
use crate::encryption::{encrypted_range_for_plaintext, EncryptionService, CHUNK_SIZE};
use crate::playback::SharedSparseBuffer;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{debug, info};

/// Chunk size for streaming downloads (64KB)
const STREAMING_CHUNK_SIZE: usize = 65536;

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

        let max_len = (end - start) as usize;
        let mut buffer = vec![0u8; max_len];
        // Use read instead of read_exact to handle ranges that extend past EOF
        let bytes_read = file.read(&mut buffer).await?;
        buffer.truncate(bytes_read);

        Ok(buffer)
    }

    async fn delete(&self, path: &str) -> Result<(), CloudStorageError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(CloudStorageError::Io)
    }
}

/// Download a file to a sparse streaming buffer in chunks.
///
/// Data is appended at the appropriate offset as it arrives, enabling streaming playback
/// with smart seek support.
pub async fn download_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
) -> Result<(), CloudStorageError> {
    info!("Starting streaming download: {}", path);

    // Download the entire file in chunks using range requests
    let mut offset = 0u64;

    loop {
        if buffer.is_cancelled() {
            debug!("Streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        let end = offset + STREAMING_CHUNK_SIZE as u64;
        let chunk = storage.download_range(path, offset, end).await;

        match chunk {
            Ok(data) if data.is_empty() => {
                // End of file
                break;
            }
            Ok(data) => {
                let len = data.len();
                buffer.append_at(offset, &data);
                offset += len as u64;

                // If we got less than requested, we've reached EOF
                if len < STREAMING_CHUNK_SIZE {
                    break;
                }
            }
            Err(e) => {
                // Some storage backends return an error at EOF, treat as EOF
                debug!("Download range returned error (may be EOF): {:?}", e);
                break;
            }
        }
    }

    buffer.set_total_size(offset);
    buffer.mark_eof();

    info!("Streaming download complete: {} bytes", offset);
    Ok(())
}

/// Download a byte range to a sparse streaming buffer in chunks.
///
/// Used for CUE/FLAC tracks where only a portion of the file is needed.
/// Data is written with offsets relative to the start of the range (i.e., buffer offset 0
/// corresponds to file offset start_byte).
pub async fn download_to_streaming_buffer_with_range(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    start_byte: u64,
    end_byte: u64,
) -> Result<(), CloudStorageError> {
    info!(
        "Starting streaming download with range: {} bytes {}-{}",
        path, start_byte, end_byte
    );

    let mut offset = start_byte;
    let total_size = end_byte - start_byte;

    while offset < end_byte {
        if buffer.is_cancelled() {
            debug!("Streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        let chunk_end = (offset + STREAMING_CHUNK_SIZE as u64).min(end_byte);
        let chunk = storage.download_range(path, offset, chunk_end).await?;

        if chunk.is_empty() {
            break;
        }

        let len = chunk.len();
        // Buffer offset is relative to start_byte
        let buffer_offset = offset - start_byte;
        buffer.append_at(buffer_offset, &chunk);
        offset += len as u64;
    }

    buffer.set_total_size(total_size);
    buffer.mark_eof();

    info!(
        "Streaming range download complete: {} bytes",
        offset - start_byte
    );
    Ok(())
}

/// Download an encrypted file to a sparse streaming buffer, decrypting on the fly.
///
/// Downloads encrypted chunks aligned with encryption boundaries, decrypts,
/// and appends plaintext to the buffer at the appropriate offset.
pub async fn download_encrypted_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    encryption: &EncryptionService,
    plaintext_start: u64,
    plaintext_end: Option<u64>,
) -> Result<(), CloudStorageError> {
    const NONCE_SIZE: u64 = 24;
    // Process 4 encryption chunks at a time (256KB plaintext)
    let plaintext_chunk_size = CHUNK_SIZE as u64 * 4;

    info!(
        "Starting encrypted streaming download: {} (plaintext start: {})",
        path, plaintext_start
    );

    // First, download the nonce (always needed)
    let nonce_data = storage.download_range(path, 0, NONCE_SIZE).await?;
    if nonce_data.len() < NONCE_SIZE as usize {
        return Err(CloudStorageError::Download(
            "File too short for nonce".into(),
        ));
    }

    let mut plaintext_pos = plaintext_start;
    // Buffer offset is relative to plaintext_start
    let mut buffer_offset = 0u64;

    loop {
        if buffer.is_cancelled() {
            debug!("Encrypted streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        // Calculate the plaintext range for this iteration
        let chunk_plaintext_end = if let Some(end) = plaintext_end {
            (plaintext_pos + plaintext_chunk_size).min(end)
        } else {
            plaintext_pos + plaintext_chunk_size
        };

        // Calculate the encrypted byte range needed
        let (enc_start, enc_end) =
            encrypted_range_for_plaintext(plaintext_pos, chunk_plaintext_end);
        let expected_enc_len = (enc_end - enc_start) as usize;

        // Download encrypted data (including nonce at start)
        let encrypted_data = storage.download_range(path, enc_start, enc_end).await?;

        if encrypted_data.is_empty() {
            break;
        }

        // If we got less than expected encrypted bytes, adjust the plaintext end
        // to only cover chunks we can actually decrypt
        let actual_plaintext_end = if encrypted_data.len() < expected_enc_len {
            // Calculate how much plaintext we can actually decrypt
            let nonce_size = 24usize;
            let enc_data_after_nonce = encrypted_data.len().saturating_sub(nonce_size);
            let enc_chunk_size = CHUNK_SIZE + 16; // plaintext + tag
            let complete_chunks = enc_data_after_nonce / enc_chunk_size;
            let remaining_enc = enc_data_after_nonce % enc_chunk_size;

            // Calculate total decryptable plaintext
            let complete_plaintext = complete_chunks * CHUNK_SIZE;
            // Last partial chunk: remaining encrypted bytes minus tag (if any)
            let partial_plaintext = remaining_enc.saturating_sub(16);

            if complete_plaintext == 0 && partial_plaintext == 0 {
                debug!("No decryptable data available at EOF");
                break;
            }

            // Calculate actual plaintext end from start position
            plaintext_pos + (complete_plaintext + partial_plaintext) as u64
        } else {
            chunk_plaintext_end
        };

        let plaintext = encryption
            .decrypt_range(&encrypted_data, plaintext_pos, actual_plaintext_end)
            .map_err(|e| CloudStorageError::Download(format!("Decryption failed: {}", e)))?;

        if plaintext.is_empty() {
            break;
        }

        let len = plaintext.len() as u64;
        buffer.append_at(buffer_offset, &plaintext);
        plaintext_pos += len;
        buffer_offset += len;

        // Check if we've reached the requested end
        if let Some(end) = plaintext_end {
            if plaintext_pos >= end {
                break;
            }
        }

        // If we got less than expected, we've hit EOF
        if plaintext.len() < plaintext_chunk_size as usize {
            break;
        }
    }

    buffer.set_total_size(buffer_offset);
    buffer.mark_eof();

    info!(
        "Encrypted streaming download complete: {} plaintext bytes",
        plaintext_pos - plaintext_start
    );
    Ok(())
}
