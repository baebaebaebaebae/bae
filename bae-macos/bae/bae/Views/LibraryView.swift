import SwiftUI
import UniformTypeIdentifiers

struct LibraryView: View {
    let appService: AppService

    @State private var artists: [BridgeArtist] = []
    @State private var albums: [BridgeAlbum] = []
    @State private var selection: ArtistSelection = .all
    @State private var selectedAlbumId: String?
    @State private var error: String?
    @State private var importFolderURL: URL?
    @State private var showingImportPicker = false
    @State private var showingSettings = false

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
        .focusedSceneValue(\.appService, appService)
        .task {
            loadArtists()
            loadAlbums()
        }
        .onChange(of: selection) { _, _ in
            selectedAlbumId = nil
            loadAlbums()
        }
        .onReceive(NotificationCenter.default.publisher(for: .importFolder)) { _ in
            showingImportPicker = true
        }
        .fileImporter(
            isPresented: $showingImportPicker,
            allowedContentTypes: [.folder],
            allowsMultipleSelection: false
        ) { result in
            if case let .success(urls) = result, let url = urls.first {
                importFolderURL = url
            }
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDrop(providers)
        }
        .sheet(item: $importFolderURL) { url in
            ImportView(folderURL: url, appService: appService) {
                importFolderURL = nil
                loadArtists()
                loadAlbums()
            }
        }
        .sheet(isPresented: $showingSettings) {
            SettingsView(appService: appService)
        }
        .toolbar {
            ToolbarItemGroup {
                Button(action: { showingImportPicker = true }) {
                    Label("Import", systemImage: "plus")
                }
                .help("Import a folder of music")
                .accessibilityLabel("Import folder")

                Button(action: { showingSettings = true }) {
                    Label("Settings", systemImage: "gearshape")
                }
                .help("Open settings")
                .accessibilityLabel("Settings")
            }
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
                importFolderURL = url
            }
        }
        return true
    }
}

// Make URL identifiable for .sheet(item:)
extension URL: @retroactive Identifiable {
    public var id: String { absoluteString }
}
