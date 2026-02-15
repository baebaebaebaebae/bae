#![cfg(feature = "test-utils")]
//! Integration tests for managed and unmanaged import storage.
//!
//! Tests:
//! - Managed import: files stored to derived paths in library dir
//! - Unmanaged import: files stay in original location
//! - Unmanaged delete preserves files on disk
mod support;
use crate::support::test_encryption_service;
use bae_core::content_type::ContentType;
use bae_core::db::{Database, ImportStatus, LibraryImageType};
use bae_core::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae_core::encryption::EncryptionService;
use bae_core::import::{CoverSelection, ImportPhase, ImportProgress, ImportRequest, ImportService};
use bae_core::library::LibraryManager;
use std::path::Path;
use std::sync::Arc;
use std::{fs, path::PathBuf};
use tempfile::TempDir;
use tracing::info;

/// Initialize tracing for tests
fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Test managed import: files are stored into the library's storage directory.
#[tokio::test]
async fn test_managed_import() {
    tracing_init();
    run_managed_test().await;
}

/// Test unmanaged import: files stay in original location.
#[tokio::test]
async fn test_unmanaged_import() {
    tracing_init();

    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("album dir");
    fs::create_dir_all(&db_dir).expect("db dir");

    let file_data = generate_test_files(&album_dir);
    let original_files: Vec<_> = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ]
    .iter()
    .map(|f| album_dir.join(f))
    .collect();

    // Verify files exist before import
    for path in &original_files {
        assert!(path.exists(), "Test file should exist: {:?}", path);
    }

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager =
        bae_core::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);

    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());

    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
        database_arc,
        bae_core::keys::KeyService::new(true, "test".to_string()),
        bae_core::library_dir::LibraryDir::new(db_dir.clone()),
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
            managed: false,
            selected_cover: None,
        })
        .await
        .expect("send request");

    info!("Unmanaged import request sent, release_id: {}", release_id);

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

    // Verify tracks completed
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), file_data.len(), "Should have all tracks");

    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete",
            track.title,
        );
    }

    info!("All {} tracks are Complete", tracks.len());

    // Verify audio_format records exist with file_id linkage
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio_format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have an audio_format record",
            track.title
        );
        let af = audio_format.unwrap();
        assert!(
            af.file_id.is_some(),
            "Track '{}' audio_format should have a file_id",
            track.title
        );
    }

    info!("All tracks have audio_format records with file_id");

    // Verify release is unmanaged
    let release = database
        .get_release_by_id(&release_id)
        .await
        .expect("query")
        .expect("release should exist");
    assert!(
        !release.managed_locally,
        "Unmanaged import should NOT set managed_locally"
    );
    assert!(
        release.unmanaged_path.is_some(),
        "Unmanaged import should set unmanaged_path"
    );

    info!("Release is correctly unmanaged");

    // Verify original files still exist in place
    for path in &original_files {
        assert!(
            path.exists(),
            "Original file should still exist after unmanaged import: {:?}",
            path
        );
    }

    info!("Original files preserved in place");
}

/// Test that deleting an unmanaged release preserves the original files on disk.
///
/// When a release is unmanaged, the files live at their original location.
/// Deleting the release should only remove database records, NOT the actual files.
#[tokio::test]
async fn test_unmanaged_delete_preserves_files() {
    tracing_init();

    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("album dir");
    fs::create_dir_all(&db_dir).expect("db dir");

    let _file_data = generate_test_files(&album_dir);
    let original_files: Vec<_> = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ]
    .iter()
    .map(|f| album_dir.join(f))
    .collect();

    // Verify files exist before import
    for path in &original_files {
        assert!(path.exists(), "Test file should exist: {:?}", path);
    }

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager =
        bae_core::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);

    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());

    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager.clone(),
        encryption_service,
        database_arc,
        bae_core::keys::KeyService::new(true, "test".to_string()),
        bae_core::library_dir::LibraryDir::new(db_dir.clone()),
    );

    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir.clone(),
            master_year: 2024,
            managed: false,
            selected_cover: None,
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

    // Verify import succeeded
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks after import");

    // Verify audio_format records exist with file_id linkage
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio_format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have an audio_format record",
            track.title
        );
    }

    // Files should still exist after import
    for path in &original_files {
        assert!(
            path.exists(),
            "File should still exist after unmanaged import: {:?}",
            path
        );
    }

    // Now delete the release
    info!("Deleting release {}", release_id);
    let library_dir = bae_core::library_dir::LibraryDir::new(db_dir.clone());
    shared_library_manager
        .get()
        .delete_release(&release_id, &library_dir)
        .await
        .expect("delete release");

    // Verify database records are gone
    let tracks_after = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks after delete");
    assert!(
        tracks_after.is_empty(),
        "Tracks should be deleted from database"
    );

    let album_after = library_manager
        .get_album_by_id(&album_id)
        .await
        .expect("get album after delete");
    assert!(
        album_after.is_none(),
        "Album should be deleted (was last release)"
    );

    // THE KEY ASSERTION: Original files must still exist on disk
    for path in &original_files {
        assert!(
            path.exists(),
            "Original file must be preserved after deleting unmanaged release: {:?}",
            path
        );
    }

    info!("Unmanaged delete preserves original files");
}

async fn run_managed_test() {
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("Failed to create album dir");
    fs::create_dir_all(&db_dir).expect("Failed to create db dir");
    let file_data = generate_test_files(&album_dir);
    info!("Generated {} test files", file_data.len());
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("Failed to create database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager =
        bae_core::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());

    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service.clone(),
        database_arc,
        bae_core::keys::KeyService::new(true, "test".to_string()),
        bae_core::library_dir::LibraryDir::new(db_dir.clone()),
    );
    let discogs_release = create_test_discogs_release();
    let master_year = discogs_release.year.unwrap_or(2024);
    let selected_cover = "scans/back.jpg".to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir.clone(),
            master_year,
            managed: true,
            selected_cover: Some(CoverSelection::Local(selected_cover.clone())),
            import_id: uuid::Uuid::new_v4().to_string(),
        })
        .await
        .expect("Failed to send import request");
    info!("Import request sent, release_id: {}", release_id);
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    let mut track_complete_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut release_complete_received = false;
    let mut release_progress_received = false;
    let mut max_release_percent: u8 = 0;
    let mut progress_events_with_chunk_phase = 0;
    while let Some(progress) = progress_rx.recv().await {
        info!("Progress: {:?}", progress);
        match &progress {
            ImportProgress::Progress {
                id, percent, phase, ..
            } => {
                if id == &release_id {
                    assert_eq!(
                        *phase,
                        Some(ImportPhase::Store),
                        "Storage import progress should have phase=Store",
                    );
                    progress_events_with_chunk_phase += 1;
                    release_progress_received = true;
                    if *percent > max_release_percent {
                        max_release_percent = *percent;
                    }
                }
            }
            ImportProgress::Complete {
                id,
                release_id: rid,
                ..
            } => {
                if rid.is_none() {
                    release_complete_received = true;
                    info!("Release completion event received!");
                    break;
                } else {
                    track_complete_ids.insert(id.clone());
                    info!("Track completion event received for: {}", id);
                }
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }

    // Verify release is managed locally
    let release = database
        .get_release_by_id(&release_id)
        .await
        .expect("Failed to query release")
        .expect("release should exist");
    assert!(
        release.managed_locally,
        "Managed import should set managed_locally"
    );
    info!("Release is managed locally");

    let releases = library_manager
        .get_releases_for_album(&_album_id)
        .await
        .expect("Failed to get releases");
    let tracks = library_manager
        .get_tracks(&releases[0].id)
        .await
        .expect("Failed to get tracks");
    assert_eq!(tracks.len(), 3, "Expected 3 tracks");
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete, got {:?}",
            track.title,
            track.import_status,
        );
    }
    info!("All {} tracks are Complete", tracks.len());
    assert!(
        release_complete_received,
        "Should receive ImportProgress::Complete event for release",
    );
    assert_eq!(
        track_complete_ids.len(),
        tracks.len(),
        "Should receive ImportProgress::Complete event for each track. Got {} events for {} tracks. Track IDs received: {:?}",
        track_complete_ids.len(),
        tracks.len(),
        track_complete_ids,
    );
    info!("Received Complete events for all {} tracks", tracks.len());
    assert!(
        release_progress_received,
        "Should receive ImportProgress::Progress events for release during import",
    );
    assert_eq!(
        max_release_percent, 100,
        "Release progress should reach 100%"
    );
    assert!(
        progress_events_with_chunk_phase > 0,
        "Should receive multiple Progress events with phase=Store",
    );
    info!(
        "Received {} Progress events for release (max: {}%, phase=Store)",
        progress_events_with_chunk_phase, max_release_percent
    );
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("Failed to get files");
    assert!(!files.is_empty(), "Should have file records");

    // Verify files exist at derived storage paths
    let library_dir = bae_core::library_dir::LibraryDir::new(db_dir.clone());
    for file in &files {
        let storage_path = file.local_storage_path(&library_dir);
        assert!(
            storage_path.exists(),
            "File '{}' should exist at derived path: {:?}",
            file.original_filename,
            storage_path,
        );
    }
    info!("{} DbFile records with storage paths", files.len());

    // Verify audio format records for tracks
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format")
            .expect("Audio format should exist for track");
        assert_eq!(
            audio_format.content_type,
            ContentType::Flac,
            "Should be FLAC format"
        );
        assert!(
            audio_format.file_id.is_some(),
            "Track '{}' audio_format should have a file_id",
            track.title
        );
        info!("Track '{}' has audio format with file_id", track.title);
    }

    let cover = library_manager
        .get_library_image(&release_id, &LibraryImageType::Cover)
        .await
        .expect("Failed to get cover")
        .expect("Cover should exist in library_images");
    assert_eq!(cover.content_type, ContentType::Jpeg);
    assert_eq!(cover.source, "local");
    let source_url = cover.source_url.as_ref().expect("source_url should be set");
    assert!(
        source_url.starts_with("release://"),
        "source_url should start with release://, got: {}",
        source_url,
    );
    assert!(
        source_url.contains(&selected_cover),
        "source_url should contain selected cover '{}', got: {}",
        selected_cover,
        source_url,
    );
    info!("Cover library_image record exists with correct source");
    let album_id = library_manager
        .get_album_id_for_release(&release_id)
        .await
        .expect("Failed to get album_id");
    let album = library_manager
        .get_album_by_id(&album_id)
        .await
        .expect("Failed to get album")
        .expect("Album should exist");
    assert_eq!(
        album.cover_release_id.as_ref(),
        Some(&release_id),
        "Album cover_release_id should match the release",
    );
    info!("Album cover_release_id is set correctly");

    // Verify roundtrip: audio format records exist with file_id
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format")
            .expect("Audio format should exist for single-file track");
        assert_eq!(
            audio_format.content_type,
            ContentType::Flac,
            "Should be FLAC format"
        );
        assert!(
            audio_format.file_id.is_some(),
            "Track '{}' audio_format should have a file_id",
            track.title
        );
    }
    info!("Managed import test passed");
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-release-storage".to_string(),
        title: "Storage Test Album".to_string(),
        year: Some(2024),
        genre: vec![],
        style: vec![],
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![],
        tracklist: vec![
            DiscogsTrack {
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("3:00".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("4:00".to_string()),
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("2:30".to_string()),
            },
        ],
        master_id: Some("test-master-storage".to_string()),
        is_compilation: false,
    }
}

fn generate_test_files(dir: &Path) -> Vec<Vec<u8>> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac")
        .join("01 Test Track 1.flac");
    let flac_template = std::fs::read(&fixture_path)
        .expect("Failed to read FLAC fixture - run scripts/generate_test_flac.sh");
    let files = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ];
    let bae_dir = dir.join(".bae");
    fs::create_dir_all(&bae_dir).expect("Failed to create .bae directory");
    let minimal_jpeg: Vec<u8> = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B,
        0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
        0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00,
        0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05,
        0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
        0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
        0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A,
        0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
        0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56,
        0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75,
        0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93,
        0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9,
        0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6,
        0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
        0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
        0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD5,
        0xDB, 0x20, 0xA8, 0xF1, 0x7E, 0xFF, 0xD9,
    ];
    fs::write(bae_dir.join("cover-mb.jpg"), &minimal_jpeg).expect("Failed to write cover image");
    fs::write(dir.join("cover.jpg"), &minimal_jpeg).expect("Failed to write local cover");
    let scans_dir = dir.join("scans");
    fs::create_dir_all(&scans_dir).expect("Failed to create scans directory");
    fs::write(scans_dir.join("front.jpg"), &minimal_jpeg).expect("Failed to write scans/front.jpg");
    fs::write(scans_dir.join("back.jpg"), &minimal_jpeg).expect("Failed to write scans/back.jpg");
    files
        .iter()
        .map(|filename| {
            let file_path = dir.join(filename);
            fs::write(&file_path, &flac_template).expect("Failed to write FLAC file");
            flac_template.clone()
        })
        .collect()
}

/// Test with a real album - run with:
/// REAL_ALBUM_PATH="/path/to/album" cargo test --test test_storage --features test-utils test_real_album -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn test_real_album() {
    tracing_init();
    let real_album_path = std::env::var("REAL_ALBUM_PATH")
        .map(PathBuf::from)
        .expect("Set REAL_ALBUM_PATH env var to run this test");
    if !real_album_path.exists() {
        panic!("Real album path does not exist: {:?}", real_album_path);
    }
    run_real_album_test(real_album_path).await;
}

async fn run_real_album_test(album_dir: PathBuf) {
    info!("\n\n========== Testing REAL ALBUM ==========\n");
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&db_dir).expect("Failed to create db dir");
    let entries: Vec<_> = fs::read_dir(&album_dir)
        .expect("Failed to read album dir")
        .filter_map(|e| e.ok())
        .collect();
    info!("Album contains {} entries:", entries.len());
    for entry in &entries {
        info!("  - {:?}", entry.file_name());
    }
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("Failed to create database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager =
        bae_core::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service.clone(),
        database_arc.clone(),
        bae_core::keys::KeyService::new(true, "test".to_string()),
        bae_core::library_dir::LibraryDir::new(db_dir.clone()),
    );
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            discogs_release: None,
            mb_release: None,
            folder: album_dir.clone(),
            master_year: 1981,
            managed: true,
            selected_cover: None,
            import_id: uuid::Uuid::new_v4().to_string(),
        })
        .await
        .expect("Failed to send import request");
    info!("Import request sent, release_id: {}", release_id);
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete { .. } => {
                info!("Import completed!");
                break;
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {
                info!("Progress: {:?}", progress);
            }
        }
    }
    info!("\n--- Verifying database state ---");
    let release = database
        .get_release_by_id(&release_id)
        .await
        .expect("Failed to get release")
        .expect("release should exist");
    info!("managed_locally: {}", release.managed_locally);
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("Failed to get tracks");
    info!("\nTracks ({}):", tracks.len());
    for track in &tracks {
        info!(
            "  - [{}] '{}' status={:?}",
            track.track_number.unwrap_or(0),
            track.title,
            track.import_status
        );
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format");
        if let Some(af) = audio_format {
            info!(
                "    audio_format: {}, flac_headers={}",
                af.content_type,
                af.flac_headers.is_some()
            );
        } else {
            info!("    audio_format: None");
        }
    }
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("Failed to get files");
    info!("\nFiles ({}):", files.len());
    for file in &files {
        info!("  - '{}'", file.original_filename);
    }
    let complete_count = tracks
        .iter()
        .filter(|t| t.import_status == ImportStatus::Complete)
        .count();
    let queued_count = tracks
        .iter()
        .filter(|t| t.import_status == ImportStatus::Queued)
        .count();
    info!(
        "\nTrack status summary: {} Complete, {} Queued, {} total",
        complete_count,
        queued_count,
        tracks.len()
    );
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete but is {:?}",
            track.title,
            track.import_status,
        );
    }
    let cover = library_manager
        .get_library_image(&release_id, &LibraryImageType::Cover)
        .await
        .expect("Failed to get cover");
    assert!(cover.is_some(), "Should have a cover in library_images");
    info!("Cover library_image record exists");
    info!("\nAll tracks are Complete!");
}
