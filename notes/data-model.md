# Data Model: Releases, Files, and Images

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

Images that bae creates and manages. These live in the library directory (`~/.bae/libraries/<id>/`), not with the release files.

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

```
audio_formats
  id                TEXT PK
  track_id          TEXT FK → tracks (UNIQUE)
  content_type      TEXT NOT NULL    -- "audio/flac"
  flac_headers      BLOB
  needs_headers     BOOLEAN NOT NULL
  start_byte_offset INTEGER          -- for CUE/FLAC tracks
  end_byte_offset   INTEGER
  ...
  file_id           TEXT FK → files
  created_at        TEXT NOT NULL
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

## Cloud sync

The library directory syncs to cloud:
```
s3://bucket/bae/{library_id}/
  library.db.enc
  meta.json
  covers/           -- metadata images, encrypted individually
  artists/          -- metadata images, encrypted individually
```

Release files sync via their storage profile, not via library sync.

## Cover lifecycle

**Import with local cover**: user's release has cover.jpg → bae copies the bytes to `covers/{release_id}`, inserts `library_images` row with `type = "cover"`, `source = "local"`, `source_url = "release://cover.jpg"`. The original image stays untouched in the release files. The cover is a copy — bae can crop, resize, or optimize it without affecting the original. This is the same flow as a remotely fetched cover, just with a local source.

**Import with remote cover**: user selects MB/Discogs cover → bae downloads, writes to `covers/{release_id}`, inserts `library_images` row with source_url pointing back to the external source.

**Cover picker — change to existing release image**: user picks a different image from the release's files → bae reads from `source_path`, writes to `covers/{release_id}`, upserts `library_images` row.

**Cover picker — download new cover**: user picks from MB/Discogs → download, write to `covers/{release_id}`, upsert `library_images` row.

**Artist image fetch**: during import, fetch artist photo from Discogs/MB → write to `artists/{artist_id}`, upsert `library_images` row with `type = "artist"`.

In all cases: one file write, one DB write. No dual systems.
