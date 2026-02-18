import SwiftUI

struct AlbumDetailView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let album: Album
    @State private var detail: AlbumDetail?

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    if let imageService {
                        AsyncAlbumArt(
                            imageId: album.coverReleaseId, imageService: imageService, size: 200
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
                            HStack {
                                if let num = track.trackNumber {
                                    Text("\(num)")
                                        .font(.subheadline)
                                        .foregroundStyle(.secondary)
                                        .frame(width: 28, alignment: .trailing)
                                }
                                VStack(alignment: .leading) {
                                    Text(track.title)
                                    if let artists = track.artistNames {
                                        Text(artists)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if let ms = track.durationMs {
                                    Text(formatDuration(ms))
                                        .font(.subheadline)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
        }
        .listStyle(.plain)
        .navigationTitle(album.title)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            detail = try? databaseService.albumDetail(albumId: album.id)
        }
    }

    private func formatDuration(_ ms: Int) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return String(format: "%d:%02d", minutes, seconds)
    }
}
