import SwiftUI

struct AlbumGridView: View {
    let albums: [BridgeAlbum]
    let appService: AppService
    @Binding var selectedAlbumId: String?

    private let columns = [
        GridItem(.adaptive(minimum: 160), spacing: 16)
    ]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 20) {
                ForEach(albums, id: \.id) { album in
                    AlbumCardView(
                        album: album,
                        appService: appService,
                        isSelected: selectedAlbumId == album.id
                    )
                    .onTapGesture {
                        selectedAlbumId = album.id
                    }
                    .onTapGesture(count: 2) {
                        appService.playAlbum(albumId: album.id)
                    }
                }
            }
            .padding()
        }
        .background(Theme.background)
    }
}

struct AlbumCardView: View {
    let album: BridgeAlbum
    let appService: AppService
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

            Text(album.title)
                .font(.callout)
                .fontWeight(.medium)
                .lineLimit(1)

            Text(album.artistNames)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            if let year = album.year {
                Text(String(year))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(width: 160)
    }

    @ViewBuilder
    private var albumArt: some View {
        if let coverReleaseId = album.coverReleaseId,
           let urlString = appService.appHandle.getImageUrl(imageId: coverReleaseId),
           let url = URL(string: urlString) {
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
