import SwiftUI

struct SearchView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    @State private var searchText = ""
    @State private var artists: [Artist] = []
    @State private var albums: [Album] = []
    @State private var tracks: [Track] = []

    var body: some View {
        NavigationStack {
            List {
                if searchText.isEmpty {
                    ContentUnavailableView(
                        "Search your library",
                        systemImage: "magnifyingglass",
                        description: Text("Search for artists, albums, and tracks")
                    )
                } else if artists.isEmpty && albums.isEmpty && tracks.isEmpty {
                    ContentUnavailableView.search(text: searchText)
                } else {
                    if !artists.isEmpty {
                        Section("Artists") {
                            ForEach(artists) { artist in
                                NavigationLink(value: artist) {
                                    Text(artist.name)
                                }
                            }
                        }
                    }
                    if !albums.isEmpty {
                        Section("Albums") {
                            ForEach(albums) { album in
                                NavigationLink(value: album) {
                                    HStack(spacing: 12) {
                                        if let imageService {
                                            AsyncAlbumArt(
                                                imageId: album.coverReleaseId,
                                                imageService: imageService, size: 44)
                                        }
                                        VStack(alignment: .leading) {
                                            Text(album.title)
                                                .font(.headline)
                                            Text(album.artistNames)
                                                .font(.subheadline)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !tracks.isEmpty {
                        Section("Tracks") {
                            ForEach(tracks) { track in
                                Button {
                                    if let playbackService {
                                        Task {
                                            await playbackService.play(
                                                track: track, albumArtId: nil, allTracks: [track])
                                        }
                                    }
                                } label: {
                                    VStack(alignment: .leading) {
                                        Text(track.title)
                                        if let artists = track.artistNames {
                                            Text(artists)
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Search")
            .searchable(text: $searchText, prompt: "Artists, albums, tracks")
            .onChange(of: searchText) { _, newValue in
                performSearch(query: newValue)
            }
            .navigationDestination(for: Artist.self) { artist in
                ArtistAlbumsView(
                    databaseService: databaseService, imageService: imageService,
                    playbackService: playbackService, artist: artist)
            }
            .navigationDestination(for: Album.self) { album in
                AlbumDetailView(
                    databaseService: databaseService, imageService: imageService,
                    playbackService: playbackService, album: album)
            }
        }
    }

    private func performSearch(query: String) {
        guard !query.isEmpty else {
            artists = []
            albums = []
            tracks = []
            return
        }
        artists = (try? databaseService.searchArtists(query: query)) ?? []
        albums = (try? databaseService.searchAlbums(query: query)) ?? []
        tracks = (try? databaseService.searchTracks(query: query)) ?? []
    }
}
