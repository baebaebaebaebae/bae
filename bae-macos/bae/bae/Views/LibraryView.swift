import SwiftUI

struct LibraryView: View {
    let appService: AppService
    @Binding var searchText: String

    @State private var albums: [BridgeAlbum] = []
    @State private var selectedAlbumId: String?
    @State private var error: String?
    @State private var searchDebounceTask: Task<Void, Never>?

    private var isSearching: Bool {
        !searchText.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        Group {
            if isSearching {
                SearchView(
                    appService: appService,
                    onSelectArtist: { _ in },
                    onSelectAlbum: { albumId in
                        searchText = ""
                        appService.search(query: "")
                        selectedAlbumId = albumId
                    }
                )
            } else if let error {
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
            } else if let albumId = selectedAlbumId {
                VSplitView {
                    AlbumGridView(
                        albums: albums,
                        appService: appService,
                        selectedAlbumId: $selectedAlbumId
                    )
                    .frame(minHeight: 120)
                    AlbumDetailView(
                        albumId: albumId,
                        appService: appService,
                        onClose: { selectedAlbumId = nil }
                    )
                    .frame(minHeight: 200)
                }
            } else {
                AlbumGridView(
                    albums: albums,
                    appService: appService,
                    selectedAlbumId: $selectedAlbumId
                )
            }
        }
        .onChange(of: searchText) { _, newValue in
            searchDebounceTask?.cancel()
            searchDebounceTask = Task {
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled else { return }
                appService.search(query: newValue)
            }
        }
        .task { loadAlbums() }
        .onChange(of: appService.libraryVersion) { _, _ in loadAlbums() }
    }

    private func loadAlbums() {
        do {
            albums = try appService.appHandle.getAlbums()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
