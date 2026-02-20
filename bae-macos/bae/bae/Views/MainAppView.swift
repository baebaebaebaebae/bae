import SwiftUI
import UniformTypeIdentifiers

enum MainSection {
    case library
    case importing
}

struct SearchField: View {
    @Binding var text: String
    var prompt: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.caption)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
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

struct MainAppView: View {
    let appService: AppService
    @Environment(\.openSettings) private var openSettings
    @State private var activeSection: MainSection = .library
    @State private var searchText: String = ""
    @State private var showQueue = false

    var body: some View {
        VStack(spacing: 0) {
            titleBar
            ZStack {
                LibraryView(appService: appService, searchText: $searchText)
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
                    }
                )
            }
        }
        .toolbar(.hidden)
        .ignoresSafeArea(.all, edges: .top)
        .onKeyPress(.space) {
            appService.togglePlayPause()
            return .handled
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

    private var titleBar: some View {
        HStack(spacing: 12) {
            Picker("Section", selection: $activeSection) {
                Text("Library").tag(MainSection.library)
                Text("Import").tag(MainSection.importing)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 200)

            Spacer()

            SearchField(text: $searchText, prompt: "Artists, albums, tracks")
                .frame(width: 250)

            Button(action: { openSettings() }) {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("Settings")
        }
        .padding(.top, 6)
        .padding(.bottom, 8)
        .padding(.leading, 80)
        .padding(.trailing, 16)
        .background(Theme.surface)
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
