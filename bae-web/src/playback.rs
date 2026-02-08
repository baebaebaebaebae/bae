use bae_ui::display_types::{QueueItem, Track};
use bae_ui::stores::playback::{PlaybackStatus, PlaybackUiState, PlaybackUiStateStoreExt};
use dioxus::prelude::*;
use std::collections::{HashMap, VecDeque};
use tracing::info;

/// Display info for a track, passed by callers who already have the data
pub struct TrackInfo {
    pub track_id: String,
    pub track: Track,
    pub album_title: String,
    pub cover_url: Option<String>,
    pub artist_name: String,
    pub artist_id: Option<String>,
}

/// Cached display info for building QueueItems
struct CachedTrackInfo {
    track: Track,
    album_title: String,
    cover_url: Option<String>,
    artist_name: String,
    artist_id: Option<String>,
}

/// Repeat mode for web playback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    None,
    Track,
    Album,
}

/// Web playback service managing an HTML <audio> element, queue, and store updates
pub struct WebPlaybackService {
    queue: VecDeque<String>,
    current_track_id: Option<String>,
    previous_track_id: Option<String>,
    repeat_mode: RepeatMode,
    store: Store<PlaybackUiState>,
    audio: Option<web_sys_x::HtmlMediaElement>,
    track_cache: HashMap<String, CachedTrackInfo>,
    pre_mute_volume: f32,
}

impl WebPlaybackService {
    pub fn new(store: Store<PlaybackUiState>) -> Self {
        Self {
            queue: VecDeque::new(),
            current_track_id: None,
            previous_track_id: None,
            repeat_mode: RepeatMode::None,
            store,
            audio: None,
            track_cache: HashMap::new(),
            pre_mute_volume: 1.0,
        }
    }

    /// Set the audio element reference (called from layout's onmounted)
    pub fn set_audio_element(&mut self, el: web_sys_x::HtmlMediaElement) {
        // Apply current volume to the element
        let volume = *self.store.volume().read();
        el.set_volume(volume as f64);
        self.audio = Some(el);
    }

    /// Play tracks: first track plays, rest goes to queue
    pub fn play_album(&mut self, infos: Vec<TrackInfo>) {
        if infos.is_empty() {
            return;
        }

        self.queue.clear();

        // Cache all track info
        let mut track_ids: Vec<String> = Vec::with_capacity(infos.len());
        for info in infos {
            track_ids.push(info.track_id.clone());
            self.cache_track_info(info);
        }

        // Queue all, pop first to play
        for id in &track_ids {
            self.queue.push_back(id.clone());
        }

        if let Some(first) = self.queue.pop_front() {
            if let Some(old) = self.current_track_id.take() {
                self.previous_track_id = Some(old);
            }
            self.play_track_by_id(&first);
        }

        self.sync_queue_to_store();
    }

    pub fn pause(&mut self) {
        if let Some(ref audio) = self.audio {
            let _ = audio.pause();
        }
        self.store.status().set(PlaybackStatus::Paused);
    }

    pub fn resume(&mut self) {
        if let Some(ref audio) = self.audio {
            let _ = audio.play();
        }
        self.store.status().set(PlaybackStatus::Playing);
    }

    pub fn next(&mut self) {
        self.advance_to_next();
    }

    pub fn previous(&mut self) {
        let position_ms = *self.store.position_ms().read();
        if position_ms < 3000 {
            if let Some(prev_id) = self.previous_track_id.clone() {
                self.play_track_by_id(&prev_id);
                self.sync_queue_to_store();
                return;
            }
        }
        // Restart current track
        self.seek(0);
    }

    pub fn seek(&mut self, ms: u64) {
        if let Some(ref audio) = self.audio {
            audio.set_current_time(ms as f64 / 1000.0);
        }
        self.store.position_ms().set(ms);
    }

    pub fn set_volume(&mut self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        if let Some(ref audio) = self.audio {
            audio.set_volume(vol as f64);
        }
        self.store.volume().set(vol);
    }

    pub fn toggle_mute(&mut self) {
        let current = *self.store.volume().read();
        if current > 0.0 {
            self.pre_mute_volume = current;
            self.set_volume(0.0);
        } else {
            self.set_volume(self.pre_mute_volume);
        }
    }

    pub fn cycle_repeat_mode(&mut self) {
        self.repeat_mode = match self.repeat_mode {
            RepeatMode::None => RepeatMode::Album,
            RepeatMode::Album => RepeatMode::Track,
            RepeatMode::Track => RepeatMode::None,
        };

        let store_mode = match self.repeat_mode {
            RepeatMode::None => bae_ui::stores::playback::RepeatMode::None,
            RepeatMode::Track => bae_ui::stores::playback::RepeatMode::Track,
            RepeatMode::Album => bae_ui::stores::playback::RepeatMode::Album,
        };
        self.store.repeat_mode().set(store_mode);
    }

    // Queue operations

    pub fn add_to_queue_with_info(&mut self, infos: Vec<TrackInfo>) {
        for info in infos {
            self.queue.push_back(info.track_id.clone());
            self.cache_track_info(info);
        }
        self.sync_queue_to_store();
    }

    pub fn add_next_with_info(&mut self, infos: Vec<TrackInfo>) {
        for info in infos.into_iter().rev() {
            self.queue.push_front(info.track_id.clone());
            self.cache_track_info(info);
        }
        self.sync_queue_to_store();
    }

    pub fn remove_from_queue(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
            self.sync_queue_to_store();
        }
    }

    pub fn reorder_queue(&mut self, from: usize, to: usize) {
        if from < self.queue.len() && to < self.queue.len() && from != to {
            if let Some(track_id) = self.queue.remove(from) {
                if to > from {
                    self.queue.insert(to - 1, track_id);
                } else {
                    self.queue.insert(to, track_id);
                }
                self.sync_queue_to_store();
            }
        }
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.sync_queue_to_store();
    }

    pub fn skip_to(&mut self, index: usize) {
        if index >= self.queue.len() {
            return;
        }

        // Drain all tracks before the target index
        for _ in 0..index {
            self.queue.pop_front();
        }

        if let Some(track_id) = self.queue.pop_front() {
            if let Some(old) = self.current_track_id.take() {
                self.previous_track_id = Some(old);
            }
            self.play_track_by_id(&track_id);
            self.sync_queue_to_store();
        }
    }

    // Audio event handlers (called from layout's event bindings)

    pub fn on_time_update(&mut self) {
        if let Some(ref audio) = self.audio {
            let current_time = audio.current_time();
            self.store.position_ms().set((current_time * 1000.0) as u64);
        }
    }

    pub fn on_loaded_metadata(&mut self) {
        if let Some(ref audio) = self.audio {
            let duration = audio.duration();
            if duration.is_finite() {
                self.store.duration_ms().set((duration * 1000.0) as u64);
            }
        }
    }

    pub fn on_ended(&mut self) {
        self.advance_to_next();
    }

    pub fn on_error(&mut self) {
        self.store
            .playback_error()
            .set(Some("Failed to play audio".to_string()));
        self.store.status().set(PlaybackStatus::Stopped);
    }

    pub fn on_play(&mut self) {
        let status = *self.store.status().read();
        if status == PlaybackStatus::Loading || status == PlaybackStatus::Stopped {
            // Transition from Loading to Playing
        }
        self.store.status().set(PlaybackStatus::Playing);
    }

    pub fn on_pause_event(&mut self) {
        // Only set Paused if we're not stopping (ended triggers pause before ended)
        let status = *self.store.status().read();
        if status == PlaybackStatus::Playing {
            self.store.status().set(PlaybackStatus::Paused);
        }
    }

    pub fn dismiss_error(&mut self) {
        self.store.playback_error().set(None);
    }

    // Private helpers

    fn play_track_by_id(&mut self, track_id: &str) {
        self.store.status().set(PlaybackStatus::Loading);
        self.store.position_ms().set(0);
        self.store.duration_ms().set(0);
        self.store.playback_error().set(None);
        self.current_track_id = Some(track_id.to_string());
        self.store
            .current_track_id()
            .set(Some(track_id.to_string()));

        // Update display info from cache
        if let Some(cached) = self.track_cache.get(track_id) {
            self.store.current_track().set(Some(QueueItem {
                track: cached.track.clone(),
                album_title: cached.album_title.clone(),
                cover_url: cached.cover_url.clone(),
            }));
            self.store.artist_name().set(cached.artist_name.clone());
            self.store.artist_id().set(cached.artist_id.clone());
            self.store.cover_url().set(cached.cover_url.clone());
        }

        // Set audio src and play
        if let Some(ref audio) = self.audio {
            let src = format!("/rest/stream?id={}", track_id);
            audio.set_src(&src);
            let _ = audio.play();
        }

        info!("Playing track: {}", track_id);
    }

    fn advance_to_next(&mut self) {
        // Repeat Track: replay current
        if self.repeat_mode == RepeatMode::Track {
            if let Some(ref id) = self.current_track_id {
                let id = id.clone();
                self.play_track_by_id(&id);
                return;
            }
        }

        // Try queue
        if let Some(next_id) = self.queue.pop_front() {
            if let Some(old) = self.current_track_id.take() {
                self.previous_track_id = Some(old);
            }
            self.play_track_by_id(&next_id);
            self.sync_queue_to_store();
            return;
        }

        // Repeat Album: not implemented for web (would need API call)
        // Just stop
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(ref audio) = self.audio {
            let _ = audio.pause();
            audio.set_src("");
        }
        self.store.status().set(PlaybackStatus::Stopped);
        self.store.position_ms().set(0);
        self.store.duration_ms().set(0);
        self.store.current_track_id().set(None);
        self.store.current_track().set(None);
        self.store.artist_name().set(String::new());
        self.store.artist_id().set(None);
        self.store.cover_url().set(None);
        self.current_track_id = None;
    }

    fn cache_track_info(&mut self, info: TrackInfo) {
        self.track_cache.insert(
            info.track_id,
            CachedTrackInfo {
                track: info.track,
                album_title: info.album_title,
                cover_url: info.cover_url,
                artist_name: info.artist_name,
                artist_id: info.artist_id,
            },
        );
    }

    fn sync_queue_to_store(&self) {
        let queue_ids: Vec<String> = self.queue.iter().cloned().collect();
        let queue_items: Vec<QueueItem> = queue_ids
            .iter()
            .filter_map(|id| {
                self.track_cache.get(id).map(|cached| QueueItem {
                    track: cached.track.clone(),
                    album_title: cached.album_title.clone(),
                    cover_url: cached.cover_url.clone(),
                })
            })
            .collect();

        self.store.queue().set(queue_ids);
        self.store.queue_items().set(queue_items);
    }
}
