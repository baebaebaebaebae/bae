import SwiftUI

enum AppScreen {
    case onboarding
    case linked(LibraryCredentials)
}

@Observable
class AppState {
    var screen: AppScreen
    var cloudClient: CloudHomeClient?

    init(keychainService: KeychainService) {
        if let creds = keychainService.loadCredentials() {
            screen = .linked(creds)
            if let url = URL(string: creds.proxyUrl) {
                cloudClient = CloudHomeClient(baseURL: url)
            }
        } else {
            screen = .onboarding
        }
    }
}
