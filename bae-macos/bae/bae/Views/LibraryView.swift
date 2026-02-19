import SwiftUI

struct LibraryView: View {
    let appService: AppService
    @Binding var searchText: String
    @Binding var showQueue: Bool

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
                    results: appService.searchResults,
                    searchQuery: appService.searchQuery,
                    resolveImageURL: { appService.imageURL(for: $0) },
                    onSelectArtist: { _ in },
                    onSelectAlbum: { albumId in
                        searchText = ""
                        appService.search(query: "")
                        selectedAlbumId = albumId
                    },
                    onPlayTrack: { appService.playTracks(trackIds: [$0]) }
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
                HStack(spacing: 0) {
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
                        onPlayAlbum: { appService.playAlbum(albumId: $0) }
                    )
                    .frame(maxWidth: .infinity)
                    if showQueue {
                        Divider()
                        QueueView(
                            isActive: appService.isActive,
                            nowPlayingTitle: appService.trackTitle,
                            nowPlayingArtist: appService.artistNames,
                            nowPlayingArtURL: appService.imageURL(for: appService.coverImageId),
                            items: appService.queueItems.map { item in
                                QueueItemViewModel(
                                    id: item.trackId,
                                    title: item.title,
                                    artistNames: item.artistNames,
                                    albumTitle: item.albumTitle,
                                    durationMs: item.durationMs,
                                    coverArtURL: appService.imageURL(for: item.coverImageId)
                                )
                            },
                            onClose: { showQueue = false },
                            onClear: { appService.clearQueue() },
                            onSkipTo: { appService.skipToQueueIndex(index: UInt32($0)) },
                            onRemove: { appService.removeFromQueue(index: UInt32($0)) },
                            onReorder: { from, to in
                                appService.reorderQueue(fromIndex: UInt32(from), toIndex: UInt32(to))
                            }
                        )
                        .frame(width: 450)
                    } else if let albumId = selectedAlbumId {
                        Divider()
                        AlbumDetailView(
                            albumId: albumId,
                            appService: appService,
                            onClose: { selectedAlbumId = nil }
                        )
                        .frame(width: 450)
                    }
                }
            }
        }
        .animation(nil, value: selectedAlbumId)
        .onChange(of: selectedAlbumId) { _, newValue in
            if newValue != nil {
                showQueue = false
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
