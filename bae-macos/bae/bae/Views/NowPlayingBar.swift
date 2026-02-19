import SwiftUI

struct NowPlayingBar: View {
    let appService: AppService
    @Binding var showQueue: Bool

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
                if let title = appService.trackTitle {
                    Text(title)
                        .font(.callout)
                        .fontWeight(.medium)
                        .lineLimit(1)
                }

                if let artist = appService.artistNames {
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
        if let imageId = appService.coverImageId,
           let urlString = appService.appHandle.getImageUrl(imageId: imageId),
           let url = URL(string: urlString) {
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
                Button(action: { appService.previousTrack() }) {
                    Image(systemName: "backward.fill")
                        .font(.body)
                }
                .buttonStyle(.plain)
                .help("Previous track")
                .accessibilityLabel("Previous track")

                Button(action: { appService.togglePlayPause() }) {
                    Image(systemName: appService.isPlaying ? "pause.fill" : "play.fill")
                        .font(.title2)
                }
                .buttonStyle(.plain)
                .help(appService.isPlaying ? "Pause" : "Play")
                .accessibilityLabel(appService.isPlaying ? "Pause" : "Play")

                Button(action: { appService.nextTrack() }) {
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
                        isSeeking ? seekPosition : Double(appService.currentPositionMs)
                    },
                    set: { newValue in
                        isSeeking = true
                        seekPosition = newValue
                    }
                ),
                in: 0...max(Double(appService.currentDurationMs), 1),
                onEditingChanged: { editing in
                    if !editing {
                        appService.seek(positionMs: UInt64(seekPosition))
                        isSeeking = false
                    }
                }
            )
            .frame(width: 300)
            .accessibilityLabel("Playback position")

            Text(formatTime(appService.currentDurationMs))
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 40, alignment: .leading)
        }
    }

    private var currentMs: UInt64 {
        isSeeking ? UInt64(seekPosition) : appService.currentPositionMs
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

            Image(systemName: "speaker.fill")
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            Slider(
                value: Binding(
                    get: { appService.volume },
                    set: { appService.setVolume($0) }
                ),
                in: 0...1
            )
            .frame(width: 80)
            .accessibilityLabel("Volume")
        }
    }

    private var repeatButton: some View {
        Button(action: { appService.cycleRepeatMode() }) {
            switch appService.repeatMode {
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
        switch appService.repeatMode {
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
