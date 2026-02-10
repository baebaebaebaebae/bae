CREATE TABLE artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT,
    discogs_artist_id TEXT,
    bandcamp_artist_id TEXT,
    musicbrainz_artist_id TEXT,

    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    year INTEGER,
    bandcamp_album_id TEXT,
    cover_release_id TEXT,
    cover_art_url TEXT,
    is_compilation BOOLEAN NOT NULL DEFAULT FALSE,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE album_discogs (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL UNIQUE,
    discogs_master_id TEXT,
    discogs_release_id TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
);

CREATE TABLE album_musicbrainz (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL UNIQUE,
    musicbrainz_release_group_id TEXT NOT NULL,
    musicbrainz_release_id TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
);

CREATE TABLE album_artists (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(album_id, artist_id)
);

CREATE TABLE releases (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    release_name TEXT,
    year INTEGER,
    discogs_release_id TEXT,
    bandcamp_release_id TEXT,
    format TEXT,
    label TEXT,
    catalog_number TEXT,
    country TEXT,
    barcode TEXT,
    import_status TEXT NOT NULL DEFAULT 'queued',
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
    UNIQUE(album_id, discogs_release_id),
    UNIQUE(album_id, bandcamp_release_id)
);

CREATE TABLE tracks (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    title TEXT NOT NULL,
    disc_number INTEGER,
    track_number INTEGER,
    duration_ms INTEGER,
    discogs_position TEXT,
    import_status TEXT NOT NULL DEFAULT 'queued',
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

CREATE TABLE track_artists (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    role TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
);

CREATE TABLE release_files (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    source_path TEXT,
    encryption_nonce BLOB,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

CREATE TABLE audio_formats (
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
    file_id TEXT REFERENCES release_files(id),
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
);

CREATE TABLE torrents (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    info_hash TEXT NOT NULL UNIQUE,
    magnet_link TEXT,
    torrent_name TEXT NOT NULL,
    total_size_bytes INTEGER NOT NULL,
    piece_length INTEGER NOT NULL,
    num_pieces INTEGER NOT NULL,
    is_seeding BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

CREATE TABLE torrent_piece_mappings (
    id TEXT PRIMARY KEY,
    torrent_id TEXT NOT NULL,
    piece_index INTEGER NOT NULL,
    chunk_ids TEXT NOT NULL,
    start_byte_in_first_chunk INTEGER NOT NULL,
    end_byte_in_last_chunk INTEGER NOT NULL,
    FOREIGN KEY (torrent_id) REFERENCES torrents (id) ON DELETE CASCADE,
    UNIQUE(torrent_id, piece_index)
);

CREATE TABLE library_images (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE storage_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    location TEXT NOT NULL,
    location_path TEXT NOT NULL,
    encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    is_home BOOLEAN NOT NULL DEFAULT FALSE,
    cloud_bucket TEXT,
    cloud_region TEXT,
    cloud_endpoint TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE release_storage (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    storage_profile_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (storage_profile_id) REFERENCES storage_profiles (id)
);

CREATE TABLE imports (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'preparing',
    release_id TEXT REFERENCES releases(id),
    album_title TEXT NOT NULL,
    artist_name TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    error_message TEXT
);

-- Indexes
CREATE INDEX idx_artists_discogs_id ON artists (discogs_artist_id);
CREATE INDEX idx_artists_mb_id ON artists (musicbrainz_artist_id);
CREATE INDEX idx_artists_name ON artists (name COLLATE NOCASE);
CREATE INDEX idx_album_artists_album_id ON album_artists (album_id);
CREATE INDEX idx_album_artists_artist_id ON album_artists (artist_id);
CREATE INDEX idx_track_artists_track_id ON track_artists (track_id);
CREATE INDEX idx_track_artists_artist_id ON track_artists (artist_id);
CREATE INDEX idx_releases_album_id ON releases (album_id);
CREATE INDEX idx_tracks_release_id ON tracks (release_id);
CREATE INDEX idx_release_files_release_id ON release_files (release_id);
CREATE INDEX idx_torrents_release_id ON torrents (release_id);
CREATE INDEX idx_torrents_info_hash ON torrents (info_hash);
CREATE INDEX idx_torrent_piece_mappings_torrent_id ON torrent_piece_mappings (torrent_id);
CREATE INDEX idx_audio_formats_track_id ON audio_formats (track_id);
CREATE INDEX idx_library_images_type ON library_images (type);
CREATE INDEX idx_release_storage_profile_id ON release_storage (storage_profile_id);
CREATE INDEX idx_imports_status ON imports (status);
CREATE INDEX idx_imports_release_id ON imports (release_id);
