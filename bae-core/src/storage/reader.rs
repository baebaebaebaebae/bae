//! Storage reader utilities
use crate::cloud_storage::{CloudStorage, CloudStorageError};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

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
