import SwiftUI

struct ArtistListView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let syncService: SyncService?
    @State private var artists: [Artist] = []
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Group {
                if let error {
                    ContentUnavailableView {
                        Label("Error", systemImage: "exclamationmark.triangle")
                    } description: {
                        Text(error)
                    }
                } else if artists.isEmpty {
                    ContentUnavailableView {
                        Label("No Artists", systemImage: "books.vertical")
                    } description: {
                        Text("Your library is empty.")
                    }
                } else {
                    List(artists) { artist in
                        NavigationLink(value: artist) {
                            Text(artist.name)
                        }
                    }
                    .navigationDestination(for: Artist.self) { artist in
                        ArtistAlbumsView(
                            databaseService: databaseService, imageService: imageService,
                            playbackService: playbackService, artist: artist)
                    }
                    .refreshable {
                        await syncService?.sync()
                        try? databaseService.reopen()
                        artists = (try? databaseService.allArtists()) ?? []
                    }
                }
            }
            .navigationTitle("Artists")
            .task {
                do {
                    artists = try databaseService.allArtists()
                } catch {
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
