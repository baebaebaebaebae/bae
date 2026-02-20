import SwiftUI

struct NowPlayingBar: View {
    let trackTitle: String?
    let artistNames: String?
    let coverArtURL: URL?
    let isPlaying: Bool
    let currentPositionMs: UInt64
    let currentDurationMs: UInt64
    let volume: Float
    let repeatMode: BridgeRepeatMode
    @Binding var showQueue: Bool
    let queueIsActive: Bool
    let queueNowPlayingTitle: String?
    let queueNowPlayingArtist: String?
    let queueNowPlayingArtURL: URL?
    let queueItems: [QueueItemViewModel]
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (UInt64) -> Void
    let onVolumeChange: (Float) -> Void
    let onCycleRepeat: () -> Void
    let onQueueClear: () -> Void
    let onQueueSkipTo: (Int) -> Void
    let onQueueRemove: (Int) -> Void
    let onQueueReorder: (Int, Int) -> Void

    @State private var isSeeking = false
    @State private var seekPosition: Double = 0

    var body: some View {
        HStack(spacing: 16) {
            trackInfo
                .frame(width: 220, alignment: .leading)

            Spacer()

            transportControls

            Spacer()

            trailingControls
                .frame(width: 180, alignment: .trailing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Theme.surface)
    }

    // MARK: - Left: track info

    private var trackInfo: some View {
        HStack(spacing: 12) {
            albumArt
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel("Album art")

            VStack(alignment: .leading, spacing: 2) {
                if let title = trackTitle {
                    Text(title)
                        .font(.callout)
                        .fontWeight(.medium)
                        .lineLimit(1)
                }

                if let artist = artistNames {
                    Text(artist)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }

    @ViewBuilder
    private var albumArt: some View {
        if let url = coverArtURL {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                case .failure:
                    albumArtPlaceholder
                default:
                    Theme.placeholder
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Theme.placeholder
            Image(systemName: "photo")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Center: transport controls + progress

    private var transportControls: some View {
        VStack(spacing: 4) {
            HStack(spacing: 20) {
                Button(action: onPrevious) {
                    Image(systemName: "backward.fill")
                        .font(.body)
                }
                .buttonStyle(.plain)
                .help("Previous track")
                .accessibilityLabel("Previous track")

                Button(action: onPlayPause) {
                    Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                        .font(.title2)
                }
                .buttonStyle(.plain)
                .help(isPlaying ? "Pause" : "Play")
                .accessibilityLabel(isPlaying ? "Pause" : "Play")

                Button(action: onNext) {
                    Image(systemName: "forward.fill")
                        .font(.body)
                }
                .buttonStyle(.plain)
                .help("Next track")
                .accessibilityLabel("Next track")
            }

            progressBar
        }
    }

    private var progressBar: some View {
        HStack(spacing: 8) {
            Text(formatTime(currentMs))
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 40, alignment: .trailing)

            Slider(
                value: Binding(
                    get: {
                        isSeeking ? seekPosition : Double(currentPositionMs)
                    },
                    set: { newValue in
                        isSeeking = true
                        seekPosition = newValue
                    }
                ),
                in: 0...max(Double(currentDurationMs), 1),
                onEditingChanged: { editing in
                    if !editing {
                        onSeek(UInt64(seekPosition))
                        isSeeking = false
                    }
                }
            )
            .frame(width: 300)
            .accessibilityLabel("Playback position")

            Text(formatTime(currentDurationMs))
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 40, alignment: .leading)
        }
    }

    private var currentMs: UInt64 {
        isSeeking ? UInt64(seekPosition) : currentPositionMs
    }

    // MARK: - Right: volume + repeat

    private var trailingControls: some View {
        HStack(spacing: 12) {
            repeatButton

            Button(action: { showQueue.toggle() }) {
                Image(systemName: "list.bullet")
                    .foregroundColor(showQueue ? .accentColor : .secondary)
            }
            .buttonStyle(.plain)
            .font(.caption)
            .help("Queue")
            .accessibilityLabel("Queue")
            .popover(isPresented: $showQueue, arrowEdge: .top) {
                QueueView(
                    isActive: queueIsActive,
                    nowPlayingTitle: queueNowPlayingTitle,
                    nowPlayingArtist: queueNowPlayingArtist,
                    nowPlayingArtURL: queueNowPlayingArtURL,
                    items: queueItems,
                    onClose: { showQueue = false },
                    onClear: onQueueClear,
                    onSkipTo: onQueueSkipTo,
                    onRemove: onQueueRemove,
                    onReorder: onQueueReorder
                )
                .frame(width: 350, height: 500)
            }

            Image(systemName: "speaker.fill")
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            Slider(
                value: Binding(
                    get: { volume },
                    set: { onVolumeChange($0) }
                ),
                in: 0...1
            )
            .frame(width: 80)
            .accessibilityLabel("Volume")
        }
    }

    private var repeatButton: some View {
        Button(action: onCycleRepeat) {
            switch repeatMode {
            case .none:
                Image(systemName: "repeat")
                    .foregroundStyle(.secondary)
            case .album:
                Image(systemName: "repeat")
                    .foregroundColor(.accentColor)
            case .track:
                Image(systemName: "repeat.1")
                    .foregroundColor(.accentColor)
            }
        }
        .buttonStyle(.plain)
        .font(.caption)
        .help(repeatHelp)
        .accessibilityLabel(repeatHelp)
    }

    private var repeatHelp: String {
        switch repeatMode {
        case .none:
            return "Repeat: off"
        case .album:
            return "Repeat: album"
        case .track:
            return "Repeat: track"
        }
    }

    // MARK: - Formatting

    private func formatTime(_ ms: UInt64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return "\(minutes):\(String(format: "%02d", seconds))"
    }
}

// MARK: - Previews

#Preview("Playing") {
    NowPlayingBar(
        trackTitle: PreviewData.nowPlayingTitle,
        artistNames: PreviewData.nowPlayingArtist,
        coverArtURL: PreviewData.nowPlayingCoverURL,
        isPlaying: true,
        currentPositionMs: 63_000,
        currentDurationMs: 210_000,
        volume: 0.75,
        repeatMode: .none,
        showQueue: .constant(false),
        queueIsActive: true,
        queueNowPlayingTitle: PreviewData.nowPlayingTitle,
        queueNowPlayingArtist: PreviewData.nowPlayingArtist,
        queueNowPlayingArtURL: PreviewData.nowPlayingCoverURL,
        queueItems: PreviewData.queueItems,
        onPlayPause: {},
        onNext: {},
        onPrevious: {},
        onSeek: { _ in },
        onVolumeChange: { _ in },
        onCycleRepeat: {},
        onQueueClear: {},
        onQueueSkipTo: { _ in },
        onQueueRemove: { _ in },
        onQueueReorder: { _, _ in }
    )
}

#Preview("Paused + Repeat Album") {
    NowPlayingBar(
        trackTitle: "Tide Pool",
        artistNames: "Glass Harbor",
        coverArtURL: PreviewData.nowPlayingCoverURL,
        isPlaying: false,
        currentPositionMs: 120_000,
        currentDurationMs: 198_000,
        volume: 0.5,
        repeatMode: .album,
        showQueue: .constant(false),
        queueIsActive: true,
        queueNowPlayingTitle: "Tide Pool",
        queueNowPlayingArtist: "Glass Harbor",
        queueNowPlayingArtURL: PreviewData.nowPlayingCoverURL,
        queueItems: PreviewData.queueItems,
        onPlayPause: {},
        onNext: {},
        onPrevious: {},
        onSeek: { _ in },
        onVolumeChange: { _ in },
        onCycleRepeat: {},
        onQueueClear: {},
        onQueueSkipTo: { _ in },
        onQueueRemove: { _ in },
        onQueueReorder: { _, _ in }
    )
}
