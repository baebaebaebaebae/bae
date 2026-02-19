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

#Preview("Album Grid") {
    AlbumGridView(
        albums: [
            AlbumCardViewModel(id: "a-1", title: "Album Title", artistNames: "Artist Name", year: 2024, coverArtURL: nil),
            AlbumCardViewModel(id: "a-2", title: "Another Album", artistNames: "Another Artist", year: 2023, coverArtURL: nil),
            AlbumCardViewModel(id: "a-3", title: "Third Album", artistNames: "Third Artist", year: nil, coverArtURL: nil),
            AlbumCardViewModel(id: "a-4", title: "Fourth Album With a Long Title", artistNames: "Artist With Long Name", year: 2022, coverArtURL: nil),
        ],
        selectedAlbumId: .constant("a-2"),
        sortField: .constant(.dateAdded),
        sortDirection: .constant(.descending),
        onPlayAlbum: { _ in },
        onAddToQueue: { _ in },
        onAddNext: { _ in }
    )
    .frame(width: 600, height: 400)
}

#Preview("Album Card") {
    HStack(spacing: 20) {
        AlbumCardView(
            title: "Album Title",
            artistNames: "Artist Name",
            year: 2024,
            coverArtURL: nil,
            isSelected: false,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {}
        )
        AlbumCardView(
            title: "Selected Album",
            artistNames: "Another Artist",
            year: 2023,
            coverArtURL: nil,
            isSelected: true,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {}
        )
    }
    .padding()
}
