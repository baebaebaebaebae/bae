import SwiftUI

struct ArtistAlbumsView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let artist: Artist
    @State private var albums: [Album] = []
    @State private var isLoading = true
    @State private var error: String?

    var body: some View {
        Group {
            if isLoading {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error {
                ContentUnavailableView {
                    Label("Error", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(error)
                }
            } else if albums.isEmpty {
                ContentUnavailableView {
                    Label("No Albums", systemImage: "square.stack")
                } description: {
                    Text("No albums found for this artist.")
                }
            } else {
                List(albums) { album in
                    NavigationLink(value: album) {
                        HStack(spacing: 12) {
                            if let imageService {
                                AsyncAlbumArt(
                                    imageId: album.coverReleaseId, imageService: imageService,
                                    size: 48)
                            }
                            VStack(alignment: .leading) {
                                Text(album.title)
                                    .font(.headline)
                                if let year = album.year {
                                    Text(String(year))
                                        .font(.subheadline)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
                .refreshable {
                    await loadAlbums()
                }
            }
        }
        .navigationTitle(artist.name)
        .navigationDestination(for: Album.self) { album in
            AlbumDetailView(
                databaseService: databaseService, imageService: imageService,
                playbackService: playbackService, album: album)
        }
        .task {
            await loadAlbums()
        }
    }

    private func loadAlbums() async {
        do {
            albums = try databaseService.albumsByArtist(artistId: artist.id)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
        isLoading = false
    }
}
