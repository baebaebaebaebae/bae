import SwiftUI
import UniformTypeIdentifiers

struct LibraryView: View {
    let appService: AppService

    @State private var artists: [BridgeArtist] = []
    @State private var albums: [BridgeAlbum] = []
    @State private var selection: ArtistSelection = .all
    @State private var selectedAlbumId: String?
    @State private var error: String?
    @State private var showingImport = false
    @State private var showingSettings = false
    @State private var showingSyncSettings = false
    @State private var searchText: String = ""
    @State private var searchDebounceTask: Task<Void, Never>?

    private var isSearching: Bool {
        !searchText.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            NavigationSplitView {
                ArtistSidebarView(
                    artists: artists,
                    selection: $selection
                )
                .navigationTitle("Artists")
            } content: {
                if isSearching {
                    SearchView(
                        appService: appService,
                        onSelectArtist: { artistId in
                            searchText = ""
                            appService.search(query: "")
                            selection = .artist(artistId)
                        },
                        onSelectAlbum: { albumId in
                            searchText = ""
                            appService.search(query: "")
                            selectedAlbumId = albumId
                        }
                    )
                    .navigationTitle("Search")
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
            .searchable(text: $searchText, prompt: "Artists, albums, tracks")
            .onChange(of: searchText) { _, newValue in
                searchDebounceTask?.cancel()
                searchDebounceTask = Task {
                    try? await Task.sleep(for: .milliseconds(300))
                    guard !Task.isCancelled else { return }
                    appService.search(query: newValue)
                }
            }
            .toolbar {
                ToolbarItemGroup {
                    Button(action: { openFolderAndScan() }) {
                        Label("Import", systemImage: "plus")
                    }
                    .help("Import a folder of music")
                    .accessibilityLabel("Import folder")

                    Button(action: { showingSyncSettings = true }) {
                        Image(systemName: "arrow.triangle.2.circlepath")
                    }
                    .help("Sync Settings")

                    Button(action: { showingSettings = true }) {
                        Label("Settings", systemImage: "gearshape")
                    }
                    .help("Open settings")
                    .accessibilityLabel("Settings")
                }
            }

            if appService.isActive {
                Divider()
                NowPlayingBar(appService: appService)
            }
        }
        .focusedSceneValue(\.appService, appService)
        .task {
            loadArtists()
            loadAlbums()
        }
        .onChange(of: selection) { _, _ in
            selectedAlbumId = nil
            loadAlbums()
        }
        .onChange(of: appService.libraryVersion) { _, _ in
            loadArtists()
            loadAlbums()
        }
        .sheet(isPresented: $showingImport) {
            ImportView(
                appService: appService,
                isPresented: $showingImport
            )
        }
        .sheet(isPresented: $showingSettings) {
            SettingsView(appService: appService)
        }
        .sheet(isPresented: $showingSyncSettings) {
            NavigationStack {
                SyncSettingsView(appService: appService)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") {
                                showingSyncSettings = false
                            }
                        }
                    }
            }
            .frame(minWidth: 500, minHeight: 400)
        }
        .onKeyPress(.space) {
            appService.togglePlayPause()
            return .handled
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDrop(providers)
        }
        .onReceive(NotificationCenter.default.publisher(for: .importFolder)) { _ in
            openFolderAndScan()
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

    private func openFolderAndScan() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder containing music to import"
        panel.prompt = "Scan"

        guard panel.runModal() == .OK, let url = panel.url else { return }

        appService.scanFolder(path: url.path)
        showingImport = true
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

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        guard provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else {
            return false
        }

        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data,
                  let url = URL(dataRepresentation: data, relativeTo: nil) else {
                return
            }
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir),
                  isDir.boolValue else {
                return
            }
            DispatchQueue.main.async {
                appService.scanFolder(path: url.path)
                showingImport = true
            }
        }
        return true
    }
}
