import SwiftUI

struct MiniPlayerView: View {
    let playbackService: PlaybackService
    let imageService: ImageService?
    @State private var showNowPlaying = false

    var body: some View {
        if let track = playbackService.currentTrack {
            VStack(spacing: 0) {
                // Progress bar
                GeometryReader { geo in
                    Rectangle()
                        .fill(Color.accentColor)
                        .frame(
                            width: playbackService.duration > 0
                                ? geo.size.width
                                    * (playbackService.progress / playbackService.duration) : 0
                        )
                }
                .frame(height: 2)
                HStack(spacing: 12) {
                    // Album art
                    if let imageService {
                        AsyncAlbumArt(
                            imageId: playbackService.currentAlbumArtId,
                            imageService: imageService, size: 40)
                    }
                    // Track info
                    VStack(alignment: .leading, spacing: 2) {
                        Text(track.title)
                            .font(.subheadline)
                            .fontWeight(.medium)
                            .lineLimit(1)
                        if let artists = track.artistNames {
                            Text(artists)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    Spacer()
                    // Loading indicator or play/pause
                    if playbackService.isLoading {
                        ProgressView()
                    } else {
                        Button {
                            playbackService.togglePlayPause()
                        } label: {
                            Image(
                                systemName: playbackService.isPlaying
                                    ? "pause.fill" : "play.fill"
                            )
                            .font(.title3)
                        }
                    }
                }
                .padding(.horizontal)
                .padding(.vertical, 8)
            }
            .background(.ultraThinMaterial)
            .contentShape(Rectangle())
            .onTapGesture {
                showNowPlaying = true
            }
            .sheet(isPresented: $showNowPlaying) {
                NowPlayingView(
                    playbackService: playbackService,
                    imageService: imageService
                )
            }
        }
    }
}
