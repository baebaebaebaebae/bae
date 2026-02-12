//! Scan event consumption: converts folder scan events into import state updates.

use super::conversion::{categorized_files_from_scanned, to_display_metadata};
use super::load_selected_release;
use crate::ui::app_service::AppService;
use bae_core::import::{
    detect_folder_contents, DetectedCandidate as CoreDetectedCandidate, ScanEvent,
};
use bae_ui::display_types::{CategorizedFileInfo, FolderMetadata as DisplayFolderMetadata};
use bae_ui::stores::AppStateStoreExt;
use dioxus::prelude::*;
use std::collections::HashSet;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Detect local metadata and files for a candidate before it is shown in the UI.
pub fn detect_candidate_locally(
    candidate: &CoreDetectedCandidate,
    imgs: &bae_core::image_server::ImageServerHandle,
) -> Result<(CategorizedFileInfo, DisplayFolderMetadata), String> {
    let files = categorized_files_from_scanned(&candidate.files, imgs);

    info!(
        "Detecting metadata for candidate: {} ({:?})",
        candidate.name, candidate.path
    );

    let folder_contents = detect_folder_contents(candidate.path.clone())
        .map_err(|e| format!("Failed to detect folder contents: {}", e))?;
    let core_metadata = folder_contents.metadata;

    info!(
        "Detected metadata: artist={:?}, album={:?}, year={:?}, mb_discid={:?}",
        core_metadata.artist, core_metadata.album, core_metadata.year, core_metadata.mb_discid
    );

    let metadata = to_display_metadata(&core_metadata);

    Ok((files, metadata))
}

/// Consume folder scan events and update import state
pub async fn consume_scan_events(app: AppService, mut rx: broadcast::Receiver<ScanEvent>) {
    loop {
        let mut import_store = app.state.import();
        let existing_paths: HashSet<String> = {
            let state = import_store.read();
            state
                .detected_candidates
                .iter()
                .map(|c| c.path.clone())
                .collect()
        };

        let mut first_selected_index = None;

        loop {
            match rx.recv().await {
                Ok(ScanEvent::Candidate(candidate)) => {
                    let key = candidate.path.to_string_lossy().to_string();
                    if existing_paths.contains(&key) {
                        continue;
                    }

                    let (files, metadata) =
                        match detect_candidate_locally(&candidate, &app.image_server) {
                            Ok(result) => result,
                            Err(e) => {
                                warn!(
                                    "Skipping candidate {} due to detection failure: {}",
                                    candidate.name, e
                                );
                                continue;
                            }
                        };

                    // Convert to display type
                    let display_candidate = bae_ui::display_types::DetectedCandidate {
                        name: candidate.name.clone(),
                        path: key.clone(),
                        status: bae_ui::display_types::DetectedCandidateStatus::Pending,
                    };

                    {
                        let mut state = import_store.write();
                        state.init_state_machine(&key, files, metadata);
                        state.detected_candidates.push(display_candidate);

                        if state.current_candidate_key.is_none() {
                            let index = state.detected_candidates.len() - 1;
                            state.switch_candidate(Some(key));
                            state.current_release_index = index;
                            first_selected_index = Some(index);
                        }
                    }
                }
                Ok(ScanEvent::Error(error)) => {
                    warn!("Scan error: {}", error);
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Ok(ScanEvent::Finished) => {
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Scan event receiver lagged, missed {} events", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    import_store.write().is_scanning_candidates = false;
                    return;
                }
            }
        }

        // After scan completes, load the first selected release if any
        if let Some(index) = first_selected_index {
            let detected = import_store.read().detected_candidates.clone();
            if let Err(e) = load_selected_release(&app, index, &detected).await {
                warn!("Failed to load selected release: {}", e);
            }
        }
    }
}
