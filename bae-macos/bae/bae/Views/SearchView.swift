import SwiftUI

struct SearchView: View {
    let results: BridgeSearchResults?
    let searchQuery: String
    let resolveImageURL: (String?) -> URL?
    let onSelectArtist: (String) -> Void
    let onSelectAlbum: (String) -> Void
    let onPlayTrack: (String) -> Void

    var body: some View {
        if let results {
            if results.artists.isEmpty && results.albums.isEmpty && results.tracks.isEmpty {
                ContentUnavailableView.search(text: searchQuery)
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
        .scrollContentBackground(.hidden)
        .background(Theme.background)
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
            onPlayTrack(track.id)
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
        if let url = resolveImageURL(album.coverReleaseId) {
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

// MARK: - Previews

#Preview("With results") {
    SearchView(
        results: BridgeSearchResults(
            artists: [
                BridgeArtistSearchResult(id: "ar-1", name: "Glass Harbor", albumCount: 2),
                BridgeArtistSearchResult(id: "ar-2", name: "Velvet Mathematics", albumCount: 2),
            ],
            albums: [
                BridgeAlbumSearchResult(id: "a-02", title: "Pacific Standard", year: 2019, coverReleaseId: nil, artistName: "Glass Harbor"),
                BridgeAlbumSearchResult(id: "a-14", title: "Landlocked", year: 2022, coverReleaseId: nil, artistName: "Glass Harbor"),
                BridgeAlbumSearchResult(id: "a-03", title: "Proof by Induction", year: 2021, coverReleaseId: nil, artistName: "Velvet Mathematics"),
            ],
            tracks: [
                BridgeTrackSearchResult(id: "t-03", title: "Tide Pool", durationMs: 198_000, albumId: "a-02", albumTitle: "Pacific Standard", artistName: "Glass Harbor"),
                BridgeTrackSearchResult(id: "t-05", title: "Axiom", durationMs: 187_000, albumId: "a-03", albumTitle: "Proof by Induction", artistName: "Velvet Mathematics"),
            ]
        ),
        searchQuery: "glass",
        resolveImageURL: { _ in nil },
        onSelectArtist: { _ in },
        onSelectAlbum: { _ in },
        onPlayTrack: { _ in }
    )
    .frame(width: 600, height: 500)
}

#Preview("No results") {
    SearchView(
        results: BridgeSearchResults(artists: [], albums: [], tracks: []),
        searchQuery: "nonexistent",
        resolveImageURL: { _ in nil },
        onSelectArtist: { _ in },
        onSelectAlbum: { _ in },
        onPlayTrack: { _ in }
    )
    .frame(width: 600, height: 400)
}
