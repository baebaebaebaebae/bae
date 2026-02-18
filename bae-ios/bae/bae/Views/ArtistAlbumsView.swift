import SwiftUI

struct ArtistAlbumsView: View {
    let databaseService: DatabaseService
    let artist: Artist
    @State private var albums: [Album] = []

    var body: some View {
        List(albums) { album in
            NavigationLink(value: album) {
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
        .navigationTitle(artist.name)
        .navigationDestination(for: Album.self) { album in
            AlbumDetailView(databaseService: databaseService, album: album)
        }
        .task {
            albums = (try? databaseService.albumsByArtist(artistId: artist.id)) ?? []
        }
    }
}
