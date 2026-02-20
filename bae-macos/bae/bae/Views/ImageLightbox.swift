import SwiftUI

struct LightboxItem: Identifiable {
    let id: String
    let label: String
    let url: URL?
}

struct ImageLightbox: View {
    let items: [LightboxItem]
    @Binding var currentIndex: Int?

    private var safeIndex: Int {
        guard let idx = currentIndex, idx >= 0, idx < items.count else { return 0 }
        return idx
    }

    private var currentItem: LightboxItem {
        items[safeIndex]
    }

    private var canCycle: Bool {
        items.count > 1
    }

    var body: some View {
        ZStack {
            Color.black.opacity(0.85)
                .ignoresSafeArea()

            VStack(spacing: 0) {
                GeometryReader { geometry in
                    let availableHeight = geometry.size.height - 120
                    ZStack {
                        if let url = currentItem.url {
                            AsyncImage(url: url) { phase in
                                switch phase {
                                case .success(let image):
                                    image
                                        .resizable()
                                        .aspectRatio(contentMode: .fit)
                                        .frame(
                                            maxWidth: geometry.size.width - 120,
                                            maxHeight: availableHeight
                                        )
                                        .shadow(color: .black.opacity(0.5), radius: 20)
                                case .failure:
                                    imageFallback
                                default:
                                    ProgressView()
                                        .controlSize(.large)
                                }
                            }
                        } else {
                            imageFallback
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                VStack(spacing: 10) {
                    Text("\(currentItem.label) \u{2014} \(safeIndex + 1) of \(items.count)")
                        .font(.callout)
                        .foregroundStyle(.white.opacity(0.7))
                        .lineLimit(1)

                    if canCycle {
                        ScrollViewReader { scrollProxy in
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 6) {
                                    ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                                        thumbnailView(for: item, at: index)
                                            .id(index)
                                    }
                                }
                                .padding(.horizontal, 8)
                            }
                            .frame(height: 64)
                            .onChange(of: safeIndex) { _, newIndex in
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    scrollProxy.scrollTo(newIndex, anchor: .center)
                                }
                            }
                        }
                    }
                }
                .padding(.bottom, 16)
            }

            if canCycle {
                HStack {
                    Button(action: navigatePrevious) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 48, height: 48)
                            Image(systemName: "chevron.left")
                                .font(.title2.weight(.medium))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(.leading, 16)
                    Spacer()
                }
            }

            if canCycle {
                HStack {
                    Spacer()
                    Button(action: navigateNext) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 48, height: 48)
                            Image(systemName: "chevron.right")
                                .font(.title2.weight(.medium))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(.trailing, 16)
                }
            }

            VStack {
                HStack {
                    Spacer()
                    Button(action: { currentIndex = nil }) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 36, height: 36)
                            Image(systemName: "xmark")
                                .font(.body.weight(.semibold))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(12)
                }
                Spacer()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onKeyPress(.leftArrow) {
            navigatePrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            navigateNext()
            return .handled
        }
        .onKeyPress(.escape) {
            currentIndex = nil
            return .handled
        }
    }

    private var imageFallback: some View {
        VStack(spacing: 8) {
            Image(systemName: "photo")
                .font(.largeTitle)
                .foregroundStyle(.gray)
            Text("Cannot load image")
                .font(.callout)
                .foregroundStyle(.gray)
        }
    }

    @ViewBuilder
    private func thumbnailView(for item: LightboxItem, at index: Int) -> some View {
        let isActive = index == safeIndex
        Button(action: { currentIndex = index }) {
            Group {
                if let url = item.url {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case .success(let image):
                            image
                                .resizable()
                                .aspectRatio(contentMode: .fill)
                                .frame(width: 56, height: 56)
                                .clipped()
                        case .failure:
                            thumbnailPlaceholder
                        default:
                            Theme.placeholder
                                .frame(width: 56, height: 56)
                        }
                    }
                } else {
                    thumbnailPlaceholder
                }
            }
            .frame(width: 56, height: 56)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(
                        isActive ? .white : .gray.opacity(0.4),
                        lineWidth: isActive ? 2 : 1
                    )
            )
        }
        .buttonStyle(.plain)
    }

    private var thumbnailPlaceholder: some View {
        Color.gray.opacity(0.3)
            .frame(width: 56, height: 56)
            .overlay {
                Image(systemName: "photo")
                    .foregroundStyle(.gray)
            }
    }

    private func navigatePrevious() {
        if canCycle {
            currentIndex = safeIndex == 0 ? items.count - 1 : safeIndex - 1
        }
    }

    private func navigateNext() {
        if canCycle {
            currentIndex = safeIndex == items.count - 1 ? 0 : safeIndex + 1
        }
    }
}
