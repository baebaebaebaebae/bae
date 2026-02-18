import SwiftUI

enum AppScreen {
    case onboarding
    case bootstrapping(LibraryCredentials)
    case library(LibraryCredentials)
}

@Observable
class AppState {
    var screen: AppScreen
    var cloudClient: CloudHomeClient?
    var bootstrapResult: BootstrapResult?

    init(keychainService: KeychainService) {
        if let creds = keychainService.loadCredentials() {
            // Check if we already have a local database
            if FileManager.default.fileExists(atPath: BootstrapService.databasePath().path) {
                screen = .library(creds)
            } else {
                screen = .bootstrapping(creds)
            }
            if let url = URL(string: creds.proxyUrl) {
                cloudClient = CloudHomeClient(baseURL: url)
            }
        } else {
            screen = .onboarding
        }
    }
}
