# Data Model: Releases, Files, and Images

## Directory layout

### Library home (`~/.bae/`)

The library home is the first local storage profile, created on first launch. It's where desktop runs day-to-day. It has a `storage_profiles` row like any other profile.

```
~/.bae/
  active-library               # pointer file — path to the active library home
  config.yaml                  # device-specific settings (not replicated)
  manifest.json                # library identity (replicated to all profiles)
  library.db                   # SQLite — all metadata
  covers/<release_id>          # cover art (no extension, content type in DB)
  artists/<artist_id>          # artist images (no extension)
  ab/cd/<file_id>              # audio files (no extension, content type in DB)
  pending_deletions.json       # deferred file deletion manifest
```

The default library lives at `~/.bae/`. `active-library` is a pointer file — absent or self-referencing means "use `~/.bae/`". Multiple libraries are supported but each owns its own directory; a second library would live at a completely separate path.

**`manifest.json`** — identifies library and profile. Replicated to every profile, always unencrypted. Contains `library_id`, `library_name`, `encryption_key_fingerprint`, `profile_id`, `profile_name`, `replicated_at`. Used by readers to identify both what library and which profile they're looking at, and validate the encryption key before downloading anything large.

**`config.yaml`** — device-specific settings. Not replicated, only at the library home. Contains keyring hint flags (`discogs_key_stored`, `encryption_key_stored`), torrent settings, subsonic settings. Non-secret only — credentials go in the keyring.

### Keyring (OS keyring, namespaced by library_id)

Managed by `KeyService`. On macOS, uses the protected data store with iCloud Keychain sync.

- `encryption_master_key` — one per library, used for all file and metadata encryption
- `discogs_api_key`

### Storage profile layout

Each profile owns its directory or bucket exclusively — no sharing between libraries.

Every profile stores both audio files and a metadata replica. Files are keyed by DB file ID — no filenames, no extensions:

**Local profile:**
```
{location_path}/
  manifest.json
  library.db
  covers/{release_id}
  artists/{artist_id}
  ab/cd/{file_id}
```

**Cloud profile:**
```
s3://{bucket}/
  manifest.json
  library.db.enc
  covers/{release_id}
  artists/{artist_id}
  ab/cd/{file_id}
```

Every profile is self-contained — it has all the data needed to restore a full library. See `storage-profiles.md` for details.

## Two classes of files

bae manages two fundamentally different kinds of files:

### Release files

All files that came with a release — audio tracks, cover scans, booklet pages, CUE sheets, logs. These are the user's data. bae stores them exactly as imported so they can be ejected intact or seeded as torrents.

Where they live depends on the storage profile:
- **Unmanaged**: files stay where the user has them on disk. bae indexes but doesn't touch.
- **Local storage profile**: bae copies files to the profile's local directory.
- **Cloud storage profile**: bae encrypts and uploads to S3.

All release files for a given release are stored together. The `files` table tracks each one with `source_path` pointing to the actual location (local path or S3 key).

### Metadata images

Images that bae creates and manages. These live in the library home directory, not with the release files.

Two kinds:
- **Release covers** — display art for album grids, detail views, playback. One per release. May originate from a file in the release, or fetched from MusicBrainz/Discogs. bae makes its own copy. Stored at `covers/{release_id}` (no extension — content type is in the DB).
- **Artist images** — fetched from external sources. Stored at `artists/{artist_id}` (no extension).

## DB tables

### `files` — release files (audio + images + metadata)

Tracks every file in a release. These travel with the release.

```
files
  id                TEXT PK
  release_id        TEXT FK → releases
  original_filename TEXT NOT NULL    -- "01 - Track.flac", "cover.jpg", "disc.cue"
  file_size         INTEGER NOT NULL
  content_type      TEXT NOT NULL    -- "audio/flac", "image/jpeg", "text/plain"
  source_path       TEXT             -- actual location (local path or s3:// key)
  encryption_nonce  BLOB
  created_at        TEXT NOT NULL
```

Content types are stored as MIME strings and mapped to the `ContentType` enum in Rust for type-safe comparisons (`ContentType::Flac`, `ContentType::Jpeg`, etc.) with helpers like `is_audio()`, `is_image()`, `display_name()`, and `from_extension()`.

### `audio_formats` — playback metadata (1:1 with tracks)

Stores everything needed to play a track: codec info, FLAC headers for CUE/FLAC tracks (where the decoder needs headers prepended since playback starts mid-file), byte offsets for track boundaries, and a dense seektable for frame-accurate seeking (~93ms precision).

For one-file-per-track FLAC: `start_byte_offset`/`end_byte_offset` are NULL, `needs_headers` is false. For CUE/FLAC: both offsets point into the shared FLAC file, `needs_headers` is true.

`file_id` links to the `files` row containing this track's audio data.

```
audio_formats
  id                  TEXT PK
  track_id            TEXT FK → tracks (UNIQUE)
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
  file_id             TEXT FK → files
  created_at          TEXT NOT NULL
```

### `library_images` — bae-managed metadata images

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

File location is deterministic from type + id:
- `type = "cover"` → `covers/{id}`
- `type = "artist"` → `artists/{id}`

No extension on disk — content type is in the DB. No `source_path` needed — the path is derived.

`source_url` values:
- MusicBrainz: CAA numeric image ID (e.g., `"12345678901"`)
- Discogs: image URL (e.g., `"https://i.discogs.com/..."`)
- Local (selected from release files): `"release://{relative_path}"` (e.g., `"release://Artwork/front.jpg"`)

## Protocol serving

- `bae://cover/{release_id}` → query `library_images WHERE id = ? AND type = 'cover'` → read `covers/{release_id}` → serve with correct Content-Type
- `bae://image/{file_id}` → query `files WHERE id = ?` → read from `source_path` → decrypt if needed → serve with correct Content-Type
- `bae://artist-image/{artist_id}` → query `library_images WHERE id = ? AND type = 'artist'` → read `artists/{artist_id}` → serve with correct Content-Type

## Metadata replication

After mutations, desktop replicates metadata (DB, covers, artists) to all other profiles. Each profile's replica lives alongside the audio files at the profile root. See `storage-profiles.md` and `library-and-cloud.md` for the full sync flow.

## Cover lifecycle

**Import with local cover**: user's release has cover.jpg → bae copies the bytes to `covers/{release_id}`, inserts `library_images` row with `type = "cover"`, `source = "local"`, `source_url = "release://cover.jpg"`. The original image stays untouched in the release files. The cover is a copy — bae can crop, resize, or optimize it without affecting the original. This is the same flow as a remotely fetched cover, just with a local source.

**Import with remote cover**: user selects MB/Discogs cover → bae downloads, writes to `covers/{release_id}`, inserts `library_images` row with source_url pointing back to the external source.

**Cover picker — change to existing release image**: user picks a different image from the release's files → bae reads from `source_path`, writes to `covers/{release_id}`, upserts `library_images` row.

**Cover picker — download new cover**: user picks from MB/Discogs → download, write to `covers/{release_id}`, upsert `library_images` row.

**Artist image fetch**: during import, fetch artist photo from Discogs/MB → write to `artists/{artist_id}`, upsert `library_images` row with `type = "artist"`.

In all cases: one file write, one DB write. No dual systems.
