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
    var searchQuery: String = ""
    var searchResults: BridgeSearchResults?

    // Import state
    var scanResults: [BridgeImportCandidate] = []
    var importStatuses: [String: BridgeImportStatus] = [:]
    var libraryVersion: Int = 0

    // Sync state
    var syncStatus: BridgeSyncStatus?

    init(appHandle: AppHandle) {
        self.appHandle = appHandle
        appHandle.setEventHandler(handler: self)

        // Start background sync loop if sync is configured
        if appHandle.isSyncReady() {
            appHandle.startSyncLoop()
        }
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

    func onScanResult(candidate: BridgeImportCandidate) {
        Task { @MainActor in
            self.scanResults.append(candidate)
        }
    }

    func onImportProgress(folderPath: String, status: BridgeImportStatus) {
        Task { @MainActor in
            self.importStatuses[folderPath] = status
        }
    }

    func onLibraryChanged() {
        Task { @MainActor in
            self.libraryVersion += 1
        }
    }

    func onSyncStatusChanged(status: BridgeSyncStatus) {
        Task { @MainActor in
            self.syncStatus = status
        }
    }

    func onError(message: String) {
        print("Error: \(message)")
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

    // MARK: - Import

    func scanFolder(path: String) {
        scanResults = []
        Task.detached { [appHandle] in
            try? appHandle.scanFolder(path: path)
        }
    }

    func searchMusicbrainz(artist: String, album: String) async -> [BridgeMetadataResult] {
        return await Task.detached { [appHandle] in
            (try? appHandle.searchMusicbrainz(artist: artist, album: album)) ?? []
        }.value
    }

    func searchDiscogs(artist: String, album: String) async -> [BridgeMetadataResult] {
        return await Task.detached { [appHandle] in
            (try? appHandle.searchDiscogs(artist: artist, album: album)) ?? []
        }.value
    }

    func commitImport(folderPath: String, releaseId: String, source: String) {
        Task.detached { [appHandle] in
            try? appHandle.commitImport(
                folderPath: folderPath,
                releaseId: releaseId,
                source: source
            )
        }
    }

    func searchByCatalogNumber(catalog: String, source: String) async -> [BridgeMetadataResult] {
        return await Task.detached { [appHandle] in
            (try? appHandle.searchByCatalogNumber(catalogNumber: catalog, source: source)) ?? []
        }.value
    }

    func searchByBarcode(barcode: String, source: String) async -> [BridgeMetadataResult] {
        return await Task.detached { [appHandle] in
            (try? appHandle.searchByBarcode(barcode: barcode, source: source)) ?? []
        }.value
    }

    func getCandidateFiles(folderPath: String) -> BridgeCandidateFiles? {
        try? appHandle.getCandidateFiles(folderPath: folderPath)
    }

    // MARK: - Search

    func search(query: String) {
        searchQuery = query
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            searchResults = nil
            return
        }

        Task.detached { [appHandle] in
            let results = try? appHandle.search(query: query)
            await MainActor.run { [weak self] in
                guard let self, self.searchQuery == query else { return }
                self.searchResults = results
            }
        }
    }

    // MARK: - Sync

    /// Trigger a manual sync cycle. Runs on a background thread since it blocks.
    func triggerSync() {
        Task.detached { [appHandle] in
            let status = try? appHandle.triggerSync()
            await MainActor.run { [weak self] in
                if let status {
                    self?.syncStatus = status
                }
            }
        }
    }

    // MARK: - Computed properties

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
