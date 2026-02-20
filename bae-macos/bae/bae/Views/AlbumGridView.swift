import SwiftUI

struct AlbumCardViewModel: Identifiable {
    let id: String
    let title: String
    let artistNames: String
    let year: Int32?
    let coverArtURL: URL?
}

private let albumCardSize: CGFloat = 180

struct AlbumGridView: View {
    let albums: [AlbumCardViewModel]
    @Binding var selectedAlbumId: String?
    @Binding var sortField: LibrarySortField
    @Binding var sortDirection: SortDirection
    let onPlayAlbum: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNext: (String) -> Void

    private let columns = [
        GridItem(.adaptive(minimum: albumCardSize), spacing: 24, alignment: .topLeading)
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                libraryHeader
                    .padding(.horizontal)
                    .padding(.top, 40)
                    .padding(.bottom, 20)

                LazyVGrid(columns: columns, spacing: 28) {
                    ForEach(albums) { album in
                        AlbumCardView(
                            title: album.title,
                            artistNames: album.artistNames,
                            year: album.year,
                            coverArtURL: album.coverArtURL,
                            isSelected: selectedAlbumId == album.id,
                            onPlay: { onPlayAlbum(album.id) },
                            onAddToQueue: { onAddToQueue(album.id) },
                            onAddNext: { onAddNext(album.id) }
                        )
                        .draggable(album.id)
                        .onTapGesture {
                            selectedAlbumId = album.id
                        }
                    }
                }
                .padding(.horizontal)
                .padding(.bottom)
            }
        }
        .background(Theme.background)
    }

    private var libraryHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("Library")
                .font(.system(size: 36, weight: .bold))

            Spacer()

            sortControls
        }
    }

    private var sortControls: some View {
        HStack(spacing: 8) {
            Menu {
                ForEach(LibrarySortField.allCases, id: \.self) { field in
                    Button {
                        sortField = field
                    } label: {
                        HStack {
                            Text(field.rawValue)
                            if sortField == field {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            } label: {
                Text(sortField.rawValue)
                        .font(.callout)
                .foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()

            Button(action: {
                sortDirection = sortDirection == .ascending ? .descending : .ascending
            }) {
                Image(systemName: sortDirection == .ascending ? "arrow.up" : "arrow.down")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help(sortDirection == .ascending ? "Sort ascending" : "Sort descending")
        }
    }
}

struct AlbumCardView: View {
    let title: String
    let artistNames: String
    let year: Int32?
    let coverArtURL: URL?
    let isSelected: Bool
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void

    @State private var isHovered = false
    @State private var showMenu = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            albumArt
                .aspectRatio(1, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(
                            Color.accentColor,
                            lineWidth: isSelected ? 3 : 0
                        )
                )
                .overlay(alignment: .topTrailing) {
                    if isHovered || showMenu {
                        CardMenuButton(onPlay: onPlay, onAddToQueue: onAddToQueue, onAddNext: onAddNext, showMenu: $showMenu)
                            .padding(6)
                            .transition(.opacity)
                    }
                }
                .onHover { isHovered = $0 }
                .padding(.bottom, 6)

            Text(title)
                .font(.body)
                .fontWeight(.medium)
                .lineLimit(1)

            Text(artistNames)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            if let year {
                Text(String(year))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .contextMenu {
            Button("Play") { onPlay() }
            Button("Add to Queue") { onAddToQueue() }
            Button("Add Next") { onAddNext() }
        }
    }

    @ViewBuilder
    private var albumArt: some View {
        if let url = coverArtURL {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                case .failure:
                    albumArtPlaceholder
                default:
                    Theme.placeholder
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Theme.placeholder
            Image(systemName: "photo")
                .font(.title)
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Card Overlay Button

private struct CardMenuButton: View {
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void
    @Binding var showMenu: Bool
    @State private var isHovered = false

    var body: some View {
        Button(action: presentMenu) {
            Image(systemName: "ellipsis")
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(.white)
                .frame(width: 30, height: 30)
                .background(isHovered ? Color.accentColor : Color.black.opacity(0.4))
                .clipShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }

    private func presentMenu() {
        showMenu = true
        let menu = NSMenu()
        let playItem = MenuItem(title: "Play") { onPlay() }
        menu.addItem(playItem)
        menu.addItem(NSMenuItem.separator())
        let queueItem = MenuItem(title: "Add to Queue") { onAddToQueue() }
        menu.addItem(queueItem)
        let nextItem = MenuItem(title: "Add Next") { onAddNext() }
        menu.addItem(nextItem)

        menu.popUp(positioning: nil,
                    at: NSEvent.mouseLocation,
                    in: nil)
        showMenu = false
    }
}

private class MenuItem: NSMenuItem {
    private let handler: () -> Void

    init(title: String, handler: @escaping () -> Void) {
        self.handler = handler
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        self.target = self
    }

    required init(coder: NSCoder) { fatalError() }

    @objc private func fire() { handler() }
}

// MARK: - Previews

private func sortedAlbums(
    _ field: LibrarySortField, _ direction: SortDirection
) -> [AlbumCardViewModel] {
    let sorted: [AlbumCardViewModel]
    switch field {
    case .title:
        sorted = PreviewData.albums.sorted { $0.title.localizedCompare($1.title) == .orderedAscending }
    case .artist:
        sorted = PreviewData.albums.sorted { $0.artistNames.localizedCompare($1.artistNames) == .orderedAscending }
    case .year:
        sorted = PreviewData.albums.sorted { ($0.year ?? 0) < ($1.year ?? 0) }
    case .dateAdded:
        sorted = PreviewData.albums
    }
    return direction == .ascending ? sorted : sorted.reversed()
}

private struct GridPreview: View {
    let width: CGFloat
    let height: CGFloat
    @State private var selectedAlbumId: String? = "a-04"
    @State private var sortField: LibrarySortField = .dateAdded
    @State private var sortDirection: SortDirection = .descending

    var body: some View {
        AlbumGridView(
            albums: sortedAlbums(sortField, sortDirection),
            selectedAlbumId: $selectedAlbumId,
            sortField: $sortField,
            sortDirection: $sortDirection,
            onPlayAlbum: { _ in },
            onAddToQueue: { _ in },
            onAddNext: { _ in }
        )
        .frame(width: width, height: height)
    }
}

#Preview("Grid — Wide") {
    GridPreview(width: 1100, height: 700)
}

#Preview("Grid — Medium") {
    GridPreview(width: 700, height: 600)
}

#Preview("Grid — Narrow") {
    GridPreview(width: 400, height: 600)
}

#Preview("Album Card") {
    let album = PreviewData.albums[0]
    let selected = PreviewData.albums[3]
    return HStack(spacing: 20) {
        AlbumCardView(
            title: album.title,
            artistNames: album.artistNames,
            year: album.year,
            coverArtURL: album.coverArtURL,
            isSelected: false,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {}
        )
        AlbumCardView(
            title: selected.title,
            artistNames: selected.artistNames,
            year: selected.year,
            coverArtURL: selected.coverArtURL,
            isSelected: true,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {}
        )
    }
    .padding()
}

private struct LibraryPreview: View {
    @State private var selectedAlbumId: String? = nil
    @State private var sortField: LibrarySortField = .dateAdded
    @State private var sortDirection: SortDirection = .descending
    @State private var selectedReleaseIndex: Int = 0

    var body: some View {
        HStack(spacing: 0) {
            AlbumGridView(
                albums: sortedAlbums(sortField, sortDirection),
                selectedAlbumId: $selectedAlbumId,
                sortField: $sortField,
                sortDirection: $sortDirection,
                onPlayAlbum: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in }
            )

            if let albumId = selectedAlbumId,
               let detail = PreviewData.albumDetails[albumId] {
                Divider()
                AlbumDetailContent(
                    detail: detail,
                    coverArtURL: PreviewData.albums.first(where: { $0.id == albumId })?.coverArtURL,
                    lightboxItems: [],
                    selectedReleaseIndex: $selectedReleaseIndex,
                    showShareCopied: false,
                    onClose: { selectedAlbumId = nil },
                    onPlay: {},
                    onShuffle: {},
                    onPlayFromTrack: { _ in },
                    onShare: {},
                    onAddNext: { _ in },
                    onAddToQueue: { _ in },
                    onChangeCover: {},
                    onManage: {},
                    onDeleteAlbum: {}
                )
                .frame(width: 400)
            }
        }
    }
}

#Preview("Library — Grid + Detail") {
    LibraryPreview()
        .frame(width: 1200, height: 700)
}
