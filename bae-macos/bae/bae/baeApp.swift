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

@main
struct baeApp: App {
    @State private var appService: AppService?

    var body: some Scene {
        WindowGroup {
            ContentView(appService: $appService)
        }
        Settings {
            if let appService {
                SettingsView(appService: appService)
            } else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "books.vertical",
                    description: Text("Open a library first to access settings")
                )
                .frame(width: 300, height: 200)
            }
        }
        .commands {
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
