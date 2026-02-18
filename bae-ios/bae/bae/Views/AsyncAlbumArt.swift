import SwiftUI

struct AsyncAlbumArt: View {
    let imageId: String?
    let imageService: ImageService
    let size: CGFloat

    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color(.systemGray5))
                    .overlay {
                        Image(systemName: "photo")
                            .foregroundStyle(.tertiary)
                    }
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .task(id: imageId) {
            guard let imageId else { return }
            image = await imageService.image(for: imageId)
        }
    }
}
