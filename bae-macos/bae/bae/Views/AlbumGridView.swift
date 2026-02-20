import SwiftUI

struct AlbumCardViewModel: Identifiable {
    let id: String
    let title: String
    let artistNames: String
    let year: Int32?
    let coverArtURL: URL?
}

struct AlbumGridView: View {
    let albums: [AlbumCardViewModel]
    @Binding var selectedAlbumId: String?
    @Binding var sortField: LibrarySortField
    @Binding var sortDirection: SortDirection
    let onPlayAlbum: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNext: (String) -> Void

    private let columns = [
        GridItem(.adaptive(minimum: 160), spacing: 16)
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            libraryHeader
                .padding(.horizontal)
                .padding(.vertical, 8)

            ScrollView {
                LazyVGrid(columns: columns, spacing: 20) {
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
                        .onTapGesture(count: 2) {
                            onPlayAlbum(album.id)
                        }
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
        HStack {
            Text("Library")
                .font(.title2)
                .fontWeight(.bold)

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
                HStack(spacing: 4) {
                    Text(sortField.rawValue)
                        .font(.callout)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.caption2)
                }
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

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ZStack(alignment: .topTrailing) {
                albumArt
                    .frame(width: 160, height: 160)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .strokeBorder(
                                Color.accentColor,
                                lineWidth: isSelected ? 3 : 0
                            )
                    )

                if isHovered {
                    Menu {
                        Button("Play") { onPlay() }
                        Button("Add to Queue") { onAddToQueue() }
                        Button("Add Next") { onAddNext() }
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.caption)
                            .foregroundColor(.white)
                            .frame(width: 24, height: 24)
                            .background(.black.opacity(0.7))
                            .clipShape(Circle())
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .padding(6)
                }
            }
            .onHover { hovering in
                isHovered = hovering
            }

            Text(title)
                .font(.callout)
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
        .frame(width: 160)
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

// MARK: - Previews

private struct GridPreview: View {
    let width: CGFloat
    let height: CGFloat
    @State private var selectedAlbumId: String? = "a-04"
    @State private var sortField: LibrarySortField = .dateAdded
    @State private var sortDirection: SortDirection = .descending

    var body: some View {
        AlbumGridView(
            albums: PreviewData.albums,
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
                albums: PreviewData.albums,
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
                    selectedReleaseIndex: $selectedReleaseIndex,
                    showShareCopied: false,
                    transferring: false,
                    onClose: { selectedAlbumId = nil },
                    onPlay: {},
                    onPlayFromTrack: { _ in },
                    onShare: {},
                    onChangeCover: {},
                    onTransferToManaged: { _ in },
                    onEject: { _ in }
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
