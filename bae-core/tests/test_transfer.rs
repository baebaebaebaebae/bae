#![cfg(feature = "test-utils")]
//! Integration tests for the storage transfer service.
//!
//! Tests:
//! - Self-managed → local profile transfer
//! - Local profile → local profile transfer
//! - Local profile → eject (export to folder)
//! - Self-managed originals are preserved after transfer

mod support;

use bae_core::content_type::ContentType;
use bae_core::db::{
    Database, DbAlbum, DbAudioFormat, DbFile, DbRelease, DbReleaseStorage, DbStorageProfile,
    DbTrack, ImportStatus,
};
use bae_core::keys::KeyService;
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::storage::cleanup::PendingDeletion;
use bae_core::storage::transfer::{TransferProgress, TransferService, TransferTarget};
use chrono::Utc;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Set up a database and library manager in a temp directory
async fn setup_db(temp: &TempDir) -> (Database, LibraryManager) {
    let db_path = temp.path().join("test.db");
    let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
    let enc = support::test_encryption_service();
    let mgr = LibraryManager::new(db.clone(), enc);
    (db, mgr)
}

/// Create a test album + release in the DB, return (album_id, release_id)
async fn create_album_and_release(db: &Database) -> (String, String) {
    let now = Utc::now();
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Transfer Test Album".to_string(),
        year: Some(2024),
        discogs_release: None,
        musicbrainz_release: None,
        bandcamp_album_id: None,
        cover_release_id: None,
        cover_art_url: None,
        is_compilation: false,
        created_at: now,
        updated_at: now,
    };
    db.insert_album(&album).await.unwrap();

    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        year: None,
        discogs_release_id: None,
        bandcamp_release_id: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        import_status: ImportStatus::Complete,
        private: false,
        created_at: now,
        updated_at: now,
    };
    db.insert_release(&release).await.unwrap();

    (album.id, release.id)
}

/// Create self-managed files on disk and insert DbFile records pointing to them
async fn create_self_managed_files(
    mgr: &LibraryManager,
    release_id: &str,
    source_dir: &Path,
) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("track1.flac", b"file-data-track-one" as &[u8]),
        ("track2.flac", b"file-data-track-two"),
        ("cover.jpg", b"jpeg-cover-data"),
    ];

    let mut result = Vec::new();
    for (name, data) in &files {
        let file_path = source_dir.join(name);
        tokio::fs::write(&file_path, data).await.unwrap();

        let mut db_file = DbFile::new(release_id, name, data.len() as i64, ContentType::Flac);
        db_file.source_path = Some(file_path.display().to_string());
        mgr.add_file(&db_file).await.unwrap();

        result.push((name.to_string(), data.to_vec()));
    }

    result
}

/// Create files in a local storage profile directory and insert DbFile + release_storage records
async fn create_profile_files(
    db: &Database,
    mgr: &LibraryManager,
    release_id: &str,
    profile: &DbStorageProfile,
    storage_dir: &Path,
) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("track1.flac", b"stored-data-track-one" as &[u8]),
        ("track2.flac", b"stored-data-track-two"),
    ];

    let release_dir = storage_dir.join(release_id);
    tokio::fs::create_dir_all(&release_dir).await.unwrap();

    let mut result = Vec::new();
    for (name, data) in &files {
        let file_path = release_dir.join(name);
        tokio::fs::write(&file_path, data).await.unwrap();

        let mut db_file = DbFile::new(release_id, name, data.len() as i64, ContentType::Flac);
        db_file.source_path = Some(file_path.display().to_string());
        mgr.add_file(&db_file).await.unwrap();

        result.push((name.to_string(), data.to_vec()));
    }

    // Link release to profile
    let rs = DbReleaseStorage::new(release_id, &profile.id);
    db.set_release_storage(&rs).await.unwrap();

    result
}

/// Drain all progress events from a transfer receiver
async fn collect_progress(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TransferProgress>,
) -> Vec<TransferProgress> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let is_terminal = matches!(
            event,
            TransferProgress::Complete { .. } | TransferProgress::Failed { .. }
        );
        events.push(event);
        if is_terminal {
            break;
        }
    }
    events
}

/// Read the pending_deletions.json manifest from the library path
async fn read_pending_deletions(library_path: &Path) -> Vec<PendingDeletion> {
    let manifest = library_path.join("pending_deletions.json");
    if !manifest.exists() {
        return Vec::new();
    }
    let contents = tokio::fs::read_to_string(&manifest).await.unwrap();
    serde_json::from_str(&contents).unwrap()
}

/// Transfer self-managed files into a local storage profile.
/// Verifies: files written to storage, DB updated, original files preserved, no pending deletions.
#[tokio::test]
async fn test_transfer_self_managed_to_local_profile() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("source");
    let storage_dir = temp.path().join("storage");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&source_dir).await.unwrap();
    tokio::fs::create_dir_all(&storage_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db).await;
    let original_files = create_self_managed_files(&mgr, &release_id, &source_dir).await;

    // Create destination profile
    let dest_profile =
        DbStorageProfile::new_local("Local Storage", storage_dir.to_str().unwrap(), false);
    db.insert_storage_profile(&dest_profile).await.unwrap();

    // Execute transfer
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
        KeyService::new(true, "test".to_string()),
    );
    let rx = service.transfer(
        release_id.clone(),
        TransferTarget::Profile(dest_profile.clone()),
    );
    let events = collect_progress(rx).await;

    // Verify progress events
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Started { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. })));

    // Verify DB: release now linked to dest profile
    let profile = shared_mgr
        .get()
        .get_storage_profile_for_release(&release_id)
        .await
        .unwrap();
    assert!(profile.is_some());
    assert_eq!(profile.unwrap().id, dest_profile.id);

    // Verify DB: new file records exist with storage paths
    let new_files = shared_mgr
        .get()
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    assert_eq!(new_files.len(), original_files.len());
    for file in &new_files {
        let source_path = file.source_path.as_ref().unwrap();
        assert!(
            source_path.contains(storage_dir.to_str().unwrap()),
            "New file should be in storage dir: {}",
            source_path
        );
        // Verify file actually exists on disk
        assert!(
            Path::new(source_path).exists(),
            "Stored file should exist: {}",
            source_path
        );
    }

    // Verify original files are preserved (self-managed → no pending deletions)
    for (name, _) in &original_files {
        let orig_path = source_dir.join(name);
        assert!(
            orig_path.exists(),
            "Original file should be preserved: {:?}",
            orig_path
        );
    }

    // Self-managed sources should NOT queue deletions
    let pending = read_pending_deletions(&library_path).await;
    assert!(
        pending.is_empty(),
        "Self-managed transfer should not queue deletions"
    );
}

/// Transfer from one local profile to another.
/// Verifies: files written to new location, DB updated, old files queued for deletion.
#[tokio::test]
async fn test_transfer_local_profile_to_local_profile() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let storage_a = temp.path().join("storage_a");
    let storage_b = temp.path().join("storage_b");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&storage_a).await.unwrap();
    tokio::fs::create_dir_all(&storage_b).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db).await;

    // Create source profile and files
    let source_profile =
        DbStorageProfile::new_local("Profile A", storage_a.to_str().unwrap(), false);
    db.insert_storage_profile(&source_profile).await.unwrap();
    let original_files =
        create_profile_files(&db, &mgr, &release_id, &source_profile, &storage_a).await;

    // Create destination profile
    let dest_profile = DbStorageProfile::new_local("Profile B", storage_b.to_str().unwrap(), false);
    db.insert_storage_profile(&dest_profile).await.unwrap();

    // Record old file paths for checking pending deletions
    let old_file_paths: Vec<String> = mgr
        .get_files_for_release(&release_id)
        .await
        .unwrap()
        .iter()
        .map(|f| f.source_path.clone().unwrap())
        .collect();

    // Execute transfer
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
        KeyService::new(true, "test".to_string()),
    );
    let rx = service.transfer(
        release_id.clone(),
        TransferTarget::Profile(dest_profile.clone()),
    );
    let events = collect_progress(rx).await;

    // Verify success
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));

    // Verify DB: release now linked to dest profile
    let profile = shared_mgr
        .get()
        .get_storage_profile_for_release(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(profile.id, dest_profile.id);

    // Verify new files exist in storage_b
    let new_files = shared_mgr
        .get()
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    assert_eq!(new_files.len(), original_files.len());
    for file in &new_files {
        let source_path = file.source_path.as_ref().unwrap();
        assert!(
            source_path.contains(storage_b.to_str().unwrap()),
            "New file should be in storage_b: {}",
            source_path
        );
        assert!(Path::new(source_path).exists());
    }

    // Verify old files are queued for deferred deletion
    let pending = read_pending_deletions(&library_path).await;
    assert_eq!(pending.len(), old_file_paths.len());
    for deletion in &pending {
        match deletion {
            PendingDeletion::Local { path } => {
                assert!(
                    old_file_paths.contains(path),
                    "Pending deletion should be an old file: {}",
                    path
                );
            }
            _ => panic!("Expected Local deletion for local profile transfer"),
        }
    }
}

/// Eject from a local profile to a user-chosen folder.
/// Verifies: files written to target folder, release_storage removed, new DbFile records
/// point to ejected location, old files queued for deletion.
#[tokio::test]
async fn test_eject_from_local_profile() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path().join("storage");
    let eject_dir = temp.path().join("ejected");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&storage_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db).await;

    // Create source profile and files
    let source_profile =
        DbStorageProfile::new_local("Managed Storage", storage_dir.to_str().unwrap(), false);
    db.insert_storage_profile(&source_profile).await.unwrap();
    let original_files =
        create_profile_files(&db, &mgr, &release_id, &source_profile, &storage_dir).await;

    // Execute eject
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
        KeyService::new(true, "test".to_string()),
    );
    let rx = service.transfer(release_id.clone(), TransferTarget::Eject(eject_dir.clone()));
    let events = collect_progress(rx).await;

    // Verify success
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));

    // Verify DB: no storage profile linked
    let profile = shared_mgr
        .get()
        .get_storage_profile_for_release(&release_id)
        .await
        .unwrap();
    assert!(
        profile.is_none(),
        "Ejected release should not have a storage profile"
    );

    // Verify files exist in eject directory
    for (name, data) in &original_files {
        let ejected_path = eject_dir.join(name);
        assert!(
            ejected_path.exists(),
            "Ejected file should exist: {:?}",
            ejected_path
        );
        let ejected_data = tokio::fs::read(&ejected_path).await.unwrap();
        assert_eq!(&ejected_data, data, "Ejected file content should match");
    }

    // Verify DB: new file records point to ejected location
    let new_files = shared_mgr
        .get()
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    assert_eq!(new_files.len(), original_files.len());
    for file in &new_files {
        let source_path = file.source_path.as_ref().unwrap();
        assert!(
            source_path.contains(eject_dir.to_str().unwrap()),
            "Ejected file record should point to eject dir: {}",
            source_path
        );
    }

    // Verify old files queued for deletion
    let pending = read_pending_deletions(&library_path).await;
    assert_eq!(pending.len(), original_files.len());
}

/// Transfer preserves audio_format.file_id since file records are updated in place.
#[tokio::test]
async fn test_transfer_preserves_audio_format_file_ids() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("source");
    let storage_dir = temp.path().join("storage");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&source_dir).await.unwrap();
    tokio::fs::create_dir_all(&storage_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db).await;

    // Create a self-managed FLAC file
    let file_path = source_dir.join("track1.flac");
    tokio::fs::write(&file_path, b"flac-data").await.unwrap();
    let mut db_file = DbFile::new(&release_id, "track1.flac", 9, ContentType::Flac);
    db_file.source_path = Some(file_path.display().to_string());
    mgr.add_file(&db_file).await.unwrap();

    // Create a track and audio_format linked to the file
    let now = Utc::now();
    let track = DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.clone(),
        title: "Track One".to_string(),
        disc_number: None,
        track_number: Some(1),
        duration_ms: Some(180000),
        discogs_position: None,
        import_status: ImportStatus::Complete,
        updated_at: now,
        created_at: now,
    };
    db.insert_track(&track).await.unwrap();

    let af = DbAudioFormat::new(
        &track.id,
        ContentType::Flac,
        None,
        false,
        44100,
        16,
        "[]".to_string(),
        0,
    )
    .with_file_id(&db_file.id);
    db.insert_audio_format(&af).await.unwrap();

    // Create destination profile and transfer
    let dest_profile =
        DbStorageProfile::new_local("Local Storage", storage_dir.to_str().unwrap(), false);
    db.insert_storage_profile(&dest_profile).await.unwrap();

    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
        KeyService::new(true, "test".to_string()),
    );
    let rx = service.transfer(
        release_id.clone(),
        TransferTarget::Profile(dest_profile.clone()),
    );
    let events = collect_progress(rx).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));

    // File ID unchanged — file record was updated in place, not deleted/recreated
    let af_after = db
        .get_audio_format_by_track_id(&track.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        af_after.file_id.as_deref(),
        Some(db_file.id.as_str()),
        "audio_format.file_id should be unchanged after transfer"
    );

    // The file record's source_path should now point to the new storage
    let file_after = shared_mgr
        .get()
        .get_file_by_id(&db_file.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        file_after
            .source_path
            .as_ref()
            .unwrap()
            .contains(storage_dir.to_str().unwrap()),
        "File source_path should point to new storage"
    );
}

/// Transfer with no files should fail gracefully.
#[tokio::test]
async fn test_transfer_empty_release_fails() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path().join("storage");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&storage_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db).await;
    // No files created

    let dest_profile = DbStorageProfile::new_local("Storage", storage_dir.to_str().unwrap(), false);
    db.insert_storage_profile(&dest_profile).await.unwrap();

    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr,
        None,
        LibraryDir::new(library_path),
        KeyService::new(true, "test".to_string()),
    );
    let rx = service.transfer(release_id.clone(), TransferTarget::Profile(dest_profile));
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Transfer with no files should fail"
    );
}
