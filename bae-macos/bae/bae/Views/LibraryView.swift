import SwiftUI

struct LibraryView: View {
    let appHandle: AppHandle

    @State private var artists: [BridgeArtist] = []
    @State private var albums: [BridgeAlbum] = []
    @State private var selection: ArtistSelection = .all
    @State private var selectedAlbumId: String?
    @State private var error: String?

    var body: some View {
        NavigationSplitView {
            ArtistSidebarView(
                artists: artists,
                selection: $selection
            )
            .navigationTitle("Artists")
        } content: {
            if let error {
                ContentUnavailableView(
                    "Failed to load library",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else if albums.isEmpty {
                ContentUnavailableView(
                    "No albums",
                    systemImage: "square.stack",
                    description: Text("Import some music to get started")
                )
            } else {
                AlbumGridView(
                    albums: albums,
                    appHandle: appHandle,
                    selectedAlbumId: $selectedAlbumId
                )
                .navigationTitle(navigationTitle)
            }
        } detail: {
            if let selectedAlbumId {
                AlbumDetailView(
                    albumId: selectedAlbumId,
                    appHandle: appHandle
                )
            } else {
                ContentUnavailableView(
                    "Select an album",
                    systemImage: "square.stack",
                    description: Text("Choose an album to see its details")
                )
            }
        }
        .task {
            loadArtists()
            loadAlbums()
        }
        .onChange(of: selection) { _, _ in
            selectedAlbumId = nil
            loadAlbums()
        }
    }

    private var navigationTitle: String {
        switch selection {
        case .all:
            return "All Albums"
        case .artist(let artistId):
            if let artist = artists.first(where: { $0.id == artistId }) {
                return artist.name
            }
            return "Albums"
        }
    }

    private func loadArtists() {
        do {
            artists = try appHandle.getArtists()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func loadAlbums() {
        do {
            switch selection {
            case .all:
                albums = try appHandle.getAlbums()
            case .artist(let artistId):
                albums = try appHandle.getArtistAlbums(artistId: artistId)
            }
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
