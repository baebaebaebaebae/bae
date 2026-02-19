import AVFoundation
import Foundation
import MediaPlayer

@Observable
class PlaybackService {
    // MARK: - Public State

    private(set) var currentTrack: Track?
    private(set) var currentAlbumArtId: String?
    private(set) var isPlaying: Bool = false
    private(set) var progress: TimeInterval = 0
    private(set) var duration: TimeInterval = 0
    private(set) var isLoading: Bool = false
    private(set) var error: String?

    // MARK: - Private State

    private var player: AVAudioPlayer?
    private var playerDelegate: PlayerDelegate?
    private var progressTimer: Timer?
    private var tempFileURL: URL?
    private var queue: [Track] = []
    private var queueIndex: Int = 0

    private let cloudClient: CloudHomeClient
    private let crypto: CryptoService
    private let encryptionKey: Data
    private let databaseService: DatabaseService

    // MARK: - Init

    init(
        cloudClient: CloudHomeClient, crypto: CryptoService, encryptionKey: Data,
        databaseService: DatabaseService
    ) {
        self.cloudClient = cloudClient
        self.crypto = crypto
        self.encryptionKey = encryptionKey
        self.databaseService = databaseService
        configureAudioSession()
        setupRemoteCommands()
    }

    // MARK: - Playback Controls

    func play(track: Track, albumArtId: String?, allTracks: [Track]) async {
        stop()
        currentTrack = track
        currentAlbumArtId = albumArtId
        isLoading = true
        error = nil
        queue = allTracks
        queueIndex = allTracks.firstIndex(where: { $0.id == track.id }) ?? 0

        do {
            // 1. Look up audio format
            guard let audioFormat = try databaseService.audioFormatForTrack(trackId: track.id)
            else {
                error = "No audio data for this track"
                isLoading = false
                return
            }
            guard let fileId = audioFormat.fileId else {
                error = "No file associated with this track"
                isLoading = false
                return
            }

            // 2. Download encrypted file from cloud
            let storageKey = Self.storagePath(for: fileId)
            let encrypted = try await cloudClient.readBlob(key: storageKey)

            // 3. Decrypt with per-release key
            let releaseKey = try crypto.deriveReleaseKey(
                masterKey: encryptionKey, releaseId: track.releaseId)
            let decrypted = try crypto.decryptFile(ciphertext: encrypted, key: releaseKey)

            // 4. Extract track audio (handle CUE/FLAC if needed)
            let audioData: Data
            if audioFormat.needsHeaders, let headers = audioFormat.flacHeaders {
                let dataStart = audioFormat.audioDataStart
                let start = (audioFormat.startByteOffset ?? 0) + dataStart
                let end = audioFormat.endByteOffset.map { $0 + dataStart } ?? decrypted.count
                var trackData = headers
                trackData.append(decrypted.subdata(in: start..<min(end, decrypted.count)))
                audioData = trackData
            } else {
                audioData = decrypted
            }

            // 5. Write to temp file
            let ext = audioFormat.contentType.contains("flac") ? "flac" : "m4a"
            let tempURL = FileManager.default.temporaryDirectory
                .appendingPathComponent("bae-playback-\(track.id).\(ext)")
            try audioData.write(to: tempURL)
            tempFileURL = tempURL

            // 6. Play
            let audioPlayer = try AVAudioPlayer(contentsOf: tempURL)
            let delegate = PlayerDelegate { [weak self] in
                guard let self else { return }
                Task { @MainActor in
                    await self.playNext()
                }
            }
            audioPlayer.delegate = delegate
            playerDelegate = delegate
            audioPlayer.prepareToPlay()
            audioPlayer.play()

            player = audioPlayer
            duration = audioPlayer.duration
            isPlaying = true
            isLoading = false
            startProgressTimer()
            updateNowPlayingInfo()
        } catch {
            self.error = error.localizedDescription
            isLoading = false
        }
    }

    func pause() {
        player?.pause()
        isPlaying = false
        stopProgressTimer()
        updateNowPlayingElapsedTime()
    }

    func resume() {
        player?.play()
        isPlaying = true
        startProgressTimer()
        updateNowPlayingElapsedTime()
    }

    func togglePlayPause() {
        if isPlaying {
            pause()
        } else {
            resume()
        }
    }

    func stop() {
        stopProgressTimer()
        player?.stop()
        player = nil
        playerDelegate = nil
        isPlaying = false
        progress = 0
        duration = 0
        cleanupTempFile()
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    }

    func seek(to time: TimeInterval) {
        player?.currentTime = time
        progress = time
        updateNowPlayingElapsedTime()
    }

    func playNext() async {
        guard !queue.isEmpty else { return }
        let nextIndex = queueIndex + 1
        guard nextIndex < queue.count else {
            stop()
            currentTrack = nil
            currentAlbumArtId = nil
            return
        }
        let albumArtId = currentAlbumArtId
        await play(track: queue[nextIndex], albumArtId: albumArtId, allTracks: queue)
    }

    func playPrevious() async {
        // Restart current track if more than 3 seconds in
        if progress > 3, let track = currentTrack {
            let albumArtId = currentAlbumArtId
            await play(track: track, albumArtId: albumArtId, allTracks: queue)
            return
        }

        guard !queue.isEmpty else { return }
        let prevIndex = queueIndex - 1
        guard prevIndex >= 0 else {
            // Restart first track
            if currentTrack != nil {
                seek(to: 0)
                if !isPlaying { resume() }
            }
            return
        }
        let albumArtId = currentAlbumArtId
        await play(track: queue[prevIndex], albumArtId: albumArtId, allTracks: queue)
    }

    // MARK: - Now Playing Info

    func updateNowPlayingArtwork(_ image: UIImage) {
        var info = MPNowPlayingInfoCenter.default().nowPlayingInfo ?? [:]
        let artwork = MPMediaItemArtwork(boundsSize: image.size) { _ in image }
        info[MPMediaItemPropertyArtwork] = artwork
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    // MARK: - Storage Path

    static func storagePath(for fileId: String) -> String {
        let hex = fileId.replacingOccurrences(of: "-", with: "")
        let ab = String(hex.prefix(2))
        let cd = String(hex.dropFirst(2).prefix(2))
        return "storage/\(ab)/\(cd)/\(fileId)"
    }

    // MARK: - Private

    private func configureAudioSession() {
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
            try AVAudioSession.sharedInstance().setActive(true)
        } catch {
            // Non-fatal: playback may still work
        }
    }

    private func setupRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()

        center.playCommand.addTarget { [weak self] _ in
            self?.resume()
            return .success
        }

        center.pauseCommand.addTarget { [weak self] _ in
            self?.pause()
            return .success
        }

        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            self?.togglePlayPause()
            return .success
        }

        center.nextTrackCommand.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            Task { await self.playNext() }
            return .success
        }

        center.previousTrackCommand.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            Task { await self.playPrevious() }
            return .success
        }

        center.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard let self,
                let posEvent = event as? MPChangePlaybackPositionCommandEvent
            else {
                return .commandFailed
            }
            self.seek(to: posEvent.positionTime)
            return .success
        }
    }

    private func updateNowPlayingInfo() {
        guard let track = currentTrack else { return }

        var info = MPNowPlayingInfoCenter.default().nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyTitle] = track.title
        info[MPMediaItemPropertyArtist] = track.artistNames ?? ""
        if let ms = track.durationMs {
            info[MPMediaItemPropertyPlaybackDuration] = Double(ms) / 1000.0
        } else {
            info[MPMediaItemPropertyPlaybackDuration] = duration
        }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = progress
        info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? 1.0 : 0.0
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    private func updateNowPlayingElapsedTime() {
        guard MPNowPlayingInfoCenter.default().nowPlayingInfo != nil else { return }

        var info = MPNowPlayingInfoCenter.default().nowPlayingInfo!
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = progress
        info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? 1.0 : 0.0
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    private func startProgressTimer() {
        stopProgressTimer()
        progressTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) {
            [weak self] _ in
            guard let self, let player = self.player else { return }
            self.progress = player.currentTime
        }
    }

    private func stopProgressTimer() {
        progressTimer?.invalidate()
        progressTimer = nil
    }

    private func cleanupTempFile() {
        if let url = tempFileURL {
            try? FileManager.default.removeItem(at: url)
            tempFileURL = nil
        }
    }
}

// MARK: - AVAudioPlayer Delegate

private class PlayerDelegate: NSObject, AVAudioPlayerDelegate {
    private let onFinished: () -> Void

    init(onFinished: @escaping () -> Void) {
        self.onFinished = onFinished
    }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully _: Bool) {
        onFinished()
    }
}
