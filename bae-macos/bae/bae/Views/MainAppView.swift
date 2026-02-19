import SwiftUI
import UniformTypeIdentifiers

enum MainSection {
    case library
    case importing
}

struct MainAppView: View {
    let appService: AppService
    @State private var activeSection: MainSection = .library
    @State private var showingSettings = false
    @State private var showingSyncSettings = false

    var body: some View {
        VStack(spacing: 0) {
            sectionBar
            Divider()

            ZStack {
                LibraryView(appService: appService)
                    .opacity(activeSection == .library ? 1 : 0)
                    .allowsHitTesting(activeSection == .library)

                ImportView(appService: appService)
                    .opacity(activeSection == .importing ? 1 : 0)
                    .allowsHitTesting(activeSection == .importing)
            }

            if appService.isActive {
                Divider()
                NowPlayingBar(appService: appService)
            }
        }
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
        .sheet(isPresented: $showingSettings) {
            SettingsView(appService: appService)
        }
        .sheet(isPresented: $showingSyncSettings) {
            NavigationStack {
                SyncSettingsView(appService: appService)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { showingSyncSettings = false }
                        }
                    }
            }
            .frame(minWidth: 500, minHeight: 400)
        }
    }

    // MARK: - Section Bar

    private var sectionBar: some View {
        HStack(spacing: 4) {
            Picker("Section", selection: $activeSection) {
                Text("Library").tag(MainSection.library)
                Text("Import").tag(MainSection.importing)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 200)

            Spacer()

            Button(action: { openFolderAndScan() }) {
                Label("Import Folder", systemImage: "plus")
            }
            .help("Import a folder of music")

            Button(action: { showingSyncSettings = true }) {
                Image(systemName: "arrow.triangle.2.circlepath")
            }
            .help("Sync Settings")

            Button(action: { showingSettings = true }) {
                Label("Settings", systemImage: "gearshape")
            }
            .help("Open settings")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
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
