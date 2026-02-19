import SwiftUI

enum AppScreen {
    case loading
    case welcome
    case unlock(libraryId: String, libraryName: String?, fingerprint: String?)
    case library(AppService)
}

struct ContentView: View {
    @Binding var appService: AppService?

    @State private var screen: AppScreen = .loading
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            switch screen {
            case .loading:
                Spacer()
                ProgressView("Loading...")
                Spacer()
            case .welcome:
                WelcomeView(onLibraryReady: openLibrary)
            case let .unlock(libraryId, libraryName, fingerprint):
                UnlockView(
                    libraryId: libraryId,
                    libraryName: libraryName,
                    fingerprint: fingerprint,
                    onUnlocked: { openLibrary(libraryId) }
                )
            case let .library(service):
                MainAppView(appService: service)
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding()
            }
        }
        .frame(minWidth: 900, minHeight: 600)
        .navigationTitle(windowTitle)
        .task {
            loadInitialState()
        }
    }

    private var windowTitle: String {
        switch screen {
        case .library(let service):
            return service.appHandle.libraryName() ?? "bae"
        default:
            return "bae"
        }
    }

    private func loadInitialState() {
        do {
            let libraries = try discoverLibraries()
            if libraries.isEmpty {
                screen = .welcome
                return
            }
            let lib = libraries[0]
            openLibrary(lib.id)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func openLibrary(_ libraryId: String) {
        error = nil
        screen = .loading
        Task.detached {
            do {
                let handle = try initApp(libraryId: libraryId)

                // Check if encryption is configured but key is missing from keyring
                if handle.isEncrypted() && !handle.checkEncryptionKeyAvailable() {
                    let name = handle.libraryName()
                    let fp = handle.getEncryptionFingerprint()
                    await MainActor.run {
                        screen = .unlock(
                            libraryId: libraryId,
                            libraryName: name,
                            fingerprint: fp
                        )
                    }
                    return
                }

                let service = AppService(appHandle: handle)
                await MainActor.run {
                    appService = service
                    screen = .library(service)
                }
            } catch {
                await MainActor.run {
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
