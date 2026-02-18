import SwiftUI

struct ContentView: View {
    @State private var appState: AppState
    private let keychainService: KeychainService

    init() {
        let keychain = KeychainService()
        self.keychainService = keychain
        self._appState = State(initialValue: AppState(keychainService: keychain))
    }

    var body: some View {
        switch appState.screen {
        case .onboarding:
            OnboardingView { creds in
                do {
                    try keychainService.saveCredentials(creds)
                    if let url = URL(string: creds.proxyUrl) {
                        appState.cloudClient = CloudHomeClient(baseURL: url)
                    }
                    appState.screen = .bootstrapping(creds)
                } catch {
                    // Save failed; user stays on onboarding and can retry
                }
            }
        case .bootstrapping(let creds):
            if let client = appState.cloudClient {
                let crypto = PlaceholderCryptoService()
                let bootstrap = BootstrapService(
                    cloudClient: client, crypto: crypto, encryptionKey: creds.encryptionKey)
                BootstrapView(bootstrapService: bootstrap) { result in
                    appState.bootstrapResult = result
                    appState.screen = .library(creds)
                } onError: { _ in
                    // Stay on bootstrap view showing error
                }
            }
        case .library(let creds):
            LinkedView(credentials: creds) {
                do {
                    try keychainService.deleteCredentials()
                    // Delete local database and cached data
                    try? FileManager.default.removeItem(at: BootstrapService.databasePath())
                    try? FileManager.default.removeItem(at: BootstrapService.syncCursorsPath())
                    try? FileManager.default.removeItem(at: BootstrapService.imageCachePath())
                    appState.cloudClient = nil
                    appState.bootstrapResult = nil
                    appState.screen = .onboarding
                } catch {
                    // Delete failed; user stays on library view
                }
            }
        }
    }
}
