# Image Model Cleanup

Eliminate the convention-based cover cache, the `images` table, and the filename-based joins. Replace with a clean model where metadata images have deterministic paths and correct content types. See `notes/data-model.md` for the target model.

## Current problems

1. Cover cache (`covers/{release_id}.{ext}`) served by guessing extensions — no correct Content-Type
2. `images` table joins to `files` by filename convention — no FK, breaks silently
3. Remote covers during import don't create `DbImage` records at all — just a cache file
4. Cover picker creates DbFile + DbImage + cache file (triple write)
5. Artist images stored as `artists/{id}.{ext}` with `image_path` column — same extension guessing
6. `download_cover_art_bytes` returns extension string, not content type
7. `create_image_records`, `fetch_image_bytes`, `update_cover_cache` are spaghetti glue between the parallel systems
8. Artist images not included in cloud sync at all
9. No single utility for content type detection — ad-hoc `mime_type_for_extension` in protocol_handler, extension guessing scattered elsewhere

## Sub-plans

### 1. Schema + types

Change `001_initial.sql` and Rust types.

**SQL changes:**
- Drop `images` table entirely
- Add `library_images` table:
  ```sql
  CREATE TABLE library_images (
      id           TEXT PRIMARY KEY,  -- release_id for covers, artist_id for artists
      type         TEXT NOT NULL,     -- "cover", "artist"
      content_type TEXT NOT NULL,     -- "image/jpeg", "image/png"
      file_size    INTEGER NOT NULL,
      width        INTEGER,
      height       INTEGER,
      source       TEXT NOT NULL,     -- "local", "musicbrainz", "discogs"
      source_url   TEXT,
      created_at   TEXT NOT NULL
  );
  ```
- On `artists` table: drop `image_path` column

Note: `files.format` stays as-is for now. It stores audio format strings ("flac", "mp3") which are used for playback format detection, not for HTTP serving. Renaming it to `content_type` with MIME types would touch playback, subsonic streaming, import, UI display, etc. — that's a separate effort.

**Rust type changes:**
- New `DbLibraryImage` struct in `db/models.rs` + `LibraryImageType` enum (`Cover`, `Artist`)
- DB operations in `client.rs`: `upsert_library_image`, `get_library_image`, `delete_library_image`
- Delete `DbImage`, `ImageSource` from `db/models.rs`
- Delete from `db/client.rs`: `insert_image`, `get_images_for_release`, `get_cover_image_for_release`, `set_cover_image`, `get_image_by_id`, `get_file_by_release_and_filename`
- Update `DbArtist`: drop `image_path` field (presence check becomes `library_images` query or a simple `EXISTS` / join)
- Delete from `library/manager.rs`: `fetch_image_bytes`, `update_cover_cache`, and all the manager wrapper methods for the deleted DB operations (`add_image`, `get_images_for_release`, `get_cover_image_for_release`, `set_cover_image`, `get_image_by_id`, `get_file_by_release_and_filename`)

**LibraryDir additions** (`library_dir.rs`):
- `cover_path(&self, release_id: &str) -> PathBuf` — `covers/{release_id}` (no extension)
- `artist_image_path(&self, artist_id: &str) -> PathBuf` — `artists/{artist_id}` (no extension)

**Content type utility:**
- Extract `mime_type_for_extension` out of `protocol_handler.rs` into a shared location (e.g., a small helper in bae-core or a common module). This is still needed for `handle_local_file` (serving arbitrary local files) and for import (detecting content type from file extension when no HTTP response is available). One function, one location, used everywhere.

**Verify:** `cargo check -p bae-core` (will have downstream errors, that's fine)

### 2. Cover art download returns content type

`download_cover_art_bytes` currently returns `(Vec<u8>, String)` where the String is a file extension. Change to return content type instead.

**Files:**
- `bae-core/src/import/cover_art.rs` — `download_cover_art_bytes` reads `Content-Type` header from HTTP response instead of guessing extension from URL
- All callers updated: `import/handle.rs`, `app_service.rs` (change_cover_async)
- `fetch_and_save_artist_image` in `import/artist_image.rs` — same pattern, read `Content-Type` from response
- Delete `image_extension_from_url` from `import/mod.rs` (that's where it actually lives, not cover_art.rs) along with its tests (lines 53-97)

### 3. Import write paths

Update import to write covers and artist images via the new model.

**`import/handle.rs` — remote cover during phase 0:**
- Download bytes + get content type from response
- Write to `covers/{release_id}` (no extension)
- Upsert `library_images` row with `type = "cover"`, content_type, source, source_url
- Delete the `remote_cover_set` boolean plumbing — cover is now a proper DB record

**`import/service.rs` — `create_image_records` → replace entirely:**
- Delete `create_image_records` function and `image_cover_priority` helper
- Replace with focused cover logic: find the cover image among release files (reuse the priority logic inline or as a small helper), copy bytes to `covers/{release_id}`, upsert `library_images` row with `source = "local"`, `source_url = "release://{path}"`
- Non-cover images are just `files` rows — already created during import, no separate tracking

**`import/artist_image.rs` — `fetch_and_save_artist_image`:**
- Write to `artists/{artist_id}` (no extension)
- Read content type from HTTP response
- Upsert `library_images` row with `type = "artist"` instead of updating `artists.image_path`
- Kill the extension-trying existence check at the top of the function (check `library_images` instead)

**Verify:** `cargo check -p bae-core`

### 4. Cover picker write path

Simplify `change_cover_async` in `bae-desktop/src/ui/app_service.rs`.

**`CoverChange::ExistingImage` variant** (select from release files):
- `image_id` now refers to a `DbFile.id` (not the old `DbImage.id`) — update the `CoverChange` enum in bae-ui accordingly
- Query `files` by id to get source_path + format
- Read bytes from source_path (handle cloud download + decrypt)
- Detect content type from file bytes or use the shared mime utility on the filename
- Write to `covers/{release_id}`
- Upsert `library_images` row
- No DbFile or DbImage creation

**`CoverChange::RemoteCover` variant** (download from MB/Discogs):
- Download bytes + content type
- Write to `covers/{release_id}`
- Upsert `library_images` row with source + source_url
- No staging directory, no DbFile, no DbImage

**`fetch_remote_covers_async`** (cover picker dedup logic):
- Currently queries `get_images_for_release` to check which sources already have images
- Replace with: query `library_images WHERE id = ? AND type = 'cover'` to get the current cover's source. Dedup logic simplifies — there's at most one cover per release now.

Delete `covers/downloads/` directory handling entirely.

**Verify:** `cargo check -p bae-desktop`

### 5. Read paths (protocol handler + media controls)

Rewrite `bae-desktop/src/ui/protocol_handler.rs` and update `media_controls.rs`.

**`handle_cover(release_id)`:**
- Query `library_images WHERE id = ? AND type = 'cover'` for content_type
- Read `covers/{release_id}` from disk
- Serve with correct Content-Type and Content-Length headers
- Delete the extension guessing loop

**`handle_image(file_id)` — for gallery display of release file images:**
- Query `files WHERE id = ?` for content_type (well, `format` for now), source_path
- Read from source_path (direct disk read or cloud download + decrypt)
- Serve with correct Content-Type
- No filename join, no images table

**`handle_artist_image(artist_id)`:**
- Query `library_images WHERE id = ? AND type = 'artist'` for content_type
- Read `artists/{artist_id}` from disk
- Serve with correct Content-Type
- Delete the extension guessing loop

**`handle_local_file`:** Keep as-is, uses the shared mime utility for extension-based content type (correct for arbitrary local files).

**`media_controls.rs` — `resolve_cover_file_url`:**
- Currently does the same extension-guessing loop as the protocol handler
- Update to read `covers/{release_id}` directly via `LibraryDir::cover_path()` — no extension needed
- This produces a `file://` URL for the OS media controls, not a `bae://` URL

### 6. Gallery + display types

**`CoverChange` enum in bae-ui (`display_types.rs`):**
- `ExistingImage { image_id }` — now refers to a `DbFile.id`, not `DbImage.id`. Rename to `file_id` for clarity.

**`bae-desktop/src/ui/display_types.rs`:**
- Delete `image_from_db_ref` (no more DbImage)
- Gallery images come from `files WHERE release_id = ? AND content_type LIKE 'image/%'` (or `format` for now — filter by image formats)
- Gallery image URL becomes `bae://image/{file_id}` (using file's ID directly)
- `artist_from_db_ref`: query `library_images` for artist image existence instead of checking `image_path`

**bae-ui display types (`Image` struct):**
- Simplify to match new data source: file id, original_filename for display
- Remove `is_cover` and `source` fields (cover status lives in `library_images`, not on gallery items)

**Album detail gallery component:**
- Load images from files query instead of images query
- Update `GalleryItem` construction

**bae-web:** Verify `AlbumDetailState` construction in `bae-web/src/api.rs` still compiles after `Image` struct changes.

### 7. Cloud sync for library images

Currently `cloud_sync.rs` only syncs covers. Add artist image sync using the same pattern.

**`bae-core/src/cloud_sync.rs`:**
- `upload_covers` / `download_covers`: filenames on S3 will be extensionless now, but the sync logic (iterate directory, encrypt each file, upload) doesn't change
- Add `upload_artists` / `download_artists` — same pattern as covers, syncs the `artists/` directory
- Or generalize: a single `upload_library_images` / `download_library_images` that takes a directory path, since the logic is identical for both `covers/` and `artists/`

**`bae-desktop/src/ui/app_service.rs` — `cloud_sync_upload`:**
- Call the new artist sync after cover sync

**`bae-desktop/src/ui/components/welcome.rs` — restore flow:**
- Download artists in addition to covers

**S3 layout becomes:**
```
s3://bucket/bae/{library_id}/
  library.db.enc
  meta.json
  covers/{release_id}       -- extensionless, encrypted
  artists/{artist_id}       -- extensionless, encrypted
```

### 8. bae-server (subsonic)

**`bae-core/src/subsonic.rs`:**
- `getCoverArt` endpoint: query `library_images` for cover content_type, read from `covers/{id}`
- Replace extension guessing with DB lookup
- Serve with correct Content-Type header

### 9. Cleanup

Delete all dead code. This should mostly be done incrementally in earlier sub-plans, but verify nothing is left:

- `DbImage` struct and `ImageSource` enum gone from `models.rs`
- All old `images` DB operations gone from `client.rs`
- `update_cover_cache()` gone from `library/manager.rs`
- `fetch_image_bytes()` gone from `library/manager.rs`
- `create_image_records()` and `image_cover_priority()` gone from `import/service.rs`
- `get_file_by_release_and_filename` gone from `client.rs`
- Extension guessing loops gone from `protocol_handler.rs`, `media_controls.rs`
- `covers/downloads/` references gone
- `image_path` field gone from `DbArtist`
- `image_extension_from_url` gone from `import/mod.rs`
- Integration tests updated: `test_storage.rs` references to `DbImage` and `get_images_for_release`
- `cargo clippy -p bae-core -p bae-desktop -p bae-mocks -p bae-server` clean

## Execution order

Sub-plans 1-3 form the core (schema + types + write paths). After that, 4-6 can proceed in any order. 7-8 are independent. 9 is verification.

Likely PR grouping:
- **PR A**: Sub-plans 1 + 2 + 3 + 9 (schema, types, import paths, delete dead code) — the foundation
- **PR B**: Sub-plans 4 + 5 + 6 (cover picker, protocol handler, gallery, media controls) — display
- **PR C**: Sub-plans 7 + 8 (cloud sync, subsonic)

Or one big PR.
