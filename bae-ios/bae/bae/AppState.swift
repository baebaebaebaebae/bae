import SwiftUI

enum AppScreen {
    case onboarding
    case linked(LibraryCredentials)
}

@Observable
class AppState {
    var screen: AppScreen

    init(keychainService: KeychainService) {
        if let creds = keychainService.loadCredentials() {
            screen = .linked(creds)
        } else {
            screen = .onboarding
        }
    }
}
