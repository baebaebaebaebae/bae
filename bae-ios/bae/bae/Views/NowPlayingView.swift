import SwiftUI

struct NowPlayingView: View {
    let playbackService: PlaybackService
    let imageService: ImageService?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                Spacer()
                albumArt
                trackInfo
                progressSection
                transportControls
                Spacer()
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Image(systemName: "chevron.down")
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var albumArt: some View {
        if let imageService {
            AsyncAlbumArt(
                imageId: playbackService.currentAlbumArtId,
                imageService: imageService, size: 280
            )
            .task(id: playbackService.currentAlbumArtId) {
                guard let artId = playbackService.currentAlbumArtId else { return }
                if let image = await imageService.image(for: artId) {
                    playbackService.updateNowPlayingArtwork(image)
                }
            }
        } else {
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(.systemGray5))
                .frame(width: 280, height: 280)
                .overlay {
                    Image(systemName: "photo")
                        .font(.system(size: 48))
                        .foregroundStyle(.secondary)
                }
        }
    }

    private var trackInfo: some View {
        VStack(spacing: 4) {
            Text(playbackService.currentTrack?.title ?? "")
                .font(.title3)
                .fontWeight(.bold)
                .lineLimit(1)
            Text(playbackService.currentTrack?.artistNames ?? "")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal)
    }

    private var progressSection: some View {
        VStack(spacing: 4) {
            Slider(
                value: Binding(
                    get: { playbackService.progress },
                    set: { playbackService.seek(to: $0) }
                ),
                in: 0...max(playbackService.duration, 1)
            )
            HStack {
                Text(formatTime(playbackService.progress))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                Spacer()
                Text("-" + formatTime(max(0, playbackService.duration - playbackService.progress)))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 24)
    }

    private var transportControls: some View {
        HStack(spacing: 40) {
            Button {
                Task { await playbackService.playPrevious() }
            } label: {
                Image(systemName: "backward.fill")
                    .font(.title2)
            }
            Button {
                playbackService.togglePlayPause()
            } label: {
                Image(systemName: playbackService.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                    .font(.system(size: 56))
            }
            Button {
                Task { await playbackService.playNext() }
            } label: {
                Image(systemName: "forward.fill")
                    .font(.title2)
            }
        }
    }

    private func formatTime(_ seconds: TimeInterval) -> String {
        let mins = Int(seconds) / 60
        let secs = Int(seconds) % 60
        return String(format: "%d:%02d", mins, secs)
    }
}
