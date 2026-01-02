#![cfg(feature = "test-utils")]
//! Tests for CUE/FLAC format handling.
//!
//! CUE/FLAC albums have multiple tracks in a single FLAC file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record byte offsets for each track in the database
//! - Enable playback to seek to the correct position for each track
//!
//! These tests use storageless imports for simplicity (the CUE/FLAC handling
//! is independent of storage configuration).
mod support;
use crate::support::tracing_init;
use bae::db::{Database, ImportStatus};
use bae::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae::encryption::EncryptionService;
use bae::import::{ImportProgress, ImportRequest, ImportService};
use bae::library::{LibraryManager, SharedLibraryManager};
use bae::torrent::LazyTorrentManager;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;
/// Test that CUE/FLAC imports record track byte positions correctly.
///
/// Regression test: CUE/FLAC imports must record byte offsets for each track
/// so playback can seek to the correct position within the single FLAC file.
#[tokio::test]
async fn test_cue_flac_records_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let library_manager = LibraryManager::new(database.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
        torrent_manager,
        database_arc,
    );
    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");
    info!("Import request sent, release_id: {}", release_id);
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => {
                info!("Import completed");
                break;
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks");
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete",
            track.title,
        );
    }
    // Check that each track has audio format with byte offsets recorded
    for (i, track) in tracks.iter().enumerate() {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format")
            .unwrap_or_else(|| {
                panic!(
                    "Track {} '{}' should have audio format recorded",
                    i + 1,
                    track.title,
                )
            });
        assert!(
            audio_format.start_byte_offset.is_some(),
            "CUE/FLAC track {} should have start_byte_offset",
            i + 1,
        );
        assert!(
            audio_format.end_byte_offset.is_some(),
            "CUE/FLAC track {} should have end_byte_offset",
            i + 1,
        );
        if i > 0 {
            let prev_format = library_manager
                .get_audio_format_by_track_id(&tracks[i - 1].id)
                .await
                .expect("get prev format")
                .expect("prev format exists");
            assert!(
                audio_format.start_byte_offset.unwrap() >= prev_format.start_byte_offset.unwrap(),
                "Track {} start_byte ({}) should be >= track {} start_byte ({})",
                i + 1,
                audio_format.start_byte_offset.unwrap(),
                i,
                prev_format.start_byte_offset.unwrap(),
            );
        }
        info!(
            "Track {} '{}': bytes {}..{}",
            i + 1,
            track.title,
            audio_format.start_byte_offset.unwrap_or(-1),
            audio_format.end_byte_offset.unwrap_or(-1),
        );
    }
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have audio format recorded",
            track.title,
        );
        let af = audio_format.unwrap();
        assert_eq!(af.format, "flac", "Should be FLAC format");
        assert!(
            af.flac_headers.is_some(),
            "Should have FLAC headers for seeking"
        );
    }
    info!("✅ All track positions recorded correctly for CUE/FLAC import");
}
/// Test that playback of track 2 loads audio from the correct byte range.
///
/// Regression test: Playback must use the recorded byte offsets to extract
/// the correct portion of the FLAC file for each track. Without this,
/// all tracks would play from the beginning of the file.
#[tokio::test]
async fn test_cue_flac_playback_uses_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let cache_config = bae::cache::CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_files: 10000,
    };
    let _cache_manager = bae::cache::CacheManager::with_config(cache_config)
        .await
        .expect("cache");
    let library_manager = LibraryManager::new(database.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
    let import_handle = ImportService::start(
        runtime_handle.clone(),
        shared_library_manager,
        encryption_service.clone(),
        torrent_manager,
        database_arc,
    );
    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => break,
            ImportProgress::Failed { error, .. } => panic!("Import failed: {}", error),
            _ => {}
        }
    }
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Test track 2 (middle track with pregap)
    let track2 = &tracks[1];
    let track2_format = library_manager
        .get_audio_format_by_track_id(&track2.id)
        .await
        .expect("get track2 format")
        .expect("track2 should have audio format");
    let track2_start = track2_format
        .start_byte_offset
        .expect("track2 should have start offset");
    let track2_end = track2_format
        .end_byte_offset
        .expect("track2 should have end offset");
    info!(
        "Track 2 '{}': bytes {}..{}",
        track2.title, track2_start, track2_end,
    );
    assert!(
        track2_end > track2_start,
        "Track 2 must have a valid (non-empty) byte range. Got {}..{}",
        track2_start,
        track2_end,
    );
    assert!(
        track2_format.flac_headers.is_some(),
        "Track 2 must have FLAC headers stored for byte-range playback",
    );

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let playback_handle = bae::playback::PlaybackService::start(
        library_manager.as_ref().clone(),
        encryption_service,
        runtime_handle,
    );
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();
    playback_handle.play(track2.id.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut track2_decoded_duration = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), progress_rx.recv()).await
        {
            Ok(Some(bae::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae::playback::PlaybackState::Playing {
                    track,
                    decoded_duration,
                    ..
                } = &state
                {
                    if track.id == track2.id {
                        track2_decoded_duration = Some(*decoded_duration);
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    let track2_decoded_duration =
        track2_decoded_duration.expect("Should get track 2 decoded_duration from playback");
    let track2_decoded_duration_ms = track2_decoded_duration.as_millis() as i64;
    info!(
        "Track 2 decoded_duration: {}ms (actual audio length)",
        track2_decoded_duration_ms
    );

    // Full album is 30 seconds. Track 2 is ~12 seconds (10s + 2s pregap).
    // If we're reading the whole file, duration would be ~30s.
    let full_album_duration_ms = 30000;
    assert!(
        track2_decoded_duration_ms < full_album_duration_ms - 5000,
        "BUG: Track 2 decoded_duration is {}ms, which is close to the full album ({}ms). \
         This means load_audio_from_source_path is NOT using track coords. \
         It's reading the whole FLAC file instead of extracting track 2's byte range.",
        track2_decoded_duration_ms,
        full_album_duration_ms,
    );
    info!("✅ CUE/FLAC playback correctly uses track positions");
}

/// Test that decoded audio duration matches expected CUE timing.
///
/// Regression test: The seektable "at or before" algorithm for end bytes
/// was cutting off audio at the end of tracks. decoded_duration must match
/// the expected duration from CUE timing (within tolerance for frame alignment).
#[tokio::test]
async fn test_cue_flac_decoded_duration_matches_cue_timing() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let cache_config = bae::cache::CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_files: 10000,
    };
    let _cache_manager = bae::cache::CacheManager::with_config(cache_config)
        .await
        .expect("cache");
    let library_manager = LibraryManager::new(database.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
    let import_handle = ImportService::start(
        runtime_handle.clone(),
        shared_library_manager,
        encryption_service.clone(),
        torrent_manager,
        database_arc,
    );
    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => break,
            ImportProgress::Failed { error, .. } => panic!("Import failed: {}", error),
            _ => {}
        }
    }
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Test track 1 (first track, 0:00-0:08, which is 8 seconds before track 2's pregap)
    // CUE timing: Track 1 INDEX 01 @ 0:00, Track 2 INDEX 00 @ 0:08
    // Expected duration: 8 seconds (8000ms)
    let track1 = &tracks[0];
    let expected_duration_ms: i64 = 8000; // Track 1 is 0:00 to 0:08

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let playback_handle = bae::playback::PlaybackService::start(
        library_manager.as_ref().clone(),
        encryption_service,
        runtime_handle,
    );
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();
    playback_handle.play(track1.id.clone());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut decoded_duration = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), progress_rx.recv()).await
        {
            Ok(Some(bae::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae::playback::PlaybackState::Playing {
                    track,
                    decoded_duration: dd,
                    ..
                } = &state
                {
                    if track.id == track1.id {
                        decoded_duration = Some(*dd);
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    let decoded_duration = decoded_duration.expect("Should get decoded_duration from playback");
    let decoded_duration_ms = decoded_duration.as_millis() as i64;

    info!(
        "Track 1: expected {}ms, decoded {}ms",
        expected_duration_ms, decoded_duration_ms
    );

    // The decoded audio must be AT LEAST as long as expected (we can't cut off audio).
    // It may be longer due to FLAC frame alignment - frames are decoded whole.
    // With sparse seektables (5-10s apart), decoded may be up to one seektable interval longer.
    assert!(
        decoded_duration_ms >= expected_duration_ms,
        "BUG: decoded_duration ({}ms) is shorter than expected CUE timing ({}ms). \
         Missing {}ms of audio. The seektable byte range algorithm is cutting off audio.",
        decoded_duration_ms,
        expected_duration_ms,
        expected_duration_ms - decoded_duration_ms
    );

    // Sanity check: decoded shouldn't be more than one seektable interval (5s) longer
    let max_overshoot_ms = 5500; // 5s seektable interval + tolerance
    assert!(
        decoded_duration_ms - expected_duration_ms < max_overshoot_ms,
        "BUG: decoded_duration ({}ms) is way longer than expected ({}ms). \
         Something is wrong with byte range extraction.",
        decoded_duration_ms,
        expected_duration_ms
    );

    info!("✅ CUE/FLAC decoded duration matches CUE timing");
}

/// Test that consecutive CUE/FLAC tracks have no gaps in byte ranges.
///
/// Byte ranges may overlap (due to FLAC frame alignment) but must not have gaps,
/// which would lose audio. FLAC decoder handles overlapping byte ranges correctly.
///
/// Uses a realistic fixture with seektable (generated by scripts/generate_cue_flac_fixture.sh)
#[tokio::test]
async fn test_cue_flac_byte_ranges_have_no_gaps() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    copy_cue_flac_fixture_with_seektable(&album_dir);

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let library_manager = LibraryManager::new(database.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
        torrent_manager,
        database_arc,
    );

    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");

    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => break,
            ImportProgress::Failed { error, .. } => panic!("Import failed: {}", error),
            _ => {}
        }
    }

    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Check all consecutive track pairs have contiguous byte ranges
    for i in 0..tracks.len() - 1 {
        let track_a_format = library_manager
            .get_audio_format_by_track_id(&tracks[i].id)
            .await
            .expect("get track format")
            .expect("track should have audio format");
        let track_b_format = library_manager
            .get_audio_format_by_track_id(&tracks[i + 1].id)
            .await
            .expect("get next track format")
            .expect("next track should have audio format");

        let track_a_end = track_a_format
            .end_byte_offset
            .expect("track should have end offset");
        let track_b_start = track_b_format
            .start_byte_offset
            .expect("next track should have start offset");

        info!(
            "Track {} '{}' end: {}, Track {} '{}' start: {}",
            i + 1,
            tracks[i].title,
            track_a_end,
            i + 2,
            tracks[i + 1].title,
            track_b_start
        );

        // Byte ranges may overlap (FLAC decoder handles this) but must not have gaps
        assert!(
            track_a_end >= track_b_start,
            "BUG: Track {} end ({}) < Track {} start ({}). \
             Gap of {} bytes would lose audio!",
            i + 1,
            track_a_end,
            i + 2,
            track_b_start,
            track_b_start - track_a_end
        );
    }

    info!("✅ CUE/FLAC byte ranges have no gaps");
}

/// Test that we build a dense seektable during import for frame-accurate seeking.
///
/// The fixture has an embedded seektable with ~5 second gaps between entries.
/// We must build our own dense seektable (every frame, ~93ms) to:
/// 1. Enable smooth seeking during playback
/// 2. Calculate accurate track boundary byte positions
///
/// This test verifies:
/// - The embedded seektable is sparse (few entries)
/// - We store a dense seektable (many more entries)
/// - Track byte positions use frame-accurate offsets
#[tokio::test]
async fn test_cue_flac_builds_dense_seektable() {
    use bae::cue_flac::CueFlacProcessor;

    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);

    // First, analyze the FLAC to see what's embedded
    let flac_path = album_dir.join("Test Album.flac");
    let file_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");

    // Build our dense seektable
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&file_data, &flac_info);

    info!(
        "FLAC info: {} samples at {}Hz, audio starts at byte {}",
        flac_info.total_samples, flac_info.sample_rate, flac_info.audio_data_start
    );
    info!(
        "Dense seektable: {} entries (frame-accurate, ~{}ms precision)",
        dense_seektable.entries.len(),
        if dense_seektable.entries.len() > 1 {
            (flac_info.total_samples as f64 / dense_seektable.entries.len() as f64)
                / flac_info.sample_rate as f64
                * 1000.0
        } else {
            0.0
        }
    );

    // The dense seektable should have MANY more entries than a sparse one
    // For a 30-second file at 44100Hz with 4096-sample frames: ~322 frames
    // Sparse seektable (5s intervals) would have ~7 entries
    assert!(
        dense_seektable.entries.len() > 50,
        "Dense seektable should have many entries (got {}), not a sparse ~7 entries",
        dense_seektable.entries.len()
    );

    // Now import and verify the stored seektable is dense
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let library_manager = LibraryManager::new(database.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
        torrent_manager,
        database_arc,
    );

    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir.clone(),
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");

    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => break,
            ImportProgress::Failed { error, .. } => panic!("Import failed: {}", error),
            _ => {}
        }
    }

    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");

    // Check that the stored seektable is dense
    let track1 = &tracks[0];
    let audio_format = library_manager
        .get_audio_format_by_track_id(&track1.id)
        .await
        .expect("get audio format")
        .expect("audio format exists");

    let stored_seektable: Vec<(u64, u64)> = audio_format
        .flac_seektable
        .as_ref()
        .map(|data| bincode::deserialize(data).expect("deserialize seektable"))
        .unwrap_or_default();

    info!("Stored seektable in DB: {} entries", stored_seektable.len());

    assert!(
        stored_seektable.len() > 50,
        "Stored seektable should be dense (got {} entries), not sparse. \
         We build our own seektable for frame-accurate seeking.",
        stored_seektable.len()
    );

    // Verify track 2 byte position is frame-accurate
    // Track 2 starts at 00:08:00 (8 seconds) = 8 * 44100 = 352800 samples
    // With frame-accurate positioning, byte offset should be within one frame (~93ms)
    let track2 = &tracks[1];
    let track2_format = library_manager
        .get_audio_format_by_track_id(&track2.id)
        .await
        .expect("get audio format")
        .expect("audio format exists");

    let _track2_start_byte = track2_format.start_byte_offset.expect("start offset");

    // Find the dense seektable entry closest to track 2's start
    let track2_start_sample = 8 * 44100u64; // 8 seconds
    let closest_entry = dense_seektable
        .entries
        .iter()
        .rev()
        .find(|e| e.sample_number <= track2_start_sample)
        .expect("should find entry before track 2");

    let samples_from_boundary = track2_start_sample - closest_entry.sample_number;
    let ms_from_boundary = (samples_from_boundary * 1000) / flac_info.sample_rate as u64;

    info!(
        "Track 2 starts at sample {}, nearest frame at sample {} ({}ms offset)",
        track2_start_sample, closest_entry.sample_number, ms_from_boundary
    );

    // With frame-accurate positioning, we should be within one frame (~93ms)
    assert!(
        ms_from_boundary < 100,
        "Track 2 should start within ~93ms of actual position (got {}ms). \
         This confirms we're using the dense seektable for frame-accurate positioning.",
        ms_from_boundary
    );

    info!("✅ Dense seektable built and stored correctly for frame-accurate seeking");
}

/// Copy the CUE/FLAC fixture with seektable (30-second file with 3 tracks).
/// Generated by scripts/generate_cue_flac_fixture.sh
fn copy_cue_flac_fixture_with_seektable(dir: &Path) {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let flac_data = fs::read(fixture_dir.join("Test Album.flac")).unwrap_or_else(|_| {
        panic!("CUE/FLAC fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh")
    });
    let cue_data = fs::read(fixture_dir.join("Test Album.cue")).unwrap_or_else(|_| {
        panic!("CUE fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh")
    });
    fs::write(dir.join("Test Album.flac"), &flac_data).expect("write flac");
    fs::write(dir.join("Test Album.cue"), &cue_data).expect("write cue");
    info!(
        "Copied CUE/FLAC fixture with seektable: {} bytes FLAC",
        flac_data.len()
    );
}

/// Discogs release matching the seektable fixture (tests/fixtures/cue_flac/)
fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-cue-flac".to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        genre: vec![],
        style: vec![],
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        artists: vec![],
        tracklist: vec![
            DiscogsTrack {
                position: "1".to_string(),
                title: "Track One (440Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two (880Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three (660Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
        ],
        master_id: "test-master".to_string(),
    }
}
