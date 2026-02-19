import AppKit
import MediaPlayer

/// Bridges playback state to macOS Now Playing (Control Center widget + media keys).
class MediaControlService {
    private var commandsRegistered = false
    private var cachedArtworkImageId: String?
    private var cachedArtwork: MPMediaItemArtwork?

    func setupRemoteCommands(appService: AppService) {
        guard !commandsRegistered else { return }
        commandsRegistered = true

        let center = MPRemoteCommandCenter.shared()

        center.playCommand.addTarget { [weak appService] _ in
            guard let appService else { return .noActionableNowPlayingItem }
            appService.appHandle.resume()
            return .success
        }

        center.pauseCommand.addTarget { [weak appService] _ in
            guard let appService else { return .noActionableNowPlayingItem }
            appService.appHandle.pause()
            return .success
        }

        center.togglePlayPauseCommand.addTarget { [weak appService] _ in
            guard let appService else { return .noActionableNowPlayingItem }
            appService.togglePlayPause()
            return .success
        }

        center.nextTrackCommand.addTarget { [weak appService] _ in
            guard let appService else { return .noActionableNowPlayingItem }
            appService.nextTrack()
            return .success
        }

        center.previousTrackCommand.addTarget { [weak appService] _ in
            guard let appService else { return .noActionableNowPlayingItem }
            appService.previousTrack()
            return .success
        }

        center.changePlaybackPositionCommand.addTarget { [weak appService] event in
            guard let appService,
                  let positionEvent = event as? MPChangePlaybackPositionCommandEvent
            else {
                return .noActionableNowPlayingItem
            }
            let positionMs = UInt64(positionEvent.positionTime * 1000)
            appService.seek(positionMs: positionMs)
            return .success
        }
    }

    func updateNowPlaying(state: BridgePlaybackState, appHandle: AppHandle) {
        let infoCenter = MPNowPlayingInfoCenter.default()

        switch state {
        case let .playing(_, trackTitle, artistNames, _, albumTitle, coverImageId, positionMs, durationMs):
            var info = infoCenter.nowPlayingInfo ?? [:]
            info[MPMediaItemPropertyTitle] = trackTitle
            info[MPMediaItemPropertyArtist] = artistNames
            info[MPMediaItemPropertyAlbumTitle] = albumTitle
            info[MPMediaItemPropertyPlaybackDuration] = Double(durationMs) / 1000.0
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = Double(positionMs) / 1000.0
            info[MPNowPlayingInfoPropertyPlaybackRate] = 1.0
            loadArtwork(imageId: coverImageId, appHandle: appHandle, into: &info)
            infoCenter.nowPlayingInfo = info

        case let .paused(_, trackTitle, artistNames, _, albumTitle, coverImageId, positionMs, durationMs):
            var info = infoCenter.nowPlayingInfo ?? [:]
            info[MPMediaItemPropertyTitle] = trackTitle
            info[MPMediaItemPropertyArtist] = artistNames
            info[MPMediaItemPropertyAlbumTitle] = albumTitle
            info[MPMediaItemPropertyPlaybackDuration] = Double(durationMs) / 1000.0
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = Double(positionMs) / 1000.0
            info[MPNowPlayingInfoPropertyPlaybackRate] = 0.0
            loadArtwork(imageId: coverImageId, appHandle: appHandle, into: &info)
            infoCenter.nowPlayingInfo = info

        case .loading:
            // Keep existing info during loading
            break

        case .stopped:
            infoCenter.nowPlayingInfo = nil
        }
    }

    func updatePosition(positionMs: UInt64, durationMs: UInt64) {
        let infoCenter = MPNowPlayingInfoCenter.default()
        guard var info = infoCenter.nowPlayingInfo else { return }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = Double(positionMs) / 1000.0
        info[MPMediaItemPropertyPlaybackDuration] = Double(durationMs) / 1000.0
        infoCenter.nowPlayingInfo = info
    }

    // MARK: - Private

    private func loadArtwork(imageId: String?, appHandle: AppHandle, into info: inout [String: Any]) {
        guard let imageId else {
            cachedArtworkImageId = nil
            cachedArtwork = nil
            info.removeValue(forKey: MPMediaItemPropertyArtwork)
            return
        }

        // Reuse cached artwork if the image hasn't changed
        if imageId == cachedArtworkImageId, let artwork = cachedArtwork {
            info[MPMediaItemPropertyArtwork] = artwork
            return
        }

        guard let urlString = appHandle.getImageUrl(imageId: imageId),
              let url = URL(string: urlString),
              let image = NSImage(contentsOf: url)
        else {
            cachedArtworkImageId = nil
            cachedArtwork = nil
            info.removeValue(forKey: MPMediaItemPropertyArtwork)
            return
        }

        let artwork = MPMediaItemArtwork(boundsSize: image.size) { _ in image }
        cachedArtworkImageId = imageId
        cachedArtwork = artwork
        info[MPMediaItemPropertyArtwork] = artwork
    }
}
