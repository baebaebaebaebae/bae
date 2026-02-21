import Sparkle
import SwiftUI

// MARK: - FocusedValue for AppService

struct AppServiceKey: FocusedValueKey {
    typealias Value = AppService
}

extension FocusedValues {
    var appService: AppService? {
        get { self[AppServiceKey.self] }
        set { self[AppServiceKey.self] = newValue }
    }
}

// MARK: - Import notification

extension Notification.Name {
    static let importFolder = Notification.Name("bae.importFolder")
}

// MARK: - Sparkle update helpers

final class CheckForUpdatesViewModel: ObservableObject {
    @Published var canCheckForUpdates = false

    private let updater: SPUUpdater

    init(updater: SPUUpdater) {
        self.updater = updater
        updater.publisher(for: \.canCheckForUpdates)
            .assign(to: &$canCheckForUpdates)
    }

    func checkForUpdates() {
        updater.checkForUpdates()
    }
}

struct CheckForUpdatesView: View {
    @ObservedObject private var viewModel: CheckForUpdatesViewModel

    init(viewModel: CheckForUpdatesViewModel) {
        self.viewModel = viewModel
    }

    var body: some View {
        Button("Check for Updates...") {
            viewModel.checkForUpdates()
        }
        .disabled(!viewModel.canCheckForUpdates)
    }
}

@main
struct baeApp: App {
    @State private var appService: AppService?
    private let updaterController: SPUStandardUpdaterController
    @ObservedObject private var checkForUpdatesViewModel: CheckForUpdatesViewModel

    init() {
        let controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil,
        )
        updaterController = controller
        checkForUpdatesViewModel = CheckForUpdatesViewModel(updater: controller.updater)
    }

    var body: some Scene {
        WindowGroup {
            ContentView(appService: $appService)
                .environmentObject(checkForUpdatesViewModel)
        }
        .windowStyle(.hiddenTitleBar)
        Settings {
            if let appService {
                SettingsView(appService: appService)
                    .environmentObject(checkForUpdatesViewModel)
            } else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "books.vertical",
                    description: Text("Open a library first to access settings"),
                )
                .frame(width: 300, height: 200)
            }
        }
        .commands {
            CommandGroup(after: .appInfo) {
                CheckForUpdatesView(viewModel: checkForUpdatesViewModel)
            }
            fileMenuCommands
            playbackMenuCommands
        }
    }

    // MARK: - File menu

    @CommandsBuilder
    private var fileMenuCommands: some Commands {
        CommandGroup(after: .newItem) {
            Button("Import Folder...") {
                NotificationCenter.default.post(name: .importFolder, object: nil)
            }
            .keyboardShortcut("i", modifiers: .command)
            .disabled(appService == nil)
        }
    }

    // MARK: - Playback menu

    @CommandsBuilder
    private var playbackMenuCommands: some Commands {
        CommandMenu("Playback") {
            Button("Play / Pause") {
                appService?.togglePlayPause()
            }
            .keyboardShortcut(.space, modifiers: [])
            .disabled(appService == nil)

            Button("Next Track") {
                appService?.nextTrack()
            }
            .keyboardShortcut(.rightArrow, modifiers: .command)
            .disabled(appService == nil)

            Button("Previous Track") {
                appService?.previousTrack()
            }
            .keyboardShortcut(.leftArrow, modifiers: .command)
            .disabled(appService == nil)

            Divider()

            Button("Cycle Repeat Mode") {
                appService?.cycleRepeatMode()
            }
            .keyboardShortcut("r", modifiers: .command)
            .disabled(appService == nil)
        }
    }
}
