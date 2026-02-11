-- Change audio_formats.file_id FK from RESTRICT (default) to SET NULL.
-- This prevents FK errors when release_files are deleted during storage transfer.
-- SQLite requires table recreation to alter FK constraints.

CREATE TABLE audio_formats_new (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    flac_headers BLOB,
    needs_headers BOOLEAN NOT NULL DEFAULT FALSE,
    start_byte_offset INTEGER,
    end_byte_offset INTEGER,
    pregap_ms INTEGER,
    frame_offset_samples INTEGER,
    exact_sample_count INTEGER,
    sample_rate INTEGER NOT NULL,
    bits_per_sample INTEGER NOT NULL,
    seektable_json TEXT NOT NULL,
    audio_data_start INTEGER NOT NULL,
    file_id TEXT REFERENCES release_files(id) ON DELETE SET NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
);

INSERT INTO audio_formats_new SELECT * FROM audio_formats;
DROP TABLE audio_formats;
ALTER TABLE audio_formats_new RENAME TO audio_formats;

CREATE INDEX idx_audio_formats_track_id ON audio_formats (track_id);
