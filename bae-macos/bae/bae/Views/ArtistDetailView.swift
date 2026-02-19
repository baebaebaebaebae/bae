import SwiftUI

struct ArtistDetailView: View {
    let artist: BridgeArtist
    let appService: AppService
    @Binding var selectedAlbumId: String?

    @State private var albums: [BridgeAlbum] = []
    @State private var error: String?

    var body: some View {
        Group {
            if let error {
                ContentUnavailableView(
                    "Failed to load artist",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else if albums.isEmpty {
                ContentUnavailableView(
                    "No albums",
                    systemImage: "square.stack",
                    description: Text("No albums found for this artist")
                )
            } else {
                AlbumGridView(
                    albums: albums,
                    appService: appService,
                    selectedAlbumId: $selectedAlbumId
                )
            }
        }
        .navigationTitle(artist.name)
        .task(id: artist.id) {
            loadAlbums()
        }
    }

    private func loadAlbums() {
        do {
            albums = try appService.appHandle.getArtistAlbums(artistId: artist.id)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
