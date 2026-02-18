use bae_core::config::Config;
use bae_core::encryption::EncryptionService;
use bae_core::image_server::ImageServerHandle;
use bae_core::import::ImportServiceHandle;
use bae_core::keys::{KeyService, UserKeypair};
use bae_core::library::SharedLibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::playback::PlaybackHandle;
use bae_core::sync::bucket::SyncBucketClient;
use bae_core::sync::hlc::Timestamp;
use bae_core::sync::service::SyncService;
use bae_core::sync::session::SyncSession;
use tracing::{error, info, warn};

use crate::ui::app_context::SyncHandle;
use crate::ui::app_service::{
    clear_staged_changeset, push_changeset, read_staged_changeset, stage_changeset,
};

/// Run bae in headless mode (no GUI).
///
/// Starts the Subsonic server (spawned), runs the sync loop on the main task
/// (because it holds a raw sqlite3 pointer that is not Send), and waits for
/// SIGTERM / Ctrl-C.
pub fn run(
    runtime: tokio::runtime::Runtime,
    config: Config,
    library_manager: SharedLibraryManager,
    encryption_service: Option<EncryptionService>,
    key_service: KeyService,
    sync_handle: Option<SyncHandle>,
    image_server: ImageServerHandle,
    user_keypair: Option<UserKeypair>,
    _import_handle: ImportServiceHandle,
    _playback_handle: PlaybackHandle,
) {
    runtime.block_on(async {
        let auth = crate::build_subsonic_auth(&config, &key_service);

        tokio::spawn(crate::start_subsonic_server(
            library_manager.clone(),
            encryption_service,
            config.server_port,
            config.server_bind_address.clone(),
            config.library_dir.clone(),
            key_service,
            auth,
        ));

        info!("bae headless server running");

        info!(
            "  Subsonic: http://{}:{}",
            config.server_bind_address, config.server_port
        );
        info!(
            "  Image server: http://{}:{}",
            image_server.host, image_server.port
        );

        // Run sync loop on the main task (not spawned) because SyncHandle
        // contains a raw sqlite3 pointer that is not Send. Use select! so
        // we still respond to shutdown signals.
        match (sync_handle, user_keypair) {
            (Some(sync), Some(keypair)) => {
                tokio::select! {
                    _ = run_headless_sync_loop(
                        &sync,
                        &keypair,
                        &library_manager,
                        &config.library_dir,
                    ) => {}
                    _ = wait_for_shutdown_signal() => {}
                }
            }
            _ => {
                wait_for_shutdown_signal().await;
            }
        }

        info!("Shutting down");
    });
}

/// Headless sync loop -- same logic as app_service.rs but without Store updates.
async fn run_headless_sync_loop(
    sync_handle: &SyncHandle,
    user_keypair: &UserKeypair,
    library_manager: &SharedLibraryManager,
    library_dir: &LibraryDir,
) {
    let db = library_manager.get().database();
    let device_id = &sync_handle.device_id;
    let bucket: &dyn SyncBucketClient = &*sync_handle.bucket_client;
    let hlc = &sync_handle.hlc;
    let sync_service = SyncService::new(device_id.clone());

    // Load persisted sync state
    let mut local_seq = db
        .get_sync_state("local_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let mut snapshot_seq: Option<u64> = db
        .get_sync_state("snapshot_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    let mut last_snapshot_time: Option<chrono::DateTime<chrono::Utc>> = db
        .get_sync_state("last_snapshot_time")
        .await
        .ok()
        .flatten()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let mut staged_seq: Option<u64> = db
        .get_sync_state("staged_seq")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok());

    let mut trigger_rx = match sync_handle.take_trigger_rx().await {
        Some(rx) => rx,
        None => {
            error!("Failed to take sync trigger receiver");
            return;
        }
    };

    // Initial delay to avoid racing with server startup
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    loop {
        // Retry staged changeset from a previous failed push
        if let Some(seq) = staged_seq {
            if let Some(staged_data) = read_staged_changeset(library_dir) {
                let timestamp = hlc.now().to_string();

                info!(seq, "Retrying staged changeset push");

                match push_changeset(
                    bucket,
                    device_id,
                    seq,
                    staged_data,
                    snapshot_seq,
                    &timestamp,
                )
                .await
                {
                    Ok(()) => {
                        info!(seq, "Staged changeset push succeeded");
                        clear_staged_changeset(library_dir);
                        staged_seq = None;
                        local_seq = seq;
                        let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                        let _ = db.set_sync_state("staged_seq", "").await;
                    }
                    Err(e) => {
                        warn!("Staged changeset push failed: {e}");
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                            msg = trigger_rx.recv() => { if msg.is_none() { break; } }
                        }
                        continue;
                    }
                }
            } else {
                staged_seq = None;
                let _ = db.set_sync_state("staged_seq", "").await;
            }
        }

        // Load cursors
        let cursors = match db.get_all_sync_cursors().await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to load sync cursors: {e}");
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                    msg = trigger_rx.recv() => { if msg.is_none() { break; } }
                }
                continue;
            }
        };

        // Take session
        let session = match sync_handle.session.lock().await.take() {
            Some(s) => s,
            None => {
                warn!("Sync session was None, creating a new one");
                match unsafe { SyncSession::start(sync_handle.raw_db()) } {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to create sync session: {e}");
                        break;
                    }
                }
            }
        };

        let timestamp = hlc.now().to_string();

        let sync_result = unsafe {
            sync_service
                .sync(
                    sync_handle.raw_db(),
                    session,
                    local_seq,
                    &cursors,
                    bucket,
                    &timestamp,
                    "headless sync",
                    user_keypair,
                    None,
                    library_dir,
                )
                .await
        };

        match sync_result {
            Ok(result) => {
                // Push outgoing changeset
                if let Some(outgoing) = &result.outgoing {
                    let seq = outgoing.seq;

                    stage_changeset(library_dir, &outgoing.packed);
                    staged_seq = Some(seq);
                    let _ = db.set_sync_state("staged_seq", &seq.to_string()).await;

                    match push_changeset(
                        bucket,
                        device_id,
                        seq,
                        outgoing.packed.clone(),
                        snapshot_seq,
                        &timestamp,
                    )
                    .await
                    {
                        Ok(()) => {
                            clear_staged_changeset(library_dir);
                            staged_seq = None;
                            local_seq = seq;
                            let _ = db.set_sync_state("local_seq", &seq.to_string()).await;
                            let _ = db.set_sync_state("staged_seq", "").await;

                            info!(seq, "Pushed changeset");
                        }
                        Err(e) => {
                            warn!(seq, "Push failed, changeset staged for retry: {e}");
                        }
                    }
                }

                // Persist cursors
                for (cursor_device_id, cursor_seq) in &result.updated_cursors {
                    if let Err(e) = db.set_sync_cursor(cursor_device_id, *cursor_seq).await {
                        warn!(
                            device_id = cursor_device_id,
                            seq = cursor_seq,
                            "Failed to persist sync cursor: {e}"
                        );
                    }
                }

                // Update HLC with max remote timestamp
                let max_remote_ts = result
                    .pull
                    .remote_heads
                    .iter()
                    .filter(|h| h.device_id != *device_id)
                    .filter_map(|h| h.last_sync.as_deref())
                    .filter_map(|ts_str| {
                        chrono::DateTime::parse_from_rfc3339(ts_str)
                            .ok()
                            .map(|dt| dt.timestamp_millis().max(0) as u64)
                    })
                    .max();

                if let Some(remote_millis) = max_remote_ts {
                    let remote_ts = Timestamp::new(remote_millis, 0, "remote".to_string());
                    hlc.update(&remote_ts);
                }

                // Start new session
                match unsafe { SyncSession::start(sync_handle.raw_db()) } {
                    Ok(new_session) => {
                        *sync_handle.session.lock().await = Some(new_session);
                    }
                    Err(e) => {
                        error!("Failed to start new sync session: {e}");
                        continue;
                    }
                }

                // Persist snapshot_seq
                if let Some(ss) = snapshot_seq {
                    let _ = db.set_sync_state("snapshot_seq", &ss.to_string()).await;
                }

                if result.pull.changesets_applied > 0 {
                    info!(
                        applied = result.pull.changesets_applied,
                        "Applied remote changes"
                    );
                }

                // Check snapshot policy
                let hours_since = last_snapshot_time.map(|t| {
                    chrono::Utc::now()
                        .signed_duration_since(t)
                        .num_hours()
                        .max(0) as u64
                });

                if bae_core::sync::snapshot::should_create_snapshot(
                    local_seq,
                    snapshot_seq,
                    hours_since,
                ) {
                    info!("Creating snapshot");

                    let temp_dir = std::env::temp_dir();
                    let snapshot_result = {
                        let enc = sync_handle.encryption.read().unwrap();
                        unsafe {
                            bae_core::sync::snapshot::create_snapshot(
                                sync_handle.raw_db(),
                                &temp_dir,
                                &enc,
                            )
                        }
                    };

                    match snapshot_result {
                        Ok(encrypted) => {
                            match bae_core::sync::snapshot::push_snapshot(
                                bucket, encrypted, device_id, local_seq,
                            )
                            .await
                            {
                                Ok(()) => {
                                    snapshot_seq = Some(local_seq);
                                    last_snapshot_time = Some(chrono::Utc::now());
                                    let _ = db
                                        .set_sync_state("snapshot_seq", &local_seq.to_string())
                                        .await;
                                    let _ = db
                                        .set_sync_state(
                                            "last_snapshot_time",
                                            &chrono::Utc::now().to_rfc3339(),
                                        )
                                        .await;

                                    info!(local_seq, "Snapshot created and pushed");
                                }
                                Err(e) => warn!("Failed to push snapshot: {e}"),
                            }
                        }
                        Err(e) => warn!("Failed to create snapshot: {e}"),
                    }
                }
            }
            Err(e) => {
                warn!("Sync cycle failed: {e}");
                // Try to restart session
                if let Ok(new_session) = unsafe { SyncSession::start(sync_handle.raw_db()) } {
                    *sync_handle.session.lock().await = Some(new_session);
                }
            }
        }

        // Wait for next cycle
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
            msg = trigger_rx.recv() => {
                if msg.is_none() {
                    info!("Sync trigger channel closed, stopping sync loop");
                    break;
                }
            }
        }
    }
}

async fn wait_for_shutdown_signal() {
    use tokio::signal;

    let ctrl_c = signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
