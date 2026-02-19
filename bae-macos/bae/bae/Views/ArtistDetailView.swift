import SwiftUI

struct ArtistDetailView: View {
    let artist: BridgeArtist
    let appService: AppService
    @Binding var selectedAlbumId: String?

    @State private var albums: [BridgeAlbum] = []
    @State private var error: String?
    @State private var sortField: LibrarySortField = .title
    @State private var sortDirection: SortDirection = .ascending

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
                    albums: albums.map { album in
                        AlbumCardViewModel(
                            id: album.id,
                            title: album.title,
                            artistNames: album.artistNames,
                            year: album.year,
                            coverArtURL: appService.imageURL(for: album.coverReleaseId)
                        )
                    },
                    selectedAlbumId: $selectedAlbumId,
                    sortField: $sortField,
                    sortDirection: $sortDirection,
                    onPlayAlbum: { appService.playAlbum(albumId: $0) },
                    onAddToQueue: { albumId in
                        Task.detached {
                            if let detail = try? appService.appHandle.getAlbumDetail(albumId: albumId) {
                                let trackIds = detail.releases.first?.tracks.map(\.id) ?? []
                                if !trackIds.isEmpty {
                                    await MainActor.run { appService.addToQueue(trackIds: trackIds) }
                                }
                            }
                        }
                    },
                    onAddNext: { albumId in
                        Task.detached {
                            if let detail = try? appService.appHandle.getAlbumDetail(albumId: albumId) {
                                let trackIds = detail.releases.first?.tracks.map(\.id) ?? []
                                if !trackIds.isEmpty {
                                    await MainActor.run { appService.addNext(trackIds: trackIds) }
                                }
                            }
                        }
                    }
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
