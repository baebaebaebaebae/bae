import SwiftUI

struct AlbumGridView: View {
    let albums: [BridgeAlbum]
    let appHandle: AppHandle

    private let columns = [
        GridItem(.adaptive(minimum: 160), spacing: 16)
    ]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 20) {
                ForEach(albums, id: \.id) { album in
                    AlbumCardView(album: album, appHandle: appHandle)
                }
            }
            .padding()
        }
    }
}

struct AlbumCardView: View {
    let album: BridgeAlbum
    let appHandle: AppHandle

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            albumArt
                .frame(width: 160, height: 160)
                .clipShape(RoundedRectangle(cornerRadius: 8))

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
           let urlString = appHandle.getImageUrl(imageId: coverReleaseId),
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
                    Color(.separatorColor)
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Color(.separatorColor)
            Image(systemName: "photo")
                .font(.title)
                .foregroundStyle(.secondary)
        }
    }
}
