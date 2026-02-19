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
    let onPlayAlbum: (String) -> Void

    private let columns = [
        GridItem(.adaptive(minimum: 160), spacing: 16)
    ]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 20) {
                ForEach(albums) { album in
                    AlbumCardView(
                        title: album.title,
                        artistNames: album.artistNames,
                        year: album.year,
                        coverArtURL: album.coverArtURL,
                        isSelected: selectedAlbumId == album.id
                    )
                    .onTapGesture {
                        selectedAlbumId = album.id
                    }
                    .onTapGesture(count: 2) {
                        onPlayAlbum(album.id)
                    }
                }
            }
            .padding()
        }
        .background(Theme.background)
    }
}

struct AlbumCardView: View {
    let title: String
    let artistNames: String
    let year: Int32?
    let coverArtURL: URL?
    let isSelected: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
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
        onPlayAlbum: { _ in }
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
            isSelected: false
        )
        AlbumCardView(
            title: "Selected Album",
            artistNames: "Another Artist",
            year: 2023,
            coverArtURL: nil,
            isSelected: true
        )
    }
    .padding()
}
