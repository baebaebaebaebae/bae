import SwiftUI

struct LibraryView: View {
    let appService: AppService

    @State private var albums: [BridgeAlbum] = []
    @State private var selectedAlbumId: String?
    @State private var error: String?
    @State private var searchText: String = ""
    @State private var searchDebounceTask: Task<Void, Never>?

    private var isSearching: Bool {
        !searchText.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        NavigationStack {
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
                } else {
                    AlbumGridView(
                        albums: albums,
                        appService: appService,
                        selectedAlbumId: $selectedAlbumId
                    )
                }
            }
            .navigationTitle("Library")
            .searchable(text: $searchText, prompt: "Artists, albums, tracks")
            .onChange(of: searchText) { _, newValue in
                searchDebounceTask?.cancel()
                searchDebounceTask = Task {
                    try? await Task.sleep(for: .milliseconds(300))
                    guard !Task.isCancelled else { return }
                    appService.search(query: newValue)
                }
            }
        }
        .task { loadAlbums() }
        .onChange(of: appService.libraryVersion) { _, _ in loadAlbums() }
        .sheet(isPresented: Binding(
            get: { selectedAlbumId != nil },
            set: { if !$0 { selectedAlbumId = nil } }
        )) {
            if let albumId = selectedAlbumId {
                NavigationStack {
                    AlbumDetailView(albumId: albumId, appService: appService)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") { selectedAlbumId = nil }
                            }
                        }
                }
                .frame(minWidth: 600, minHeight: 500)
            }
        }
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
