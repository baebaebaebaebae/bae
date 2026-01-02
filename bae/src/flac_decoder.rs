//! FLAC decoding using libflac-sys FFI bindings.
//!
//! Provides a safe wrapper around libFLAC's streaming decoder.
//! More tolerant of non-standard FLAC files than symphonia.
extern crate libflac_sys;
use tracing::{debug, info};
/// Decoded FLAC metadata and samples
pub struct DecodedFlac {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
}
/// Decode a FLAC file (or range) to PCM samples using libFLAC.
///
/// If start_ms/end_ms are provided, seeks to that time range.
/// Returns interleaved i32 samples.
pub fn decode_flac_range(
    flac_data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedFlac, String> {
    struct DecoderState {
        file_data: Vec<u8>,
        file_pos: usize,
        samples: Vec<i32>,
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
        total_samples: u64,
        current_sample: u64,
        start_sample: Option<u64>,
        end_sample: Option<u64>,
        collecting: bool,
    }
    let state = DecoderState {
        file_data: flac_data.to_vec(),
        file_pos: 0,
        samples: Vec::new(),
        sample_rate: 0,
        channels: 0,
        bits_per_sample: 0,
        total_samples: 0,
        current_sample: 0,
        start_sample: None,
        end_sample: None,
        collecting: start_ms.is_none(),
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
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        frame: *const libflac_sys::FLAC__Frame,
        buffer: *const *const i32,
        client_data: *mut libc::c_void,
    ) -> libflac_sys::FLAC__StreamDecoderWriteStatus {
        let state = unsafe { &mut *(client_data as *mut DecoderState) };
        let frame_ref = unsafe { &*frame };
        let blocksize = frame_ref.header.blocksize as usize;
        let channels = frame_ref.header.channels as usize;
        let frame_sample =
            if frame_ref.header.number_type == libflac_sys::FLAC__FRAME_NUMBER_TYPE_SAMPLE_NUMBER {
                unsafe { frame_ref.header.number.sample_number }
            } else {
                state.current_sample
            };
        if let Some(start) = state.start_sample {
            if frame_sample + blocksize as u64 > start {
                state.collecting = true;
            }
        }
        if let Some(end) = state.end_sample {
            if frame_sample >= end {
                return libflac_sys::FLAC__STREAM_DECODER_WRITE_STATUS_ABORT;
            }
        }
        if state.collecting {
            for i in 0..blocksize {
                let sample_pos = frame_sample + i as u64;
                if let Some(start) = state.start_sample {
                    if sample_pos < start {
                        continue;
                    }
                }
                if let Some(end) = state.end_sample {
                    if sample_pos >= end {
                        break;
                    }
                }
                for ch in 0..channels {
                    let channel_buffer = unsafe { *buffer.add(ch) };
                    let sample = unsafe { *channel_buffer.add(i) };
                    state.samples.push(sample);
                }
            }
        }
        state.current_sample = frame_sample + blocksize as u64;
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
            state.channels = streaminfo.channels;
            state.bits_per_sample = streaminfo.bits_per_sample;
            state.total_samples = streaminfo.total_samples;
            if let Some(start_ms) = state.start_sample.map(|_| ()).and(None::<u64>).or(None) {
                state.start_sample = Some((start_ms * state.sample_rate as u64) / 1000);
            }
        }
    }
    extern "C" fn error_callback(
        _decoder: *const libflac_sys::FLAC__StreamDecoder,
        _status: libflac_sys::FLAC__StreamDecoderErrorStatus,
        _client_data: *mut libc::c_void,
    ) {
    }
    let decoder = unsafe { libflac_sys::FLAC__stream_decoder_new() };
    if decoder.is_null() {
        return Err("Failed to create FLAC decoder".to_string());
    }
    let mut state = Box::new(state);
    let state_ptr = state.as_mut() as *mut DecoderState as *mut libc::c_void;
    let init_status = unsafe {
        libflac_sys::FLAC__stream_decoder_init_stream(
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
        )
    };
    if init_status != libflac_sys::FLAC__STREAM_DECODER_INIT_STATUS_OK {
        unsafe { libflac_sys::FLAC__stream_decoder_delete(decoder) };
        return Err(format!("Failed to initialize decoder: {}", init_status));
    }
    let metadata_ok =
        unsafe { libflac_sys::FLAC__stream_decoder_process_until_end_of_metadata(decoder) };
    if metadata_ok == 0 {
        unsafe {
            libflac_sys::FLAC__stream_decoder_finish(decoder);
            libflac_sys::FLAC__stream_decoder_delete(decoder);
        }
        return Err("Failed to process metadata".to_string());
    }
    if state.sample_rate > 0 {
        if let Some(ms) = start_ms {
            state.start_sample = Some((ms * state.sample_rate as u64) / 1000);
            state.collecting = false;
        }
        if let Some(ms) = end_ms {
            state.end_sample = Some((ms * state.sample_rate as u64) / 1000);
        }
    }
    if let Some(start_sample) = state.start_sample {
        let seek_ok =
            unsafe { libflac_sys::FLAC__stream_decoder_seek_absolute(decoder, start_sample) };
        if seek_ok == 0 {
            debug!("Seek failed, will scan from beginning");
            unsafe {
                libflac_sys::FLAC__stream_decoder_reset(decoder);
                libflac_sys::FLAC__stream_decoder_process_until_end_of_metadata(decoder);
            }
        } else {
            state.collecting = true;
            state.current_sample = start_sample;
        }
    }
    loop {
        let process_ok = unsafe { libflac_sys::FLAC__stream_decoder_process_single(decoder) };
        let decoder_state = unsafe { libflac_sys::FLAC__stream_decoder_get_state(decoder) };
        if decoder_state == libflac_sys::FLAC__STREAM_DECODER_END_OF_STREAM {
            break;
        }
        if decoder_state == libflac_sys::FLAC__STREAM_DECODER_ABORTED {
            break;
        }
        if process_ok == 0 {
            break;
        }
        if let Some(end) = state.end_sample {
            if state.current_sample >= end {
                break;
            }
        }
    }
    unsafe {
        libflac_sys::FLAC__stream_decoder_finish(decoder);
        libflac_sys::FLAC__stream_decoder_delete(decoder);
    }
    debug!(
        "Decoded {} samples ({} frames) from FLAC",
        state.samples.len(),
        state.samples.len() / state.channels.max(1) as usize
    );
    Ok(DecodedFlac {
        samples: state.samples,
        sample_rate: state.sample_rate,
        channels: state.channels,
        bits_per_sample: state.bits_per_sample,
    })
}

/// A seek point entry for building seektables
#[derive(Debug, Clone, Copy)]
pub struct SeekEntry {
    pub sample_number: u64,
    pub byte_offset: u64,
}

/// Scan result with seektable entries
pub struct FlacScanResult {
    pub seektable: Vec<SeekEntry>,
}

/// Build a frame-accurate seektable by scanning for FLAC frame sync codes.
///
/// FLAC frames start with a 14-bit sync code (0x3FFE) followed by frame header.
/// We validate each potential frame header to avoid false positives in
/// compressed audio data, then skip ahead by minimum frame size.
pub fn scan_flac_frames(flac_data: &[u8]) -> Result<FlacScanResult, String> {
    // Parse FLAC headers to get metadata
    if flac_data.len() < 4 || &flac_data[0..4] != b"fLaC" {
        return Err("Invalid FLAC signature".to_string());
    }

    let mut pos = 4;
    let mut sample_rate = 0u32;
    let mut total_samples = 0u64;
    let mut min_frame_size = 0u32;

    // Parse metadata blocks
    loop {
        if pos + 4 > flac_data.len() {
            return Err("Unexpected end of file in metadata".to_string());
        }

        let header_byte = flac_data[pos];
        let is_last = (header_byte & 0x80) != 0;
        let block_type = header_byte & 0x7F;
        let block_size = u32::from_be_bytes([
            0,
            flac_data[pos + 1],
            flac_data[pos + 2],
            flac_data[pos + 3],
        ]) as usize;

        if pos + 4 + block_size > flac_data.len() {
            return Err("Block extends beyond file".to_string());
        }

        if block_type == 0 && block_size >= 18 {
            // STREAMINFO block layout:
            // Bytes 0-1: min block size (16 bits)
            // Bytes 2-3: max block size (16 bits)
            // Bytes 4-6: min frame size (24 bits)
            // Bytes 7-9: max frame size (24 bits)
            // Bytes 10-17: sample rate, channels, bits, total samples
            let block = &flac_data[pos + 4..pos + 4 + block_size];
            min_frame_size =
                ((block[4] as u32) << 16) | ((block[5] as u32) << 8) | (block[6] as u32);
            sample_rate =
                ((block[10] as u32) << 12) | ((block[11] as u32) << 4) | ((block[12] as u32) >> 4);
            total_samples = (((block[13] & 0x0F) as u64) << 32)
                | ((block[14] as u64) << 24)
                | ((block[15] as u64) << 16)
                | ((block[16] as u64) << 8)
                | (block[17] as u64);
        }

        pos += 4 + block_size;
        if is_last {
            break;
        }
    }

    let audio_data_start = pos;
    let audio_data_end = flac_data.len();

    if sample_rate == 0 || total_samples == 0 {
        return Err("Invalid FLAC: no samples or sample rate".to_string());
    }

    // Use minimum frame size from STREAMINFO, or a reasonable default
    let skip_size = if min_frame_size > 0 {
        min_frame_size as usize
    } else {
        1000 // Conservative default ~1KB
    };

    // Scan for frame sync codes
    let mut seektable = Vec::new();
    let mut scan_pos = audio_data_start;
    let mut current_sample: u64 = 0;

    while scan_pos + 4 < audio_data_end && current_sample < total_samples {
        // FLAC frame sync: 14 bits of 1s followed by 0 = 0xFF 0xF8 or 0xFF 0xF9
        if flac_data[scan_pos] == 0xFF && (flac_data[scan_pos + 1] & 0xFE) == 0xF8 {
            // Validate frame header
            if validate_frame_header(flac_data, scan_pos) {
                let stream_offset = (scan_pos - audio_data_start) as u64;
                seektable.push(SeekEntry {
                    sample_number: current_sample,
                    byte_offset: stream_offset,
                });

                // Parse block size from frame header
                let block_size = parse_block_size(flac_data, scan_pos);
                current_sample += block_size as u64;

                // Skip ahead by minimum frame size to avoid false positives
                scan_pos += skip_size;
                continue;
            }
        }
        scan_pos += 1;
    }

    // Add final entry
    seektable.push(SeekEntry {
        sample_number: total_samples,
        byte_offset: (audio_data_end - audio_data_start) as u64,
    });

    let precision_ms = if seektable.len() > 1 && sample_rate > 0 {
        (total_samples as f64 / seektable.len() as f64) / sample_rate as f64 * 1000.0
    } else {
        0.0
    };

    info!(
        "Scanned FLAC: {} frames, {:.1}ms precision",
        seektable.len(),
        precision_ms
    );

    Ok(FlacScanResult { seektable })
}

/// Validate that a potential frame sync is actually a valid FLAC frame header
fn validate_frame_header(data: &[u8], pos: usize) -> bool {
    if pos + 4 >= data.len() {
        return false;
    }

    // Check sync code
    if data[pos] != 0xFF || (data[pos + 1] & 0xFE) != 0xF8 {
        return false;
    }

    // Byte 2: block size code (high nibble) and sample rate code (low nibble)
    let block_size_code = (data[pos + 2] >> 4) & 0x0F;
    let sample_rate_code = data[pos + 2] & 0x0F;

    // Block size code 0 is reserved
    if block_size_code == 0 {
        return false;
    }

    // Sample rate code 15 is invalid
    if sample_rate_code == 15 {
        return false;
    }

    // Byte 3: channel assignment and sample size
    let channel_assignment = (data[pos + 3] >> 4) & 0x0F;
    let sample_size_code = (data[pos + 3] >> 1) & 0x07;

    // Channel assignment > 10 is reserved
    if channel_assignment > 10 {
        return false;
    }

    // Sample size code 3 and 7 are reserved
    if sample_size_code == 3 || sample_size_code == 7 {
        return false;
    }

    // Reserved bit (LSB of byte 3) must be 0
    if data[pos + 3] & 0x01 != 0 {
        return false;
    }

    true
}

/// Parse block size from FLAC frame header
fn parse_block_size(data: &[u8], pos: usize) -> u32 {
    if pos + 4 >= data.len() {
        return 4096; // Default
    }

    let block_size_code = (data[pos + 2] >> 4) & 0x0F;

    match block_size_code {
        0 => 0, // Reserved
        1 => 192,
        2..=5 => 576 * (1 << (block_size_code - 2)),
        6 => {
            // 8-bit value follows
            if pos + 5 < data.len() {
                data[pos + 4] as u32 + 1
            } else {
                4096
            }
        }
        7 => {
            // 16-bit value follows
            if pos + 6 < data.len() {
                let hi = data[pos + 4] as u32;
                let lo = data[pos + 5] as u32;
                ((hi << 8) | lo) + 1
            } else {
                4096
            }
        }
        8..=15 => 256 * (1 << (block_size_code - 8)),
        _ => 4096,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flac_encoder::encode_to_flac;
    #[test]
    fn test_decode_roundtrip() {
        let original_samples: Vec<i32> = (0..44100)
            .map(|i| ((i as f64 * 0.1).sin() * 16000.0) as i32)
            .collect();
        let flac_data = encode_to_flac(&original_samples, 44100, 1, 16).unwrap();
        let decoded = decode_flac_range(&flac_data, None, None).unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.bits_per_sample, 16);
        assert_eq!(decoded.samples.len(), original_samples.len());
    }
}
