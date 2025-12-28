# CUE/FLAC Support

How bae handles CUE sheet + FLAC albums (single FLAC file containing entire album with CUE sheet defining track boundaries).

## The Problem

CUE/FLAC albums have one FLAC file for the entire album. Track boundaries are defined in a CUE sheet with time positions. bae needs to:
1. Stream individual tracks without splitting the file
2. Show correct track durations in players
3. Support seeking within tracks

## How It Works

### Chunking

The FLAC file is chunked as-is during import. No modification to the audio data.

```
album.flac (150MB) → chunks 001-150 (1MB each, encrypted)

Track 1: 00:00-03:45 → chunks 003-048
Track 2: 03:45-07:22 → chunks 048-095
Track 3: 07:22-11:05 → chunks 095-145
```

Track positions within chunks are stored in `track_chunk_coords` table.

### FLAC Headers

FLAC decoders need headers to play audio. The original headers have album-level metadata (total samples = entire album). For individual track playback, we generate corrected headers per track:

**What we store (in `audio_formats` table):**
- STREAMINFO block only (required for playback)
- `total_samples` corrected to track duration
- MD5 signature zeroed (signals "no signature")
- Min/max frame sizes zeroed (signals "unknown")

**What we discard:**
- SEEKTABLE - offsets are wrong for extracted tracks
- VORBIS_COMMENT - album-level tags
- PADDING, APPLICATION - unnecessary

Headers are stored in the database, not in the chunks. During playback, headers are prepended to the decrypted chunk data.

### Frame Rewriting

FLAC frames have sequence numbers. For track 2, frames might start at number 10000 (where track 2 begins in the album). Decoders expect frames to start at 0.

During reassembly, we rewrite frame headers:
1. Find frame boundaries (sync code 0xFFF8)
2. Parse the frame/sample number (UTF-8 variable-length integer)
3. Subtract track start position to get relative number (starts from 0)
4. Re-encode and recalculate CRC-8

This produces a valid standalone FLAC file from the chunk data.

### Seektables

For seeking within CUE/FLAC tracks, we need to map sample positions to byte positions. The original album seektable doesn't help (wrong offsets). We generate a track-specific seektable during import and store it in `audio_formats.flac_seektable`.

## Database Tables

**`track_chunk_coords`** - Where each track's audio lives in the chunk stream:
- `start_chunk_index`, `end_chunk_index`
- `start_byte_offset`, `end_byte_offset`
- `start_time_ms`, `end_time_ms`

**`audio_formats`** - Per-track audio metadata:
- `flac_headers` - Corrected STREAMINFO for prepending
- `flac_seektable` - Track-specific seektable for seeking
- `needs_headers` - True for CUE/FLAC tracks

## Playback Flow

1. Look up track in `track_chunk_coords`
2. Download required chunks (only the ones containing this track)
3. Decrypt chunks
4. Prepend FLAC headers from `audio_formats`
5. Rewrite frame numbers
6. Stream to audio output
