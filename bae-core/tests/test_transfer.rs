#![cfg(feature = "test-utils")]
//! Integration tests for the storage transfer service.
//!
//! Tests:
//! - Unmanaged → managed local transfer
//! - Managed local → eject (export to folder)
//! - Transfer preserves audio_format.file_id
//! - Empty release transfer fails gracefully

mod support;

use bae_core::content_type::ContentType;
use bae_core::db::{Database, DbAlbum, DbAudioFormat, DbFile, DbRelease, DbTrack, ImportStatus};
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
async fn create_album_and_release(db: &Database, unmanaged_path: Option<&str>) -> (String, String) {
    let now = Utc::now();
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Transfer Test Album".to_string(),
        year: Some(2024),
        discogs_release: None,
        musicbrainz_release: None,
        bandcamp_album_id: None,
        cover_release_id: None,
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
        managed_locally: false,
        managed_in_cloud: false,
        unmanaged_path: unmanaged_path.map(|s| s.to_string()),
        private: false,
        created_at: now,
        updated_at: now,
    };
    db.insert_release(&release).await.unwrap();

    (album.id, release.id)
}

/// Create unmanaged files on disk and insert DbFile records
async fn create_unmanaged_files(
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

        let db_file = DbFile::new(release_id, name, data.len() as i64, ContentType::Flac);
        mgr.add_file(&db_file).await.unwrap();

        result.push((name.to_string(), data.to_vec()));
    }

    result
}

/// Create managed local files on disk and insert DbFile records
async fn create_managed_local_files(
    db: &Database,
    mgr: &LibraryManager,
    release_id: &str,
    library_dir: &LibraryDir,
) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("track1.flac", b"stored-data-track-one" as &[u8]),
        ("track2.flac", b"stored-data-track-two"),
    ];

    let mut result = Vec::new();
    for (name, data) in &files {
        let db_file = DbFile::new(release_id, name, data.len() as i64, ContentType::Flac);

        // Write data to the derived storage path
        let storage_path = db_file.local_storage_path(library_dir);
        tokio::fs::create_dir_all(storage_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&storage_path, data).await.unwrap();

        mgr.add_file(&db_file).await.unwrap();

        result.push((name.to_string(), data.to_vec()));
    }

    // Mark release as managed locally
    db.set_release_managed_locally(release_id, true)
        .await
        .unwrap();

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

/// Transfer unmanaged files into managed local storage.
/// Verifies: files written to storage, DB updated, original files preserved, no pending deletions.
#[tokio::test]
async fn test_transfer_unmanaged_to_managed_local() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("source");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&source_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) =
        create_album_and_release(&db, Some(source_dir.to_str().unwrap())).await;
    let original_files = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    // Execute transfer
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
    );
    let rx = service.transfer(release_id.clone(), TransferTarget::ManagedLocal);
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

    // Verify DB: release is now managed locally
    let release = shared_mgr
        .get()
        .database()
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(release.managed_locally);

    // Verify files exist at managed local storage paths
    let library_dir = LibraryDir::new(library_path.clone());
    let new_files = shared_mgr
        .get()
        .get_files_for_release(&release_id)
        .await
        .unwrap();
    assert_eq!(new_files.len(), original_files.len());
    for file in &new_files {
        let storage_path = file.local_storage_path(&library_dir);
        assert!(
            storage_path.exists(),
            "Stored file should exist: {:?}",
            storage_path
        );
    }

    // Verify original files are preserved (unmanaged sources are never deleted)
    for (name, _) in &original_files {
        let orig_path = source_dir.join(name);
        assert!(
            orig_path.exists(),
            "Original file should be preserved: {:?}",
            orig_path
        );
    }

    // Unmanaged sources should NOT queue deletions
    let pending = read_pending_deletions(&library_path).await;
    assert!(
        pending.is_empty(),
        "Unmanaged transfer should not queue deletions"
    );
}

/// Eject from managed local storage to a user-chosen folder.
/// Verifies: files written to target folder, release becomes unmanaged,
/// old managed files queued for deletion.
#[tokio::test]
async fn test_eject_from_managed_local() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let eject_dir = temp.path().join("ejected");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db, None).await;

    let library_dir = LibraryDir::new(library_path.clone());
    let original_files = create_managed_local_files(&db, &mgr, &release_id, &library_dir).await;

    // Execute eject
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
    );
    let rx = service.transfer(release_id.clone(), TransferTarget::Eject(eject_dir.clone()));
    let events = collect_progress(rx).await;

    // Verify success
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));

    // Verify DB: release is now unmanaged
    let release = shared_mgr
        .get()
        .database()
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!release.managed_locally);
    assert_eq!(
        release.unmanaged_path.as_deref(),
        Some(eject_dir.to_str().unwrap())
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

    // Verify old managed files queued for deletion
    let pending = read_pending_deletions(&library_path).await;
    assert_eq!(pending.len(), original_files.len());
    for deletion in &pending {
        let PendingDeletion::Local { .. } = deletion;
    }
}

/// Transfer preserves audio_format.file_id since file records are updated in place.
#[tokio::test]
async fn test_transfer_preserves_audio_format_file_ids() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("source");
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&source_dir).await.unwrap();
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) =
        create_album_and_release(&db, Some(source_dir.to_str().unwrap())).await;

    // Create an unmanaged FLAC file
    let file_path = source_dir.join("track1.flac");
    tokio::fs::write(&file_path, b"flac-data").await.unwrap();
    let db_file = DbFile::new(&release_id, "track1.flac", 9, ContentType::Flac);
    let file_id = db_file.id.clone();
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
    .with_file_id(&file_id);
    db.insert_audio_format(&af).await.unwrap();

    // Transfer to managed local
    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(
        shared_mgr.clone(),
        None,
        LibraryDir::new(library_path.clone()),
    );
    let rx = service.transfer(release_id.clone(), TransferTarget::ManagedLocal);
    let events = collect_progress(rx).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));

    // File ID unchanged -- file record was updated in place, not deleted/recreated
    let af_after = db
        .get_audio_format_by_track_id(&track.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        af_after.file_id.as_deref(),
        Some(file_id.as_str()),
        "audio_format.file_id should be unchanged after transfer"
    );

    // The file should now exist at the managed local storage path
    let library_dir = LibraryDir::new(library_path);
    let file_after = shared_mgr
        .get()
        .get_file_by_id(&file_id)
        .await
        .unwrap()
        .unwrap();
    let storage_path = file_after.local_storage_path(&library_dir);
    assert!(
        storage_path.exists(),
        "File should exist at managed local storage path"
    );
}

/// Transfer with no files should fail gracefully.
#[tokio::test]
async fn test_transfer_empty_release_fails() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp).await;
    let (_album_id, release_id) = create_album_and_release(&db, None).await;
    // No files created

    let shared_mgr = bae_core::library::SharedLibraryManager::new(mgr);
    let service = TransferService::new(shared_mgr, None, LibraryDir::new(library_path));
    let rx = service.transfer(release_id.clone(), TransferTarget::ManagedLocal);
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Transfer with no files should fail"
    );
}
