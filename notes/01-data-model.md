# Data Model: Releases, Files, and Images

## Libraries and storage

A **library** is the logical entity -- a music collection. It has an identity (`library_id`), a name, and an encryption key. It lives primarily at the **library home** (`~/.bae/libraries/{uuid}/`), where desktop writes the authoritative DB.

A library can optionally have a **cloud home** -- a single cloud location (Google Drive folder, S3 bucket, etc.) that serves as the collaborative hub for multi-device sync. The cloud home holds changesets, snapshots, images, and release files. There is exactly one cloud home per library. It is configured in `config.yaml`.

Release files live in one of two places:

- **Library home** (`~/.bae/libraries/{uuid}/storage/`) -- local files, always exists
- **Cloud home** (`cloud-home/storage/`) -- encrypted, uploaded when cloud is configured
- **Unmanaged** -- files stay wherever the user has them on disk, bae just indexes them

```
Library "My Music" (lib-111)
  ├── local home: ~/.bae/libraries/lib-111/    <- desktop writes here
  └── cloud home: Google Drive / S3 bucket     <- sync + images + release files
```

Release files are separate from metadata. A file is either local, in the cloud home, or unmanaged.

## Directory layout

### bae directory (`~/.bae/`)

Used by desktop -- this is what opens when you launch the app. Contains all local libraries. `active-library` is the UUID of the currently active library -- absent means use the first (or only) library.

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

bae-server doesn't use `~/.bae/` -- it syncs from the cloud home.

### Library home

The library home is where desktop runs. It holds the authoritative DB, device-specific config, and local release files.

```
~/.bae/libraries/{uuid}/
  config.yaml                  # device-specific settings (not synced)
  library.db                   # SQLite -- all metadata
  images/ab/cd/{id}            # library images (covers, artist photos -- no extension, content type in DB)
  storage/ab/cd/{file_id}      # release files (no extension, content type in DB)
  pending_deletions.json       # deferred file deletion manifest
```

**`config.yaml`** -- device-specific settings. Not synced, only at the library home. Contains keyring hint flags (`discogs_key_stored`, `encryption_key_stored`), torrent settings, subsonic settings, cloud home configuration, and device_id. Non-secret only -- credentials go in the keyring.

### Keyring (OS keyring, namespaced by library_id)

Managed by `KeyService`. On macOS, uses the protected data store with iCloud Keychain sync.

- `encryption_master_key` -- one per library, used for all file and metadata encryption
- `discogs_api_key`
- `s3_access_key:{profile_id}` -- per-profile S3 access key (cloud profiles only)
- `s3_secret_key:{profile_id}` -- per-profile S3 secret key (cloud profiles only)
- `cloud_home_access_key` -- access key for the cloud home (S3 providers)
- `cloud_home_secret_key` -- secret key for the cloud home (S3 providers)
- `cloud_home_oauth_token` -- OAuth refresh token for the cloud home (consumer cloud providers)

### Cloud home layout

The cloud home is one location per library (a Google Drive folder, S3 bucket, Dropbox folder, etc.). It contains everything needed for multi-device sync, new-device bootstrap, and release files.

```
cloud-home/
  snapshot.db.enc                    # full DB for bootstrapping new devices
  changes/{device_id}/{seq}.enc      # binary changesets with metadata envelope
  heads/{device_id}.json.enc         # per-device sequence numbers for cheap polling
  images/ab/cd/{id}                  # all library images (encrypted)
  storage/ab/cd/{file_id}            # release files (default storage location)
```

Images in the cloud home are encrypted. Release files use chunked encryption.

Release files live under `storage/` in an opaque hash-based layout. `prefix` = first 2 chars of the file ID, `subprefix` = next 2 chars. No filenames, no extensions -- original filenames and content types live in the DB. The path is deterministic from the file ID alone: `storage/{prefix}/{subprefix}/{file_id}`. Same layout in both the library home and cloud home.

## Two classes of files

bae manages two fundamentally different kinds of files:

### Release files

All files that came with a release -- audio tracks, cover scans, booklet pages, CUE sheets, logs. These are the user's data. bae stores them exactly as imported so they can be ejected intact or seeded as torrents.

Where they live:
- **Library home**: bae copies files to `storage/ab/cd/{file_id}`. The originals are untouched.
- **Cloud home**: bae encrypts and uploads to `cloud-home/storage/ab/cd/{file_id}`.
- **Unmanaged**: files stay where the user has them on disk. bae indexes but doesn't touch.

The `release_files` table tracks each file with `source_path` pointing to the actual location (local path or cloud home key).

### Metadata images

Images that bae creates and manages. These live in the library home directory, not with the release files. They are synced to the cloud home as part of changeset sync (pushed when the `library_images` table changes).

Two kinds:
- **Release covers** -- display art for album grids, detail views, playback. One per release. May originate from a file in the release, or fetched from MusicBrainz/Discogs. bae makes its own copy.
- **Artist images** -- fetched from external sources.

All library images are stored under `images/` using the same hash-based prefixing as release files: `images/{prefix}/{subprefix}/{id}`. No extension on disk -- content type is in the DB.

## DB tables

### `release_files` -- release files (audio + images + metadata)

Tracks every file in a release. These travel with the release.

```
release_files
  id                TEXT PK
  release_id        TEXT FK -> releases
  original_filename TEXT NOT NULL    -- "01 - Track.flac", "cover.jpg", "disc.cue"
  file_size         INTEGER NOT NULL
  content_type      TEXT NOT NULL    -- "audio/flac", "image/jpeg", "text/plain"
  source_path       TEXT             -- actual location (local path or s3:// key)
  encryption_nonce  BLOB
  created_at        TEXT NOT NULL
```

Content types are stored as MIME strings and mapped to the `ContentType` enum in Rust for type-safe comparisons (`ContentType::Flac`, `ContentType::Jpeg`, etc.) with helpers like `is_audio()`, `is_image()`, `display_name()`, and `from_extension()`.

### `audio_formats` -- playback metadata (1:1 with tracks)

Stores everything needed to play a track: codec info, FLAC headers for CUE/FLAC tracks (where the decoder needs headers prepended since playback starts mid-file), byte offsets for track boundaries, and a dense seektable for frame-accurate seeking (~93ms precision).

For one-file-per-track FLAC: `start_byte_offset`/`end_byte_offset` are NULL, `needs_headers` is false. For CUE/FLAC: both offsets point into the shared FLAC file, `needs_headers` is true.

`file_id` links to the `release_files` row containing this track's audio data.

```
audio_formats
  id                  TEXT PK
  track_id            TEXT FK -> tracks (UNIQUE)
  content_type        TEXT NOT NULL    -- "audio/flac"
  flac_headers        BLOB            -- for CUE/FLAC: headers to prepend
  needs_headers       BOOLEAN NOT NULL
  start_byte_offset   INTEGER         -- CUE/FLAC: track start in shared file
  end_byte_offset     INTEGER         -- CUE/FLAC: track end in shared file
  pregap_ms           INTEGER         -- CUE/FLAC: INDEX 00 gap duration
  frame_offset_samples INTEGER        -- samples to skip after frame alignment
  exact_sample_count  INTEGER         -- for gapless playback trimming
  sample_rate         INTEGER NOT NULL
  bits_per_sample     INTEGER NOT NULL
  seektable_json      TEXT NOT NULL    -- dense frame-level seektable
  audio_data_start    INTEGER NOT NULL -- byte offset where audio data begins
  file_id             TEXT FK -> release_files
  created_at          TEXT NOT NULL
```

### `library_images` -- bae-managed metadata images

All images that bae creates and manages (as opposed to release files which are the user's data). One table, discriminated by `type`.

```
library_images
  id            TEXT PK          -- release_id for covers, artist_id for artists
  type          TEXT NOT NULL    -- "cover", "artist"
  content_type  TEXT NOT NULL    -- "image/jpeg", "image/png"
  file_size     INTEGER NOT NULL
  width         INTEGER
  height        INTEGER
  source        TEXT NOT NULL    -- "local", "musicbrainz", "discogs"
  source_url    TEXT             -- see below
  created_at    TEXT NOT NULL
```

File location is deterministic from the id: `images/{prefix}/{subprefix}/{id}` (same hash-based layout as `storage/`). No extension on disk -- content type is in the DB. No `source_path` needed -- the path is derived.

`source_url` values:
- MusicBrainz: CAA numeric image ID (e.g., `"12345678901"`)
- Discogs: image URL (e.g., `"https://i.discogs.com/..."`)
- Local (selected from release files): `"release://{relative_path}"` (e.g., `"release://Artwork/front.jpg"`)

## Image server

Desktop and bae-server run a localhost HTTP image server (axum, OS-assigned port, HMAC-signed URLs). Two endpoints:

- `/image/{id}` -- serves library images (covers, artist photos). Looks up `library_images WHERE id = ?`, reads `images/.../{id}`, serves with correct Content-Type.
- `/file/{file_id}` -- serves release files. Looks up `release_files WHERE id = ?`, reads from `source_path`, decrypts if needed, serves with correct Content-Type.

## Sync

Desktop is the single writer. After mutations, it pushes changesets to the cloud home (if configured). Other devices pull changesets and apply them with a conflict handler. Images are synced alongside the changesets that reference them. See `notes/02-sync-and-storage.md` for details.

## Cover lifecycle

**Import with local cover**: user selects a cover from among the release's image files -> bae copies the bytes to `images/.../{release_id}`, inserts `library_images` row with `type = "cover"`, `source = "local"`, `source_url = "release://cover.jpg"`. The original image stays untouched in the release files. The cover is a copy -- bae can crop, resize, or optimize it without affecting the original. This is the same flow as a remotely fetched cover, just with a local source.

**Import with remote cover**: user selects MB/Discogs cover -> bae downloads, writes to `images/.../{release_id}`, inserts `library_images` row with source_url pointing back to the external source.

**Cover picker -- change to existing release image**: user picks a different image from the release's files -> bae reads from `source_path`, writes to `images/.../{release_id}`, upserts `library_images` row.

**Cover picker -- download new cover**: user picks from MB/Discogs -> download, write to `images/.../{release_id}`, upsert `library_images` row.

**Artist image fetch**: during import, fetch artist photo from Discogs/MB -> write to `images/.../{artist_id}`, upsert `library_images` row with `type = "artist"`.
