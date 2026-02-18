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
                    appState.screen = .linked(creds)
                    if let url = URL(string: creds.proxyUrl) {
                        appState.cloudClient = CloudHomeClient(baseURL: url)
                    }
                } catch {
                    // Save failed; user stays on onboarding and can retry
                }
            }
        case .linked(let creds):
            LinkedView(credentials: creds) {
                do {
                    try keychainService.deleteCredentials()
                    appState.cloudClient = nil
                    appState.screen = .onboarding
                } catch {
                    // Delete failed; user stays on linked view
                }
            }
        }
    }
}
