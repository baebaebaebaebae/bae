import SwiftUI

struct AlbumDetailView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let album: Album
    @State private var detail: AlbumDetail?
    @State private var isLoading = true

    var body: some View {
        Group {
            if isLoading {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    Section {
                        VStack(alignment: .leading, spacing: 8) {
                            if let imageService {
                                AsyncAlbumArt(
                                    imageId: album.coverReleaseId, imageService: imageService,
                                    size: 200
                                )
                                .frame(maxWidth: .infinity)
                            } else {
                                RoundedRectangle(cornerRadius: 8)
                                    .fill(Color(.systemGray5))
                                    .aspectRatio(1, contentMode: .fit)
                                    .frame(maxWidth: 200)
                                    .overlay {
                                        Image(systemName: "photo")
                                            .font(.largeTitle)
                                            .foregroundStyle(.secondary)
                                    }
                                    .frame(maxWidth: .infinity)
                            }
                            Text(album.title)
                                .font(.title2)
                                .fontWeight(.bold)
                            Text(album.artistNames)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            if let year = album.year {
                                Text(String(year))
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .listRowInsets(EdgeInsets())
                        .padding()
                    }
                    if let detail {
                        ForEach(detail.releases) { release in
                            Section(release.releaseName ?? "Tracks") {
                                ForEach(release.tracks) { track in
                                    trackRow(track: track, allTracks: release.tracks)
                                }
                            }
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle(album.title)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            detail = try? databaseService.albumDetail(albumId: album.id)
            isLoading = false
        }
    }

    @ViewBuilder
    private func trackRow(track: Track, allTracks: [Track]) -> some View {
        let isCurrentTrack = playbackService?.currentTrack?.id == track.id

        Button {
            if let playbackService {
                Task {
                    await playbackService.play(
                        track: track, albumArtId: album.coverReleaseId, allTracks: allTracks)
                }
            }
        } label: {
            HStack {
                if let num = track.trackNumber {
                    Text("\(num)")
                        .font(.subheadline)
                        .foregroundStyle(isCurrentTrack ? Color.accentColor : .secondary)
                        .frame(width: 28, alignment: .trailing)
                }
                VStack(alignment: .leading) {
                    Text(track.title)
                        .foregroundStyle(isCurrentTrack ? Color.accentColor : .primary)
                    if let artists = track.artistNames {
                        Text(artists)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                if isCurrentTrack, let playbackService {
                    if playbackService.isLoading {
                        ProgressView()
                            .controlSize(.small)
                    } else if playbackService.isPlaying {
                        Image(systemName: "speaker.wave.2.fill")
                            .font(.caption)
                            .foregroundStyle(Color.accentColor)
                    }
                }
                if let ms = track.durationMs {
                    Text(formatDuration(ms))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func formatDuration(_ ms: Int) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return String(format: "%d:%02d", minutes, seconds)
    }
}
