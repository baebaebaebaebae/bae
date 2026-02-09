# Data Model: Releases, Files, and Images

## Libraries and profiles

A **library** is the logical entity — a music collection. It has an identity (`library_id`), a name, and an encryption key. It exists across multiple physical locations.

A **profile** is a physical location where data lives. Each profile stores a full replica of the library metadata (DB, images, `manifest.json`) plus whatever release files have been placed on it. A library has one or more profiles.

One profile is the **library home** — where desktop runs, where the authoritative DB lives, where `config.yaml` lives. The rest are replicas that receive metadata via sync.

```
Library "My Music" (lib-111)
  ├── prof-aaa  (library home, ~/.bae/libraries/lib-111/)  ← desktop writes here
  ├── prof-bbb  (cloud, s3://my-music-bucket/)             ← replica
  └── prof-ccc  (local, /Volumes/ExternalSSD/)             ← replica
```

All profiles have the full metadata catalog (every release, track, artist — the complete DB, plus all library images). Release files are separate — each release's files live on one profile (or are unmanaged). Not every profile has every release's files. bae-server can point at any profile and serve the full catalog from it, but can only play releases whose files are on that profile or on cloud profiles it can access.

## Directory layout

### bae directory (`~/.bae/`)

Used by desktop — this is what opens when you launch the app. Contains all local libraries. `active-library` is the UUID of the currently active library — absent means use the first (or only) library.

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

bae-server doesn't use `~/.bae/` — it points directly at a profile (local directory or S3 bucket).

### Library home

The library home is a storage profile — same layout as any other profile. What makes it special: `config.yaml` lives here (device-specific settings), and desktop writes the authoritative DB here (other profiles get replicas).

```
~/.bae/libraries/{uuid}/
  config.yaml                  # device-specific settings (not replicated)
  manifest.json                # library + profile identity (replicated)
  library.db                   # SQLite — all metadata
  images/ab/cd/{id}            # library images (covers, artist photos — no extension, content type in DB)
  storage/ab/cd/{file_id}      # release files (no extension, content type in DB)
  pending_deletions.json       # deferred file deletion manifest
```

**`manifest.json`** — identifies both the library and the profile that owns this directory. Present on every profile (library home, external drives, S3 buckets). Plaintext on local profiles, encrypted on cloud profiles.

```json
{
  "library_id": "...",
  "library_name": "My Music",
  "encryption_key_fingerprint": "a1b2c3d4...",
  "profile_id": "...",
  "profile_name": "MacBook Local",
  "replicated_at": "2026-02-08T..."
}
```

`profile_id` matches a row in the `storage_profiles` DB table. This is how bae matches a directory to its DB record — paths change (different machine, different mount point), but `profile_id` is stable. During metadata sync, desktop writes a manifest to each target profile with that profile's own `profile_id`.

**`config.yaml`** — device-specific settings. Not replicated, only at the library home. Contains keyring hint flags (`discogs_key_stored`, `encryption_key_stored`), torrent settings, subsonic settings. Non-secret only — credentials go in the keyring.

### Keyring (OS keyring, namespaced by library_id)

Managed by `KeyService`. On macOS, uses the protected data store with iCloud Keychain sync.

- `encryption_master_key` — one per library, used for all file and metadata encryption
- `discogs_api_key`

### Storage profile layout

Each profile owns its directory or bucket exclusively — no sharing between libraries.

Every profile stores both release files and a metadata replica. Files are keyed by DB file ID — no filenames, no extensions:

**Local profile:**
```
{location_path}/
  manifest.json
  library.db
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

**Cloud profile:**
```
s3://{bucket}/
  manifest.json.enc
  library.db.enc
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

The library home uses the same layout — `{location_path}` is `~/.bae/libraries/{uuid}/`. Other local profiles can be anywhere (external drives, other directories). Every profile is self-contained — it has all the data needed to restore a full library. See `02-storage-profiles.md` for details.

## Two classes of files

bae manages two fundamentally different kinds of files:

### Release files

All files that came with a release — audio tracks, cover scans, booklet pages, CUE sheets, logs. These are the user's data. bae stores them exactly as imported so they can be ejected intact or seeded as torrents.

Where they live depends on the storage profile:
- **Unmanaged**: files stay where the user has them on disk. bae indexes but doesn't touch.
- **Local storage profile**: bae copies files to the profile's local directory.
- **Cloud storage profile**: bae encrypts and uploads to S3.

All release files for a given release are stored together. The `release_files` table tracks each one with `source_path` pointing to the actual location (local path or S3 key).

### Metadata images

Images that bae creates and manages. These live in the library home directory, not with the release files. They are replicated in full to every storage profile as part of metadata sync — even if that profile doesn't have the associated release's or artist's files.

Two kinds:
- **Release covers** — display art for album grids, detail views, playback. One per release. May originate from a file in the release, or fetched from MusicBrainz/Discogs. bae makes its own copy.
- **Artist images** — fetched from external sources.

All library images are stored under `images/` using the same hash-based prefixing as release files: `images/{prefix}/{subprefix}/{id}`. No extension on disk — content type is in the DB.

## DB tables

### `release_files` — release files (audio + images + metadata)

Tracks every file in a release. These travel with the release.

```
release_files
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

`file_id` links to the `release_files` row containing this track's audio data.

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
  file_id             TEXT FK → release_files
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

File location is deterministic from the id: `images/{prefix}/{subprefix}/{id}` (same hash-based layout as `storage/`). No extension on disk — content type is in the DB. No `source_path` needed — the path is derived.

`source_url` values:
- MusicBrainz: CAA numeric image ID (e.g., `"12345678901"`)
- Discogs: image URL (e.g., `"https://i.discogs.com/..."`)
- Local (selected from release files): `"release://{relative_path}"` (e.g., `"release://Artwork/front.jpg"`)

## Image server

Desktop and bae-server run a localhost HTTP image server (axum, OS-assigned port, HMAC-signed URLs). Two endpoints:

- `/image/{id}` — serves library images (covers, artist photos). Looks up `library_images WHERE id = ?`, reads `images/.../{id}`, serves with correct Content-Type.
- `/file/{file_id}` — serves release files. Looks up `release_files WHERE id = ?`, reads from `source_path`, decrypts if needed, serves with correct Content-Type.

## Metadata replication

After mutations, desktop replicates metadata (DB, images) to all other profiles. Each profile's replica lives alongside the audio files at the profile root. See `02-storage-profiles.md` and `01-library-and-cloud.md` for the full sync flow.

## Cover lifecycle

**Import with local cover**: user selects a cover from among the release's image files → bae copies the bytes to `images/.../{release_id}`, inserts `library_images` row with `type = "cover"`, `source = "local"`, `source_url = "release://cover.jpg"`. The original image stays untouched in the release files. The cover is a copy — bae can crop, resize, or optimize it without affecting the original. This is the same flow as a remotely fetched cover, just with a local source.

**Import with remote cover**: user selects MB/Discogs cover → bae downloads, writes to `images/.../{release_id}`, inserts `library_images` row with source_url pointing back to the external source.

**Cover picker — change to existing release image**: user picks a different image from the release's files → bae reads from `source_path`, writes to `images/.../{release_id}`, upserts `library_images` row.

**Cover picker — download new cover**: user picks from MB/Discogs → download, write to `images/.../{release_id}`, upsert `library_images` row.

**Artist image fetch**: during import, fetch artist photo from Discogs/MB → write to `images/.../{artist_id}`, upsert `library_images` row with `type = "artist"`.

