import SwiftUI
import UniformTypeIdentifiers

enum MainSection {
    case library
    case importing
}

// Adjusts the position of the window's traffic light buttons (close/minimize/zoom).
// With .hiddenTitleBar, the buttons sit at the system default position which doesn't align
// with our custom title bar content. This modifier accesses the NSWindow and shifts them.
// Reapplies on window resize since macOS resets button positions during resize.
// Technique from: https://medium.com/@clyapp/fix-the-problem-that-nswindow-traffic-light-buttons-always-revert-to-its-origin-position-after-6a13675df18a
struct TrafficLightOffset: ViewModifier {
    let xOffset: CGFloat
    let yOffset: CGFloat

    func body(content: Content) -> some View {
        content
            .background(TrafficLightHelper(xOffset: xOffset, yOffset: yOffset))
    }

    private struct TrafficLightHelper: NSViewRepresentable {
        let xOffset: CGFloat
        let yOffset: CGFloat

        func makeNSView(context: Context) -> NSView {
            let view = NSView()
            DispatchQueue.main.async {
                guard let window = view.window else { return }
                adjustButtons(in: window)
                context.coordinator.observeResize(window: window, xOffset: xOffset, yOffset: yOffset)
            }
            return view
        }

        func updateNSView(_ nsView: NSView, context: Context) {}

        func makeCoordinator() -> Coordinator {
            Coordinator()
        }

        private func adjustButtons(in window: NSWindow) {
            for buttonType: NSWindow.ButtonType in [.closeButton, .miniaturizeButton, .zoomButton] {
                guard let button = window.standardWindowButton(buttonType) else { continue }
                var origin = button.frame.origin
                origin.x += xOffset
                origin.y -= yOffset
                button.setFrameOrigin(origin)
            }
        }

        class Coordinator: NSObject {
            private var observation: Any?

            func observeResize(window: NSWindow, xOffset: CGFloat, yOffset: CGFloat) {
                observation = NotificationCenter.default.addObserver(
                    forName: NSWindow.didResizeNotification,
                    object: window,
                    queue: .main
                ) { notification in
                    guard let window = notification.object as? NSWindow else { return }
                    for buttonType: NSWindow.ButtonType in [.closeButton, .miniaturizeButton, .zoomButton] {
                        guard let button = window.standardWindowButton(buttonType) else { continue }
                        var origin = button.frame.origin
                        origin.x += xOffset
                        origin.y -= yOffset
                        button.setFrameOrigin(origin)
                    }
                }
            }

            deinit {
                if let observation { NotificationCenter.default.removeObserver(observation) }
            }
        }
    }
}

// Makes an area behave like a native title bar: draggable to move the window,
// double-click to zoom (maximize/restore). Needed because .hiddenTitleBar removes
// the system title bar and our custom HStack doesn't inherit that behavior.
struct WindowDragArea: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = DragView()
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    private class DragView: NSView {
        override func mouseDown(with event: NSEvent) {
            if event.clickCount == 2 {
                window?.zoom(nil)
            } else {
                window?.performDrag(with: event)
            }
        }
    }
}

struct SearchField: View {
    @Binding var text: String
    var prompt: String
    var onEscape: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.caption)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .onKeyPress(.escape) {
                    text = ""
                    onEscape?()
                    return .handled
                }
            if !text.isEmpty {
                Button(action: { text = "" }) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .font(.caption)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

@Observable
class OverlayCoordinator {
    var lightboxItems: [LightboxItem] = []
    var lightboxIndex: Int?
}

struct MainAppView: View {
    let appService: AppService
    @Environment(\.openSettings) private var openSettings
    @State private var activeSection: MainSection = .library
    @State private var searchText: String = ""
    @State private var selectedAlbumId: String?
    @State private var showQueue = false
    @State private var searchDebounceTask: Task<Void, Never>?
    @FocusState private var searchFocused: Bool
    @State private var overlayCoordinator = OverlayCoordinator()

    var body: some View {
        ZStack {
            VStack(spacing: 0) {
                titleBar
                ZStack {
                    LibraryView(appService: appService, selectedAlbumId: $selectedAlbumId)
                        .opacity(activeSection == .library ? 1 : 0)
                        .allowsHitTesting(activeSection == .library)

                    ImportView(appService: appService)
                        .opacity(activeSection == .importing ? 1 : 0)
                        .allowsHitTesting(activeSection == .importing)
                }

                if appService.isActive {
                    Divider()
                    NowPlayingBar(
                        trackTitle: appService.trackTitle,
                        artistNames: appService.artistNames,
                        coverArtURL: appService.imageURL(for: appService.coverImageId),
                        isPlaying: appService.isPlaying,
                        currentPositionMs: appService.currentPositionMs,
                        currentDurationMs: appService.currentDurationMs,
                        volume: appService.volume,
                        repeatMode: appService.repeatMode,
                        showQueue: $showQueue,
                        queueIsActive: appService.isActive,
                        queueNowPlayingTitle: appService.trackTitle,
                        queueNowPlayingArtist: appService.artistNames,
                        queueNowPlayingArtURL: appService.imageURL(for: appService.coverImageId),
                        queueItems: appService.queueItems.map { item in
                            QueueItemViewModel(
                                id: item.trackId,
                                title: item.title,
                                artistNames: item.artistNames,
                                albumTitle: item.albumTitle,
                                durationMs: item.durationMs,
                                coverArtURL: appService.imageURL(for: item.coverImageId)
                            )
                        },
                        onPlayPause: { appService.togglePlayPause() },
                        onNext: { appService.nextTrack() },
                        onPrevious: { appService.previousTrack() },
                        onSeek: { appService.seek(positionMs: $0) },
                        onVolumeChange: { appService.setVolume($0) },
                        onCycleRepeat: { appService.cycleRepeatMode() },
                        onQueueClear: { appService.clearQueue() },
                        onQueueSkipTo: { appService.skipToQueueIndex(index: UInt32($0)) },
                        onQueueRemove: { appService.removeFromQueue(index: UInt32($0)) },
                        onQueueReorder: { from, to in
                            appService.reorderQueue(fromIndex: UInt32(from), toIndex: UInt32(to))
                        },
                        onQueueInsertTracks: { ids, index in
                            resolveAndInsertInQueue(ids: ids, at: index)
                        },
                        onDropToQueue: { ids in
                            resolveAndAddToQueue(ids: ids)
                        },
                        onNavigateToAlbum: {
                            if let albumId = appService.currentAlbumId {
                                selectedAlbumId = albumId
                            }
                        },
                        onNavigateToArtist: {}
                    )
                }
            }

            if overlayCoordinator.lightboxIndex != nil && !overlayCoordinator.lightboxItems.isEmpty {
                ImageLightbox(items: overlayCoordinator.lightboxItems, currentIndex: $overlayCoordinator.lightboxIndex)
            }
        }
        .environment(overlayCoordinator)
        .toolbar(.hidden)
        .ignoresSafeArea(.all, edges: .top)
        .modifier(TrafficLightOffset(xOffset: 6, yOffset: 7))
        .onAppear {
            NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                guard event.keyCode == 49 else { return event } // 49 = space
                // Don't steal space from text fields
                if let responder = event.window?.firstResponder,
                   responder is NSTextView || responder is NSTextField {
                    return event
                }
                appService.togglePlayPause()
                return nil
            }
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDrop(providers)
        }
        .onReceive(NotificationCenter.default.publisher(for: .importFolder)) { _ in
            openFolderAndScan()
        }
        .focusedSceneValue(\.appService, appService)
    }

    // MARK: - Title bar

    private var showSearchResults: Bool {
        searchFocused && !searchText.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private var titleBar: some View {
        ZStack {
            Picker("Section", selection: $activeSection) {
                Text("Library").tag(MainSection.library)
                Text("Import").tag(MainSection.importing)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 200)

            HStack(spacing: 12) {
                Spacer()
                SearchField(text: $searchText, prompt: "Artists, albums, tracks", onEscape: { searchFocused = false })
                .focused($searchFocused)
                .frame(width: 250)
                .popover(isPresented: .constant(showSearchResults), arrowEdge: .bottom) {
                    SearchView(
                        results: appService.searchResults,
                        searchQuery: appService.searchQuery,
                        resolveImageURL: { appService.imageURL(for: $0) },
                        onSelectArtist: { _ in
                            searchFocused = false
                            searchText = ""
                        },
                        onSelectAlbum: { albumId in
                            searchFocused = false
                            searchText = ""
                            appService.search(query: "")
                            selectedAlbumId = albumId
                        },
                        onPlayTrack: { _ in }
                    )
                    .frame(width: 400, height: 350)
                }

            Button(action: { openSettings() }) {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("Settings")
            }
        }
        .padding(.top, 8)
        .padding(.bottom, 8)
        .padding(.leading, 80)
        .padding(.trailing, 16)
        .background { WindowDragArea() }
        .background(Theme.surface)
        .onChange(of: searchText) { _, newValue in
            searchDebounceTask?.cancel()
            searchDebounceTask = Task {
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled else { return }
                appService.search(query: newValue)
            }
        }
        .onChange(of: searchFocused) { _, focused in
            if !focused {
                searchText = ""
                appService.search(query: "")
            }
        }
    }

    // MARK: - Queue drop handling

    /// Resolves IDs (which may be track IDs or album IDs) into track IDs and adds them to the end of the queue.
    private func resolveAndInsertInQueue(ids: [String], at index: Int) {
        Task.detached { [appService] in
            var trackIds: [String] = []
            for id in ids {
                if let detail = try? appService.appHandle.getAlbumDetail(albumId: id) {
                    trackIds.append(contentsOf: detail.releases.first?.tracks.map(\.id) ?? [])
                } else {
                    trackIds.append(id)
                }
            }
            if !trackIds.isEmpty {
                await MainActor.run {
                    appService.insertInQueue(trackIds: trackIds, index: UInt32(index))
                }
            }
        }
    }

    private func resolveAndAddToQueue(ids: [String]) {
        Task.detached { [appService] in
            var trackIds: [String] = []
            for id in ids {
                if let detail = try? appService.appHandle.getAlbumDetail(albumId: id) {
                    trackIds.append(contentsOf: detail.releases.first?.tracks.map(\.id) ?? [])
                } else {
                    trackIds.append(id)
                }
            }
            if !trackIds.isEmpty {
                await MainActor.run {
                    appService.addToQueue(trackIds: trackIds)
                }
            }
        }
    }

    // MARK: - Scan + Drop

    private func openFolderAndScan() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder containing music to import"
        panel.prompt = "Scan"

        guard panel.runModal() == .OK, let url = panel.url else { return }

        appService.scanFolder(path: url.path)
        activeSection = .importing
    }

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        guard provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else {
            return false
        }

        provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
            guard let data = item as? Data,
                  let url = URL(dataRepresentation: data, relativeTo: nil) else {
                return
            }
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir),
                  isDir.boolValue else {
                return
            }
            DispatchQueue.main.async {
                appService.scanFolder(path: url.path)
                activeSection = .importing
            }
        }
        return true
    }
}
