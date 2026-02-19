import SwiftUI

struct SearchView: View {
    let appService: AppService
    let onSelectArtist: (String) -> Void
    let onSelectAlbum: (String) -> Void

    var body: some View {
        if let results = appService.searchResults {
            if results.artists.isEmpty && results.albums.isEmpty && results.tracks.isEmpty {
                ContentUnavailableView.search(text: appService.searchQuery)
            } else {
                searchResultsList(results)
            }
        }
    }

    private func searchResultsList(_ results: BridgeSearchResults) -> some View {
        List {
            if !results.artists.isEmpty {
                Section("Artists") {
                    ForEach(results.artists, id: \.id) { artist in
                        artistRow(artist)
                    }
                }
            }

            if !results.albums.isEmpty {
                Section("Albums") {
                    ForEach(results.albums, id: \.id) { album in
                        albumRow(album)
                    }
                }
            }

            if !results.tracks.isEmpty {
                Section("Tracks") {
                    ForEach(results.tracks, id: \.id) { track in
                        trackRow(track)
                    }
                }
            }
        }
    }

    private func artistRow(_ artist: BridgeArtistSearchResult) -> some View {
        Button(action: { onSelectArtist(artist.id) }) {
            HStack(spacing: 12) {
                Image(systemName: "person")
                    .frame(width: 32, height: 32)
                    .foregroundStyle(.secondary)

                VStack(alignment: .leading, spacing: 2) {
                    Text(artist.name)
                        .font(.body)
                        .lineLimit(1)
                    Text("\(artist.albumCount) \(artist.albumCount == 1 ? "album" : "albums")")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func albumRow(_ album: BridgeAlbumSearchResult) -> some View {
        Button(action: { onSelectAlbum(album.id) }) {
            HStack(spacing: 12) {
                albumArt(album)
                    .frame(width: 32, height: 32)
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                VStack(alignment: .leading, spacing: 2) {
                    Text(album.title)
                        .font(.body)
                        .lineLimit(1)

                    HStack(spacing: 4) {
                        Text(album.artistName)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)

                        if let year = album.year {
                            Text("(\(String(year)))")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }
        }
        .buttonStyle(.plain)
    }

    private func trackRow(_ track: BridgeTrackSearchResult) -> some View {
        Button(action: {
            appService.playTracks(trackIds: [track.id])
        }) {
            HStack(spacing: 12) {
                Image(systemName: "waveform")
                    .frame(width: 32, height: 32)
                    .foregroundStyle(.secondary)

                VStack(alignment: .leading, spacing: 2) {
                    Text(track.title)
                        .font(.body)
                        .lineLimit(1)

                    Text("\(track.artistName) - \(track.albumTitle)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer()

                if let durationMs = track.durationMs {
                    Text(formatDuration(durationMs))
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func albumArt(_ album: BridgeAlbumSearchResult) -> some View {
        if let coverReleaseId = album.coverReleaseId,
           let urlString = appService.appHandle.getImageUrl(imageId: coverReleaseId),
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
                    Color(.separatorColor)
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Color(.separatorColor)
            Image(systemName: "photo")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    private func formatDuration(_ ms: Int64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return "\(minutes):\(String(format: "%02d", seconds))"
    }
}
