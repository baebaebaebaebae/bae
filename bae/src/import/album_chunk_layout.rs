use crate::cue_flac::CueFlacProcessor;
use crate::import::types::FileToChunks;
use crate::import::types::{CueFlacLayoutData, CueFlacMetadata, DiscoveredFile, TrackFile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;
extern crate libflac_sys;
/// FLAC file metadata extracted via libFLAC
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlacInfo {
    /// Seektable mapping sample positions to byte positions
    pub seektable: HashMap<u64, u64>,
    /// Sample rate in Hz (e.g., 44100)
    pub sample_rate: u32,
    /// Total number of samples in the file
    pub total_samples: u64,
}
impl FlacInfo {
    /// Calculate duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.total_samples * 1000) / self.sample_rate as u64
    }
}
/// Build a seektable and extract metadata from a FLAC file using libFLAC
pub fn build_seektable(flac_path: &Path) -> Result<FlacInfo, String> {
    use tracing::debug;
    debug!("Building seektable for: {:?}", flac_path);
    let file_data =
        std::fs::read(flac_path).map_err(|e| format!("Failed to read FLAC file: {}", e))?;
    debug!("Read {} bytes from FLAC file", file_data.len());
    struct DecoderState {
        file_data: Vec<u8>,
        file_pos: usize,
        seektable: HashMap<u64, u64>,
        cumulative_samples: u64,
        sample_rate: u32,
        total_samples: u64,
    }
    let state = DecoderState {
        file_data,
        file_pos: 0,
        seektable: HashMap::new(),
        cumulative_samples: 0,
        sample_rate: 0,
        total_samples: 0,
    };
    extern "C" fn read_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        buffer: *mut u8,
        bytes: *mut libc::size_t,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderReadStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let bytes_needed = unsafe { *bytes };
        let remaining = state.file_data.len().saturating_sub(state.file_pos);
        if remaining == 0 {
            unsafe { *bytes = 0 };
            return libflac_sys::FLAC__STREAM_DECODER_READ_STATUS_END_OF_STREAM;
        }
        let to_read = bytes_needed.min(remaining);
        unsafe {
            std::ptr::copy_nonoverlapping(
                state.file_data.as_ptr().add(state.file_pos),
                buffer,
                to_read,
            );
        }
        state.file_pos += to_read;
        unsafe { *bytes = to_read as libc::size_t };
        libflac_sys::FLAC__STREAM_DECODER_READ_STATUS_CONTINUE
    }
    extern "C" fn seek_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        absolute_byte_offset: u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderSeekStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        if absolute_byte_offset as usize > state.file_data.len() {
            return libflac_sys::FLAC__STREAM_DECODER_SEEK_STATUS_ERROR;
        }
        state.file_pos = absolute_byte_offset as usize;
        libflac_sys::FLAC__STREAM_DECODER_SEEK_STATUS_OK
    }
    extern "C" fn tell_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        absolute_byte_offset: *mut u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderTellStatus {
        let state = unsafe { &*(client_data as *const DecoderState) };
        unsafe { *absolute_byte_offset = state.file_pos as u64 };
        libflac_sys::FLAC__STREAM_DECODER_TELL_STATUS_OK
    }
    extern "C" fn length_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        stream_length: *mut u64,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderLengthStatus {
        let state = unsafe { &*(client_data as *const DecoderState) };
        unsafe { *stream_length = state.file_data.len() as u64 };
        libflac_sys::FLAC__STREAM_DECODER_LENGTH_STATUS_OK
    }
    extern "C" fn eof_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__bool {
        let state = unsafe { &*(client_data as *const DecoderState) };
        (state.file_pos >= state.file_data.len()) as libflac_sys::FLAC__bool
    }
    extern "C" fn write_callback(
        decoder: *const libflac_sys::FLAC__StreamDecoder,
        frame: *const libflac_sys::FLAC__Frame,
        _buffer: *const *const i32,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let frame_ref = unsafe { &*frame };
        let sample_number = match frame_ref.header.number_type {
            libflac_sys::FLAC__FRAME_NUMBER_TYPE_FRAME_NUMBER => state.cumulative_samples,
            libflac_sys::FLAC__FRAME_NUMBER_TYPE_SAMPLE_NUMBER => unsafe {
                let sample_num = frame_ref.header.number.sample_number;
                state.cumulative_samples = sample_num;
                sample_num
            },
            _ => {
                return libflac_sys::FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE;
            }
        };
        let mut byte_pos: u64 = 0;
        unsafe {
            libflac_sys::FLAC__stream_decoder_get_decode_position(decoder as *mut _, &mut byte_pos);
        }
        if byte_pos == 0 {
            byte_pos = state.file_pos as u64;
        }
        state.seektable.insert(sample_number, byte_pos);
        if frame_ref.header.number_type == libflac_sys::FLAC__FRAME_NUMBER_TYPE_FRAME_NUMBER {
            let blocksize: u64 = 1152;
            state.cumulative_samples += blocksize;
        }
        libflac_sys::FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE
    }
    extern "C" fn metadata_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        metadata: *const libflac_sys::FLAC__StreamMetadata,
        client_data: *mut libc::c_void,
    ) {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let metadata_ref = unsafe { &*metadata };
        if metadata_ref.type_ == libflac_sys::FLAC__METADATA_TYPE_STREAMINFO {
            let streaminfo = unsafe { &metadata_ref.data.stream_info };
            state.sample_rate = streaminfo.sample_rate;
            state.total_samples = streaminfo.total_samples;
        }
    }
    extern "C" fn error_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        _status: libflac_sys::FLAC__StreamDecoderErrorStatus,
        _client_data: *mut libc::c_void,
    ) {
        let _ = _status;
        let _ = _client_data;
    }
    let decoder = unsafe { libflac_sys::FLAC__stream_decoder_new() };
    if decoder.is_null() {
        return Err("Failed to create FLAC decoder".to_string());
    }
    debug!("Created FLAC decoder");
    let mut state = Box::new(state);
    let state_ptr = state.as_mut() as *mut DecoderState as *mut libc::c_void;
    unsafe {
        debug!("Initializing FLAC stream decoder...");
        let result = libflac_sys::FLAC__stream_decoder_init_stream(
            decoder,
            Some(read_callback),
            Some(seek_callback),
            Some(tell_callback),
            Some(length_callback),
            Some(eof_callback),
            Some(write_callback),
            Some(metadata_callback),
            Some(error_callback),
            state_ptr,
        );
        if result != libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_OK {
            let error_msg = match result {
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_OK => "OK (unexpected)",
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_ERROR_OPENING_FILE => {
                    "Error opening file"
                }
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_ALREADY_INITIALIZED => {
                    "Already initialized"
                }
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_UNSUPPORTED_CONTAINER => {
                    "Unsupported container"
                }
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_INVALID_CALLBACKS => {
                    "Invalid callbacks"
                }
                libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_MEMORY_ALLOCATION_ERROR => {
                    "Memory allocation error"
                }
                _ => "Unknown error",
            };
            libflac_sys::FLAC__stream_decoder_delete(decoder);
            return Err(format!(
                "Failed to initialize FLAC decoder: {} ({})",
                error_msg, result
            ));
        }
        debug!("FLAC decoder initialized, starting to process...");
        let mut frames_processed = 0;
        let mut consecutive_zeros = 0;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations % 1000 == 0 {
                debug!(
                    "Processed {} iterations, {} frames",
                    iterations, frames_processed
                );
            }
            let result = libflac_sys::FLAC__stream_decoder_process_single(decoder);
            let state_enum = libflac_sys::FLAC__stream_decoder_get_state(decoder);
            if state_enum == libflac_sys::FLAC__STREAM_DECODER_END_OF_STREAM {
                break;
            }
            if state_enum == libflac_sys::FLAC__STREAM_DECODER_ABORTED {
                libflac_sys::FLAC__stream_decoder_delete(decoder);
                return Err("FLAC decoder aborted".to_string());
            }
            if result == 0 {
                consecutive_zeros += 1;
                if consecutive_zeros > 3 {
                    let final_state = libflac_sys::FLAC__stream_decoder_get_state(decoder);
                    if final_state != libflac_sys::FLAC__STREAM_DECODER_END_OF_STREAM {
                        libflac_sys::FLAC__stream_decoder_delete(decoder);
                        return Err(format!(
                            "FLAC decoder stuck: process_single returned 0 {} times, state: {}",
                            consecutive_zeros, final_state,
                        ));
                    }
                    break;
                }
            } else {
                consecutive_zeros = 0;
                frames_processed += 1;
            }
            if frames_processed > 10_000_000 {
                libflac_sys::FLAC__stream_decoder_delete(decoder);
                return Err(format!(
                    "FLAC decoder processed too many frames ({}), possible infinite loop",
                    frames_processed,
                ));
            }
        }
        debug!(
            "Finished processing, frames: {}, seektable entries: {}, sample_rate: {}, total_samples: {}",
            frames_processed, state.seektable.len(), state.sample_rate, state
            .total_samples
        );
        libflac_sys::FLAC__stream_decoder_finish(decoder);
        libflac_sys::FLAC__stream_decoder_delete(decoder);
        Ok(FlacInfo {
            seektable: state.seektable,
            sample_rate: state.sample_rate,
            total_samples: state.total_samples,
        })
    }
}
/// Find exact byte range for a track using seektable and sample rate
///
/// Converts time positions to sample positions, then looks up byte positions in the seektable.
/// Uses libFLAC-derived metadata instead of symphonia for better compatibility with
/// non-standard FLAC files.
pub fn find_track_byte_range(
    flac_path: &Path,
    start_time_ms: u64,
    end_time_ms: Option<u64>,
    seektable: &HashMap<u64, u64>,
    sample_rate: u32,
) -> Result<(i64, i64), String> {
    let file_size = std::fs::metadata(flac_path)
        .map_err(|e| format!("Failed to get file metadata: {}", e))?
        .len() as i64;
    let start_sample = (start_time_ms * sample_rate as u64) / 1000;
    let start_byte = lookup_seektable(seektable, start_sample)? as i64;
    let end_byte = if let Some(end_ms) = end_time_ms {
        let end_sample = (end_ms * sample_rate as u64) / 1000;
        lookup_seektable(seektable, end_sample)? as i64
    } else {
        file_size
    };
    Ok((start_byte, end_byte))
}
/// Look up a sample position in the seektable, finding the nearest entry
fn lookup_seektable(seektable: &HashMap<u64, u64>, sample: u64) -> Result<u64, String> {
    if let Some(&byte_pos) = seektable.get(&sample) {
        return Ok(byte_pos);
    }
    let mut sorted_samples: Vec<u64> = seektable.keys().copied().collect();
    sorted_samples.sort_unstable();
    match sorted_samples.binary_search(&sample) {
        Ok(idx) => Ok(seektable[&sorted_samples[idx]]),
        Err(idx) => {
            let (before, after) = if idx == 0 {
                (None, Some(sorted_samples[0]))
            } else if idx >= sorted_samples.len() {
                (Some(sorted_samples[sorted_samples.len() - 1]), None)
            } else {
                (Some(sorted_samples[idx - 1]), Some(sorted_samples[idx]))
            };
            match (before, after) {
                (Some(b), Some(a)) => {
                    let dist_b = sample.abs_diff(b);
                    let dist_a = sample.abs_diff(a);
                    if dist_b <= dist_a {
                        Ok(seektable[&b])
                    } else {
                        Ok(seektable[&a])
                    }
                }
                (Some(b), None) => Ok(seektable[&b]),
                (None, Some(a)) => Ok(seektable[&a]),
                (None, None) => Err("Empty seektable".to_string()),
            }
        }
    }
}
/// Return type for `build_chunk_track_mappings`.
///
/// Contains:
/// - `chunk_to_track`: Maps chunk indices to track IDs
/// - `track_chunk_counts`: Maps track IDs to their total chunk counts
/// - `cue_flac_data`: Pre-calculated CUE/FLAC layout data by file path
type ChunkTrackMappings = (
    HashMap<i32, Vec<String>>,
    HashMap<String, usize>,
    HashMap<PathBuf, CueFlacLayoutData>,
);
/// Analysis of album's physical layout for chunking and progress tracking during import.
///
/// Built before pipeline starts from discovered files and track mappings.
/// Contains the "planning" phase results that drive both chunk streaming and progress tracking.
pub struct AlbumChunkLayout {
    /// Total number of chunks across all files.
    /// Used to calculate overall import progress percentage.
    pub total_chunks: usize,
    /// Maps each file to its chunk range and byte offsets within those chunks.
    /// Used by the chunk producer to stream chunks in sequence.
    /// A file can represent either a single track or a complete disc image containing multiple tracks.
    pub files_to_chunks: Vec<FileToChunks>,
    /// Maps chunk indices to track IDs.
    /// A chunk can contain data from multiple tracks (when small files share a chunk).
    /// Only chunks containing track audio data have entries; chunks with only non-track
    /// files (cover.jpg, .cue) are omitted.
    /// Used by progress emitter to attribute chunk completion to specific tracks.
    pub chunk_to_track: HashMap<i32, Vec<String>>,
    /// Maps track IDs to their total chunk counts.
    /// Used by progress emitter to calculate per-track progress percentages.
    pub track_chunk_counts: HashMap<String, usize>,
    /// Pre-calculated CUE/FLAC layout data for each CUE/FLAC file.
    /// Contains parsed CUE sheets, FLAC headers, and per-track chunk ranges.
    /// This is calculated once during layout analysis and passed to metadata persistence.
    pub cue_flac_data: HashMap<PathBuf, CueFlacLayoutData>,
}
impl AlbumChunkLayout {
    /// Analyze discovered files and build complete chunk/track layout.
    ///
    /// This is the "planning" phase - we figure out the entire chunk structure
    /// before streaming any data, so we can track progress and persist metadata correctly.
    ///
    /// For CUE/FLAC imports, uses pre-parsed CUE metadata from the validation phase
    /// to avoid redundant parsing.
    pub fn build(
        discovered_files: Vec<DiscoveredFile>,
        tracks_to_files: &[TrackFile],
        chunk_size: usize,
        cue_flac_metadata: Option<std::collections::HashMap<PathBuf, CueFlacMetadata>>,
    ) -> Result<Self, String> {
        let files_to_chunks = calculate_files_to_chunks(&discovered_files, chunk_size);
        let total_chunks = files_to_chunks
            .last()
            .map(|mapping| (mapping.end_chunk_index + 1) as usize)
            .unwrap_or(0);
        let (chunk_to_track, track_chunk_counts, cue_flac_data) = build_chunk_track_mappings(
            &files_to_chunks,
            tracks_to_files,
            chunk_size,
            cue_flac_metadata,
        )?;
        Ok(AlbumChunkLayout {
            total_chunks,
            files_to_chunks,
            chunk_to_track,
            track_chunk_counts,
            cue_flac_data,
        })
    }
}
/// Calculate file-to-chunk mappings from files discovered during import validation.
///
/// Treats all files as a single concatenated byte stream, divided into fixed-size chunks.
/// Each file mapping records which chunks it spans and byte offsets within those chunks.
/// This enables efficient streaming: open each file once, read its chunks sequentially.
fn calculate_files_to_chunks(files: &[DiscoveredFile], chunk_size: usize) -> Vec<FileToChunks> {
    let mut total_bytes_processed = 0u64;
    let mut files_to_chunks = Vec::new();
    for file in files {
        let start_byte = total_bytes_processed;
        let end_byte = total_bytes_processed + file.size;
        let start_chunk_index = (start_byte / chunk_size as u64) as i32;
        let end_chunk_index = ((end_byte - 1) / chunk_size as u64) as i32;
        files_to_chunks.push(FileToChunks {
            file_path: file.path.clone(),
            start_chunk_index,
            end_chunk_index,
            start_byte_offset: (start_byte % chunk_size as u64) as i64,
            end_byte_offset: ((end_byte - 1) % chunk_size as u64) as i64,
        });
        total_bytes_processed = end_byte;
    }
    files_to_chunks
}
/// Build chunk→track mappings for progress tracking during import.
///
/// Creates reverse mappings from chunks to tracks so we can:
/// 1. Identify which tracks a chunk belongs to when it completes
/// 2. Count how many chunks each track needs to mark it complete
///
/// This enables progressive UI updates as tracks finish, rather than waiting for the entire album.
///
/// For CUE/FLAC files, calculates precise per-track chunk ranges based on pre-parsed CUE sheet timing.
/// For regular files, maps all chunks to all tracks in that file.
fn build_chunk_track_mappings(
    files_to_chunks: &[FileToChunks],
    track_files: &[TrackFile],
    chunk_size: usize,
    cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
) -> Result<ChunkTrackMappings, String> {
    let mut file_to_tracks: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut file_to_track_files: HashMap<PathBuf, Vec<&TrackFile>> = HashMap::new();
    for track_file in track_files {
        file_to_tracks
            .entry(track_file.file_path.clone())
            .or_default()
            .push(track_file.db_track_id.clone());
        file_to_track_files
            .entry(track_file.file_path.clone())
            .or_default()
            .push(track_file);
    }
    let mut chunk_to_track: HashMap<i32, Vec<String>> = HashMap::new();
    let mut track_chunk_counts: HashMap<String, usize> = HashMap::new();
    let mut cue_flac_data: HashMap<PathBuf, CueFlacLayoutData> = HashMap::new();
    for file_to_chunks in files_to_chunks.iter() {
        let Some(track_ids) = file_to_tracks.get(&file_to_chunks.file_path) else {
            continue;
        };
        if let Some(cue_metadata) = cue_flac_metadata
            .as_ref()
            .and_then(|map| map.get(&file_to_chunks.file_path))
        {
            let track_files_for_file = file_to_track_files
                .get(&file_to_chunks.file_path)
                .ok_or("Track files not found")?;
            debug!(
                "Processing CUE/FLAC file: {}",
                file_to_chunks.file_path.display()
            );
            let flac_headers = CueFlacProcessor::extract_flac_headers(&cue_metadata.flac_path)
                .map_err(|e| format!("Failed to extract FLAC headers: {}", e))?;
            debug!(
                "Building seektable for FLAC file: {}",
                cue_metadata.flac_path.display()
            );
            let flac_info = build_seektable(&cue_metadata.flac_path)
                .map_err(|e| format!("Failed to build seektable: {}", e))?;
            debug!("Seektable built with {} entries", flac_info.seektable.len());
            let mut track_byte_ranges = HashMap::new();
            let chunk_size_i64 = chunk_size as i64;
            for (cue_track_idx, cue_track) in cue_metadata.cue_sheet.tracks.iter().enumerate() {
                let Some(track_file) = track_files_for_file.get(cue_track_idx) else {
                    continue;
                };
                let (start_byte, end_byte) = find_track_byte_range(
                    &cue_metadata.flac_path,
                    cue_track.start_time_ms,
                    cue_track.end_time_ms,
                    &flac_info.seektable,
                    flac_info.sample_rate,
                )?;
                debug!(
                    "Track {}: time {}ms-{}ms → bytes {}-{} ({}MB)",
                    cue_track.number,
                    cue_track.start_time_ms,
                    cue_track.end_time_ms.unwrap_or(0),
                    start_byte,
                    end_byte,
                    (end_byte - start_byte) / 1_000_000
                );
                let file_start_byte = file_to_chunks.start_byte_offset
                    + (file_to_chunks.start_chunk_index as i64 * chunk_size_i64);
                let absolute_start_byte = file_start_byte + start_byte;
                let absolute_end_byte = file_start_byte + end_byte;
                let start_chunk_index = (absolute_start_byte / chunk_size_i64) as i32;
                let end_chunk_index = ((absolute_end_byte - 1) / chunk_size_i64) as i32;
                debug!(
                    "Track {}: chunks {}-{} ({} chunks)",
                    cue_track.number,
                    start_chunk_index,
                    end_chunk_index,
                    end_chunk_index - start_chunk_index + 1
                );
                track_byte_ranges.insert(
                    track_file.db_track_id.clone(),
                    (absolute_start_byte, absolute_end_byte),
                );
                let chunk_count = (end_chunk_index - start_chunk_index + 1) as usize;
                for chunk_idx in start_chunk_index..=end_chunk_index {
                    let entry = chunk_to_track.entry(chunk_idx).or_default();
                    if !entry.contains(&track_file.db_track_id) {
                        entry.push(track_file.db_track_id.clone());
                    }
                }
                *track_chunk_counts
                    .entry(track_file.db_track_id.clone())
                    .or_insert(0) += chunk_count;
            }
            cue_flac_data.insert(
                file_to_chunks.file_path.clone(),
                CueFlacLayoutData {
                    cue_sheet: cue_metadata.cue_sheet.clone(),
                    flac_headers,
                    track_byte_ranges,
                    seektable: Some(flac_info.seektable),
                },
            );
        } else {
            let chunk_count =
                (file_to_chunks.end_chunk_index - file_to_chunks.start_chunk_index + 1) as usize;
            for chunk_idx in file_to_chunks.start_chunk_index..=file_to_chunks.end_chunk_index {
                let entry = chunk_to_track.entry(chunk_idx).or_default();
                for track_id in track_ids {
                    if !entry.contains(track_id) {
                        entry.push(track_id.clone());
                    }
                }
            }
            for track_id in track_ids {
                *track_chunk_counts.entry(track_id.clone()).or_insert(0) += chunk_count;
            }
        }
    }
    Ok((chunk_to_track, track_chunk_counts, cue_flac_data))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_calculate_file_mappings_integration_test_sizes() {
        let chunk_size = 1024 * 1024;
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("file1.flac"),
                size: 2_097_152,
            },
            DiscoveredFile {
                path: PathBuf::from("file2.flac"),
                size: 3_145_728,
            },
            DiscoveredFile {
                path: PathBuf::from("file3.flac"),
                size: 1_572_864,
            },
        ];
        let tracks = vec![
            TrackFile {
                db_track_id: "track-1".to_string(),
                file_path: PathBuf::from("file1.flac"),
            },
            TrackFile {
                db_track_id: "track-2".to_string(),
                file_path: PathBuf::from("file2.flac"),
            },
            TrackFile {
                db_track_id: "track-3".to_string(),
                file_path: PathBuf::from("file3.flac"),
            },
        ];
        let layout = AlbumChunkLayout::build(files, &tracks, chunk_size, None).unwrap();
        assert_eq!(layout.files_to_chunks.len(), 3);
        assert_eq!(
            layout.files_to_chunks[0].file_path,
            PathBuf::from("file1.flac")
        );
        assert_eq!(layout.files_to_chunks[0].start_chunk_index, 0);
        assert_eq!(layout.files_to_chunks[0].end_chunk_index, 1);
        assert_eq!(layout.files_to_chunks[0].start_byte_offset, 0);
        assert_eq!(
            layout.files_to_chunks[1].file_path,
            PathBuf::from("file2.flac")
        );
        assert_eq!(layout.files_to_chunks[1].start_chunk_index, 2);
        assert_eq!(layout.files_to_chunks[1].end_chunk_index, 4);
        assert_eq!(
            layout.files_to_chunks[2].file_path,
            PathBuf::from("file3.flac")
        );
        assert_eq!(layout.files_to_chunks[2].start_chunk_index, 5);
        assert_eq!(layout.files_to_chunks[2].end_chunk_index, 6);
        assert_eq!(
            layout.files_to_chunks[0].end_chunk_index + 1,
            layout.files_to_chunks[1].start_chunk_index,
        );
        assert_eq!(
            layout.files_to_chunks[1].end_chunk_index + 1,
            layout.files_to_chunks[2].start_chunk_index,
        );
        assert_eq!(layout.total_chunks, 7);
    }
    #[test]
    fn test_chunk_boundaries_with_partial_final_chunk() {
        let chunk_size = 1024 * 1024;
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("file1.flac"),
                size: 2_097_152,
            },
            DiscoveredFile {
                path: PathBuf::from("file2.flac"),
                size: 3_145_728,
            },
            DiscoveredFile {
                path: PathBuf::from("file3.flac"),
                size: 1_572_864,
            },
        ];
        let _mappings = calculate_files_to_chunks(&files, chunk_size);
        let file3_start_byte = 2_097_152u64 + 3_145_728;
        let file3_end_byte = file3_start_byte + 1_572_864;
        let chunk_6_start_byte = 6 * chunk_size as u64;
        let file3_bytes_in_chunk_6 = file3_end_byte - chunk_6_start_byte;
        assert_eq!(
            file3_bytes_in_chunk_6, 524_288,
            "File 3 should only use 0.5MB of chunk 6",
        );
    }
    #[test]
    fn test_multiple_small_files_share_chunks() {
        let chunk_size = 1024 * 1024;
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("track1.flac"),
                size: 500_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track2.flac"),
                size: 300_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track3.flac"),
                size: 400_000,
            },
        ];
        let tracks = vec![
            TrackFile {
                db_track_id: "track-1".to_string(),
                file_path: PathBuf::from("track1.flac"),
            },
            TrackFile {
                db_track_id: "track-2".to_string(),
                file_path: PathBuf::from("track2.flac"),
            },
            TrackFile {
                db_track_id: "track-3".to_string(),
                file_path: PathBuf::from("track3.flac"),
            },
        ];
        let layout = AlbumChunkLayout::build(files, &tracks, chunk_size, None).unwrap();
        assert_eq!(layout.total_chunks, 2);
        let chunk_0_tracks = layout.chunk_to_track.get(&0).unwrap();
        assert_eq!(chunk_0_tracks.len(), 3);
        assert!(chunk_0_tracks.contains(&"track-1".to_string()));
        assert!(chunk_0_tracks.contains(&"track-2".to_string()));
        assert!(chunk_0_tracks.contains(&"track-3".to_string()));
        let chunk_1_tracks = layout.chunk_to_track.get(&1).unwrap();
        assert_eq!(chunk_1_tracks.len(), 1);
        assert!(chunk_1_tracks.contains(&"track-3".to_string()));
        assert_eq!(layout.track_chunk_counts.get("track-1"), Some(&1));
        assert_eq!(layout.track_chunk_counts.get("track-2"), Some(&1));
        assert_eq!(layout.track_chunk_counts.get("track-3"), Some(&2));
    }
    #[test]
    fn test_non_track_files_excluded_from_mappings() {
        let chunk_size = 1024 * 1024;
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("cover.jpg"),
                size: 200_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track1.flac"),
                size: 900_000,
            },
            DiscoveredFile {
                path: PathBuf::from("album.cue"),
                size: 5_000,
            },
        ];
        let tracks = vec![TrackFile {
            db_track_id: "track-1".to_string(),
            file_path: PathBuf::from("track1.flac"),
        }];
        let layout = AlbumChunkLayout::build(files, &tracks, chunk_size, None).unwrap();
        assert_eq!(layout.total_chunks, 2);
        let chunk_0_tracks = layout.chunk_to_track.get(&0).unwrap();
        assert_eq!(chunk_0_tracks.len(), 1);
        assert_eq!(chunk_0_tracks[0], "track-1");
        let chunk_1_tracks = layout.chunk_to_track.get(&1).unwrap();
        assert_eq!(chunk_1_tracks.len(), 1);
        assert_eq!(chunk_1_tracks[0], "track-1");
        assert_eq!(layout.track_chunk_counts.get("track-1"), Some(&2));
    }
    #[test]
    fn test_cue_flac_multiple_tracks_same_file() {
        let chunk_size = 1024 * 1024;
        let files = vec![DiscoveredFile {
            path: PathBuf::from("album.flac"),
            size: 3_000_000,
        }];
        let tracks = vec![
            TrackFile {
                db_track_id: "track-1".to_string(),
                file_path: PathBuf::from("album.flac"),
            },
            TrackFile {
                db_track_id: "track-2".to_string(),
                file_path: PathBuf::from("album.flac"),
            },
            TrackFile {
                db_track_id: "track-3".to_string(),
                file_path: PathBuf::from("album.flac"),
            },
        ];
        let layout = AlbumChunkLayout::build(files, &tracks, chunk_size, None).unwrap();
        assert_eq!(layout.total_chunks, 3);
        for chunk_idx in 0..3 {
            let chunk_tracks = layout.chunk_to_track.get(&chunk_idx).unwrap();
            assert_eq!(chunk_tracks.len(), 3);
            assert!(chunk_tracks.contains(&"track-1".to_string()));
            assert!(chunk_tracks.contains(&"track-2".to_string()));
            assert!(chunk_tracks.contains(&"track-3".to_string()));
        }
        assert_eq!(layout.track_chunk_counts.get("track-1"), Some(&3));
        assert_eq!(layout.track_chunk_counts.get("track-2"), Some(&3));
        assert_eq!(layout.track_chunk_counts.get("track-3"), Some(&3));
    }
    #[test]
    fn test_mixed_scenario_with_edge_cases() {
        let chunk_size = 1024 * 1024;
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("cover.jpg"),
                size: 100_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track1.flac"),
                size: 200_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track2.flac"),
                size: 800_000,
            },
            DiscoveredFile {
                path: PathBuf::from("track3.flac"),
                size: 2_000_000,
            },
        ];
        let tracks = vec![
            TrackFile {
                db_track_id: "track-1".to_string(),
                file_path: PathBuf::from("track1.flac"),
            },
            TrackFile {
                db_track_id: "track-2".to_string(),
                file_path: PathBuf::from("track2.flac"),
            },
            TrackFile {
                db_track_id: "track-3".to_string(),
                file_path: PathBuf::from("track3.flac"),
            },
        ];
        let layout = AlbumChunkLayout::build(files, &tracks, chunk_size, None).unwrap();
        assert_eq!(layout.total_chunks, 3);
        let chunk_0 = layout.chunk_to_track.get(&0).unwrap();
        assert_eq!(chunk_0.len(), 2);
        assert!(chunk_0.contains(&"track-1".to_string()));
        assert!(chunk_0.contains(&"track-2".to_string()));
        let chunk_1 = layout.chunk_to_track.get(&1).unwrap();
        assert_eq!(chunk_1.len(), 2);
        assert!(chunk_1.contains(&"track-2".to_string()));
        assert!(chunk_1.contains(&"track-3".to_string()));
        let chunk_2 = layout.chunk_to_track.get(&2).unwrap();
        assert_eq!(chunk_2.len(), 1);
        assert_eq!(chunk_2[0], "track-3");
    }
}
