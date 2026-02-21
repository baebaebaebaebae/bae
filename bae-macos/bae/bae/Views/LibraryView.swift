import SwiftUI

enum LibrarySortField: String, CaseIterable {
    case title = "Title"
    case artist = "Artist"
    case year = "Year"
    case dateAdded = "Date Added"

    var bridgeField: BridgeSortField {
        switch self {
        case .title: .title
        case .artist: .artist
        case .year: .year
        case .dateAdded: .dateAdded
        }
    }
}

enum SortDirection {
    case ascending
    case descending

    var bridgeDirection: BridgeSortDirection {
        switch self {
        case .ascending: .ascending
        case .descending: .descending
        }
    }
}

struct LibraryView: View {
    let appService: AppService
    @Binding var selectedAlbumId: String?

    @State private var albums: [BridgeAlbum] = []
    @State private var error: String?
    @State private var sortField: LibrarySortField = .dateAdded
    @State private var sortDirection: SortDirection = .descending

    private var sortCriteria: [BridgeSortCriterion] {
        [BridgeSortCriterion(field: sortField.bridgeField, direction: sortDirection.bridgeDirection)]
    }

    var body: some View {
        Group {
            if let error {
                ContentUnavailableView(
                    "Failed to load library",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error),
                )
            } else if albums.isEmpty {
                ContentUnavailableView(
                    "No albums",
                    systemImage: "square.stack",
                    description: Text("Import some music to get started"),
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
                                coverArtURL: appService.imageURL(for: album.coverReleaseId),
                            )
                        },
                        selectedAlbumId: $selectedAlbumId,
                        sortField: $sortField,
                        sortDirection: $sortDirection,
                        onPlayAlbum: { appService.playAlbum(albumId: $0) },
                        onAddToQueue: { albumId in addAlbumToQueue(albumId: albumId) },
                        onAddNext: { albumId in addAlbumNext(albumId: albumId) },
                    )
                    .frame(maxWidth: 1200)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 16)
                    if let albumId = selectedAlbumId {
                        Divider()
                        AlbumDetailView(
                            albumId: albumId,
                            appService: appService,
                            onClose: { selectedAlbumId = nil },
                        )
                        .frame(width: 450)
                    }
                }
            }
        }
        .animation(nil, value: selectedAlbumId)
        .task { loadAlbums() }
        .onChange(of: appService.libraryVersion) { _, _ in loadAlbums() }
        .onChange(of: sortField) { _, _ in loadAlbums() }
        .onChange(of: sortDirection) { _, _ in loadAlbums() }
    }

    private func loadAlbums() {
        do {
            albums = try appService.appHandle.getAlbums(sortCriteria: sortCriteria)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func addAlbumToQueue(albumId: String) {
        Task.detached {
            if let detail = try? appService.appHandle.getAlbumDetail(albumId: albumId) {
                let trackIds = detail.releases.first?.tracks.map(\.id) ?? []
                if !trackIds.isEmpty {
                    await MainActor.run {
                        appService.addToQueue(trackIds: trackIds)
                    }
                }
            }
        }
    }

    private func addAlbumNext(albumId: String) {
        Task.detached {
            if let detail = try? appService.appHandle.getAlbumDetail(albumId: albumId) {
                let trackIds = detail.releases.first?.tracks.map(\.id) ?? []
                if !trackIds.isEmpty {
                    await MainActor.run {
                        appService.addNext(trackIds: trackIds)
                    }
                }
            }
        }
    }
}
