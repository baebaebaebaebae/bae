import SwiftUI

/// Wraps AppHandle and provides reactive playback state via the AppEventHandler callback.
/// @unchecked Sendable because mutations happen on MainActor via Task dispatch.
@Observable
class AppService: AppEventHandler, @unchecked Sendable {
    let appHandle: AppHandle
    let mediaControlService = MediaControlService()

    var playbackState: BridgePlaybackState = .stopped
    var currentPositionMs: UInt64 = 0
    var currentDurationMs: UInt64 = 0
    var currentTrackId: String?
    var queueTrackIds: [String] = []
    var queueItems: [BridgeQueueItem] = []
    var volume: Float = 1.0
    var repeatMode: BridgeRepeatMode = .none
    var searchQuery: String = ""
    var searchResults: BridgeSearchResults?

    // Import state
    var scanResults: [BridgeImportCandidate] = []
    var importStatuses: [String: BridgeImportStatus] = [:]
    var libraryVersion: Int = 0

    /// Sync state
    var syncStatus: BridgeSyncStatus?

    init(appHandle: AppHandle) {
        self.appHandle = appHandle
        appHandle.setEventHandler(handler: self)
        mediaControlService.setupRemoteCommands(appService: self)

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
            case let .playing(trackId, _, _, _, _, _, _, positionMs, durationMs):
                self.currentTrackId = trackId
                self.currentPositionMs = positionMs
                self.currentDurationMs = durationMs
            case let .paused(trackId, _, _, _, _, _, _, positionMs, durationMs):
                self.currentTrackId = trackId
                self.currentPositionMs = positionMs
                self.currentDurationMs = durationMs
            case let .loading(trackId):
                self.currentTrackId = trackId
            case .stopped:
                self.currentTrackId = nil
                self.currentPositionMs = 0
                self.currentDurationMs = 0
            }
            self.mediaControlService.updateNowPlaying(state: state, appHandle: self.appHandle)
        }
    }

    func onPlaybackProgress(positionMs: UInt64, durationMs: UInt64, trackId: String) {
        Task { @MainActor in
            self.currentPositionMs = positionMs
            if durationMs > 0 {
                self.currentDurationMs = durationMs
            }
            self.currentTrackId = trackId
            self.mediaControlService.updatePosition(positionMs: positionMs, durationMs: self.currentDurationMs)
        }
    }

    func onQueueUpdated(trackIds: [String]) {
        Task { @MainActor in
            self.queueTrackIds = trackIds
            self.queueItems = self.appHandle.getQueueItems(trackIds: trackIds)
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

    func playAlbum(albumId: String, startTrackIndex: UInt32? = nil, shuffle: Bool = false) {
        Task.detached { [appHandle] in
            try? appHandle.playAlbum(albumId: albumId, startTrackIndex: startTrackIndex, shuffle: shuffle)
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

    // MARK: - Queue

    func addToQueue(trackIds: [String]) {
        appHandle.addToQueue(trackIds: trackIds)
    }

    func addNext(trackIds: [String]) {
        appHandle.addNext(trackIds: trackIds)
    }

    func insertInQueue(trackIds: [String], index: UInt32) {
        appHandle.insertInQueue(trackIds: trackIds, index: index)
    }

    func removeFromQueue(index: UInt32) {
        appHandle.removeFromQueue(index: index)
    }

    func reorderQueue(fromIndex: UInt32, toIndex: UInt32) {
        // Optimistic local reorder for instant UI feedback (mirrors PlaybackQueue.reorder logic)
        let from = Int(fromIndex)
        let to = Int(toIndex)
        if from < queueItems.count, to <= queueItems.count, from != to {
            let item = queueItems.remove(at: from)
            if to > from {
                queueItems.insert(item, at: to - 1)
            } else {
                queueItems.insert(item, at: to)
            }
        }
        appHandle.reorderQueue(fromIndex: fromIndex, toIndex: toIndex)
    }

    func clearQueue() {
        appHandle.clearQueue()
    }

    func skipToQueueIndex(index: UInt32) {
        appHandle.skipToQueueIndex(index: index)
    }

    // MARK: - Import

    func scanFolder(path: String) {
        scanResults = []
        importStatuses = [:]
        Task.detached { [appHandle] in
            try? appHandle.scanFolder(path: path)
        }
    }

    /// Scan another folder, appending results to the existing list.
    func scanAdditionalFolder(path: String) {
        Task.detached { [appHandle] in
            try? appHandle.scanFolder(path: path)
        }
    }

    func clearAllCandidates() {
        scanResults = []
        importStatuses = [:]
    }

    func removeCandidate(folderPath: String) {
        importStatuses.removeValue(forKey: folderPath)
        scanResults.removeAll { $0.folderPath == folderPath }
    }

    func clearCompletedCandidates() {
        let completed = scanResults.filter { candidate in
            if case .complete = importStatuses[candidate.folderPath] { return true }
            return false
        }
        for candidate in completed {
            importStatuses.removeValue(forKey: candidate.folderPath)
        }
        scanResults.removeAll { candidate in
            completed.contains { $0.folderPath == candidate.folderPath }
        }
    }

    func clearIncompleteCandidates() {
        let incomplete = scanResults.filter { $0.badAudioCount > 0 || $0.badImageCount > 0 }
        for candidate in incomplete {
            importStatuses.removeValue(forKey: candidate.folderPath)
        }
        scanResults.removeAll { candidate in
            incomplete.contains { $0.folderPath == candidate.folderPath }
        }
    }

    func searchMusicbrainz(artist: String, album: String, year: String? = nil, label: String? = nil) async -> [BridgeMetadataResult] {
        await Task.detached { [appHandle] in
            (try? appHandle.searchMusicbrainz(artist: artist, album: album, year: year, label: label)) ?? []
        }.value
    }

    func searchDiscogs(artist: String, album: String, year: String? = nil, label: String? = nil) async -> [BridgeMetadataResult] {
        await Task.detached { [appHandle] in
            (try? appHandle.searchDiscogs(artist: artist, album: album, year: year, label: label)) ?? []
        }.value
    }

    func commitImport(folderPath: String, releaseId: String, source: String, selectedCover: BridgeCoverSelection? = nil, managed: Bool = true) {
        Task.detached { [appHandle] in
            try? appHandle.commitImport(
                folderPath: folderPath,
                releaseId: releaseId,
                source: source,
                selectedCover: selectedCover,
                managed: managed,
            )
        }
    }

    func lookupDiscId(discid: String) async -> BridgeDiscIdResult? {
        await Task.detached { [appHandle] in
            try? appHandle.lookupDiscid(discid: discid)
        }.value
    }

    func prefetchRelease(releaseId: String, source: String) async -> BridgeReleaseDetail? {
        await Task.detached { [appHandle] in
            try? appHandle.prefetchRelease(releaseId: releaseId, source: source)
        }.value
    }

    func searchByCatalogNumber(catalog: String, source: String) async -> [BridgeMetadataResult] {
        await Task.detached { [appHandle] in
            (try? appHandle.searchByCatalogNumber(catalogNumber: catalog, source: source)) ?? []
        }.value
    }

    func searchByBarcode(barcode: String, source: String) async -> [BridgeMetadataResult] {
        await Task.detached { [appHandle] in
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
                guard let self, searchQuery == query else { return }
                searchResults = results
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
            false
        default:
            true
        }
    }

    var isPlaying: Bool {
        if case .playing = playbackState { return true }
        return false
    }

    /// Current track title, or nil if nothing is active.
    var trackTitle: String? {
        switch playbackState {
        case let .playing(_, trackTitle, _, _, _, _, _, _, _):
            trackTitle
        case let .paused(_, trackTitle, _, _, _, _, _, _, _):
            trackTitle
        default:
            nil
        }
    }

    /// Current artist names, or nil if nothing is active.
    var artistNames: String? {
        switch playbackState {
        case let .playing(_, _, artistNames, _, _, _, _, _, _):
            artistNames
        case let .paused(_, _, artistNames, _, _, _, _, _, _):
            artistNames
        default:
            nil
        }
    }

    /// Cover image ID for the currently playing track's album.
    var coverImageId: String? {
        switch playbackState {
        case let .playing(_, _, _, _, _, _, coverImageId, _, _):
            coverImageId
        case let .paused(_, _, _, _, _, _, coverImageId, _, _):
            coverImageId
        default:
            nil
        }
    }

    /// Album ID for the currently playing track.
    var currentAlbumId: String? {
        switch playbackState {
        case let .playing(_, _, _, _, albumId, _, _, _, _):
            albumId
        case let .paused(_, _, _, _, albumId, _, _, _, _):
            albumId
        default:
            nil
        }
    }

    /// Artist ID for the currently playing track (first artist).
    var currentArtistId: String? {
        switch playbackState {
        case let .playing(_, _, _, artistId, _, _, _, _, _):
            artistId
        case let .paused(_, _, _, artistId, _, _, _, _, _):
            artistId
        default:
            nil
        }
    }

    // MARK: - Image URL helper

    func imageURL(for imageId: String?) -> URL? {
        guard let id = imageId,
              let str = appHandle.getImageUrl(imageId: id),
              let url = URL(string: str) else { return nil }
        return url
    }

    func fileURL(for fileId: String) -> URL? {
        URL(string: appHandle.getFileUrl(fileId: fileId))
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
