import SwiftUI

/// Wraps AppHandle and provides reactive playback state via the AppEventHandler callback.
/// @unchecked Sendable because mutations happen on MainActor via Task dispatch.
@Observable
class AppService: AppEventHandler, @unchecked Sendable {
    let appHandle: AppHandle

    var playbackState: BridgePlaybackState = .stopped
    var currentPositionMs: UInt64 = 0
    var currentDurationMs: UInt64 = 0
    var currentTrackId: String?
    var queueTrackIds: [String] = []
    var volume: Float = 1.0
    var repeatMode: BridgeRepeatMode = .none

    init(appHandle: AppHandle) {
        self.appHandle = appHandle
        appHandle.setEventHandler(handler: self)
    }

    // MARK: - AppEventHandler conformance

    func onPlaybackStateChanged(state: BridgePlaybackState) {
        Task { @MainActor in
            self.playbackState = state
            switch state {
            case let .playing(trackId, _, _, _, _, positionMs, durationMs):
                self.currentTrackId = trackId
                self.currentPositionMs = positionMs
                self.currentDurationMs = durationMs
            case let .paused(trackId, _, _, _, _, positionMs, durationMs):
                self.currentTrackId = trackId
                self.currentPositionMs = positionMs
                self.currentDurationMs = durationMs
            case .loading(let trackId):
                self.currentTrackId = trackId
            case .stopped:
                self.currentTrackId = nil
                self.currentPositionMs = 0
                self.currentDurationMs = 0
            }
        }
    }

    func onPlaybackProgress(positionMs: UInt64, durationMs: UInt64, trackId: String) {
        Task { @MainActor in
            self.currentPositionMs = positionMs
            if durationMs > 0 {
                self.currentDurationMs = durationMs
            }
            self.currentTrackId = trackId
        }
    }

    func onQueueUpdated(trackIds: [String]) {
        Task { @MainActor in
            self.queueTrackIds = trackIds
        }
    }

    func onError(message: String) {
        print("Playback error: \(message)")
    }

    // MARK: - Playback controls

    func playAlbum(albumId: String, startTrackIndex: UInt32? = nil) {
        Task.detached { [appHandle] in
            try? appHandle.playAlbum(albumId: albumId, startTrackIndex: startTrackIndex)
        }
    }

    func playTracks(trackIds: [String]) {
        appHandle.playTracks(trackIds: trackIds)
    }

    func togglePlayPause() {
        switch playbackState {
        case .playing:
            appHandle.pause()
        case .paused:
            appHandle.resume()
        default:
            break
        }
    }

    func nextTrack() {
        appHandle.nextTrack()
    }

    func previousTrack() {
        appHandle.previousTrack()
    }

    func seek(positionMs: UInt64) {
        appHandle.seek(positionMs: positionMs)
    }

    func setVolume(_ vol: Float) {
        volume = vol
        appHandle.setVolume(volume: vol)
    }

    func cycleRepeatMode() {
        switch repeatMode {
        case .none:
            repeatMode = .album
            appHandle.setRepeatMode(mode: .album)
        case .album:
            repeatMode = .track
            appHandle.setRepeatMode(mode: .track)
        case .track:
            repeatMode = .none
            appHandle.setRepeatMode(mode: .none)
        }
    }

    /// Whether there is an active track (playing, paused, or loading).
    var isActive: Bool {
        switch playbackState {
        case .stopped:
            return false
        default:
            return true
        }
    }

    var isPlaying: Bool {
        if case .playing = playbackState { return true }
        return false
    }

    /// Current track title, or nil if nothing is active.
    var trackTitle: String? {
        switch playbackState {
        case let .playing(_, trackTitle, _, _, _, _, _):
            return trackTitle
        case let .paused(_, trackTitle, _, _, _, _, _):
            return trackTitle
        default:
            return nil
        }
    }

    /// Current artist names, or nil if nothing is active.
    var artistNames: String? {
        switch playbackState {
        case let .playing(_, _, artistNames, _, _, _, _):
            return artistNames
        case let .paused(_, _, artistNames, _, _, _, _):
            return artistNames
        default:
            return nil
        }
    }

    /// Cover image ID for the currently playing track's album.
    var coverImageId: String? {
        switch playbackState {
        case let .playing(_, _, _, _, coverImageId, _, _):
            return coverImageId
        case let .paused(_, _, _, _, coverImageId, _, _):
            return coverImageId
        default:
            return nil
        }
    }

    // MARK: - Settings

    func getConfig() -> BridgeConfig {
        appHandle.getConfig()
    }

    func renameLibrary(name: String) throws {
        try appHandle.renameLibrary(name: name)
    }

    func saveDiscogsToken(token: String) throws {
        try appHandle.saveDiscogsToken(token: token)
    }

    func getDiscogsToken() -> String? {
        appHandle.getDiscogsToken()
    }

    func removeDiscogsToken() throws {
        try appHandle.removeDiscogsToken()
    }
}
