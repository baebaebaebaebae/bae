import SwiftUI

struct LibraryView: View {
    let appService: AppService

    @State private var artists: [BridgeArtist] = []
    @State private var albums: [BridgeAlbum] = []
    @State private var selection: ArtistSelection = .all
    @State private var selectedAlbumId: String?
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
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
                        appService: appService,
                        selectedAlbumId: $selectedAlbumId
                    )
                    .navigationTitle(navigationTitle)
                }
            } detail: {
                if let selectedAlbumId {
                    AlbumDetailView(
                        albumId: selectedAlbumId,
                        appService: appService
                    )
                } else {
                    ContentUnavailableView(
                        "Select an album",
                        systemImage: "square.stack",
                        description: Text("Choose an album to see its details")
                    )
                }
            }

            if appService.isActive {
                Divider()
                NowPlayingBar(appService: appService)
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
        .onKeyPress(.space) {
            appService.togglePlayPause()
            return .handled
        }
        .onKeyPress(.rightArrow, modifiers: .command) {
            appService.nextTrack()
            return .handled
        }
        .onKeyPress(.leftArrow, modifiers: .command) {
            appService.previousTrack()
            return .handled
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
            artists = try appService.appHandle.getArtists()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func loadAlbums() {
        do {
            switch selection {
            case .all:
                albums = try appService.appHandle.getAlbums()
            case .artist(let artistId):
                albums = try appService.appHandle.getArtistAlbums(artistId: artistId)
            }
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
