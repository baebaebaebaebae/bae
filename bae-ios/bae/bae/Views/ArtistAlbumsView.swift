import SwiftUI

struct ArtistAlbumsView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let artist: Artist
    @State private var albums: [Album] = []

    var body: some View {
        List(albums) { album in
            NavigationLink(value: album) {
                HStack(spacing: 12) {
                    if let imageService {
                        AsyncAlbumArt(
                            imageId: album.coverReleaseId, imageService: imageService, size: 48)
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
        .navigationTitle(artist.name)
        .navigationDestination(for: Album.self) { album in
            AlbumDetailView(
                databaseService: databaseService, imageService: imageService,
                playbackService: playbackService, album: album)
        }
        .task {
            albums = (try? databaseService.albumsByArtist(artistId: artist.id)) ?? []
        }
    }
}
