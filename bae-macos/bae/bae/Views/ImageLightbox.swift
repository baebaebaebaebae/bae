import SwiftUI

struct LightboxItem: Identifiable {
    let id: String
    let label: String
    let url: URL?
}

struct ImageLightbox: View {
    let items: [LightboxItem]
    @Binding var currentIndex: Int?
    @State private var magnification: CGFloat = 1.0
    @State private var magnifyAnchor: UnitPoint = .center
    @FocusState private var focused: Bool

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
            // Background overlay — dismiss on tap
            Color.black.opacity(0.85)
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture { currentIndex = nil }

            // Content (does not pass taps to dismiss overlay)
            VStack(spacing: 0) {
                // Image area (flexible, with nav/close overlays)
                ZStack {
                    if let url = currentItem.url {
                        AsyncImage(url: url) { phase in
                            switch phase {
                            case let .success(image):
                                image
                                    .resizable()
                                    .aspectRatio(contentMode: .fit)
                                    .scaleEffect(magnification, anchor: magnifyAnchor)
                                    .gesture(
                                        MagnifyGesture()
                                            .onChanged { value in
                                                magnifyAnchor = value.startAnchor
                                                magnification = max(value.magnification, 1.0)
                                            }
                                            .onEnded { _ in
                                                withAnimation(.easeOut(duration: 0.25)) {
                                                    magnification = 1.0
                                                }
                                            },
                                    )
                                    .padding(40)
                                    .shadow(color: .black.opacity(0.5), radius: 20)
                                    .help(currentItem.label)
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

                    // Navigation buttons (left/right edges)
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
                        }
                        .padding(.horizontal, 16)
                        .opacity(magnification > 1.01 ? 0 : 1)
                    }

                    // Close button (top-right)
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
                    .opacity(magnification > 1.01 ? 0 : 1)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                // Thumbnail strip (fixed height, below the image area)
                if canCycle {
                    GeometryReader { geo in
                        ScrollViewReader { scrollProxy in
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 6) {
                                    ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                                        thumbnailView(for: item, at: index)
                                            .id(index)
                                    }
                                }
                                .padding(.horizontal, 8)
                                .frame(minWidth: geo.size.width)
                            }
                            .onChange(of: safeIndex) { _, newIndex in
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    scrollProxy.scrollTo(newIndex, anchor: .center)
                                }
                            }
                        }
                    }
                    .frame(height: 64)
                    .padding(.bottom, 16)
                    .opacity(magnification > 1.01 ? 0 : 1)
                }
            }
            .allowsHitTesting(true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.escape) {
            currentIndex = nil
            return .handled
        }
        .onKeyPress(.leftArrow) {
            navigatePrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            navigateNext()
            return .handled
        }
        .onAppear { focused = true }
        .onChange(of: safeIndex) { _, _ in
            magnification = 1.0
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
        .allowsHitTesting(false)
    }

    @ViewBuilder
    private func thumbnailView(for item: LightboxItem, at index: Int) -> some View {
        let isActive = index == safeIndex
        Button(action: { currentIndex = index }) {
            Group {
                if let url = item.url {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case let .success(image):
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
                        lineWidth: isActive ? 2 : 1,
                    ),
            )
        }
        .buttonStyle(.plain)
        .help(item.label)
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
