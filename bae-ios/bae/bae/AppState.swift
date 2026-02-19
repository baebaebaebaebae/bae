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
    var imageService: ImageService?
    var networkMonitor = NetworkMonitor()

    init(keychainService: KeychainService) {
        if let creds = keychainService.loadCredentials() {
            // Check if we already have a local database
            if FileManager.default.fileExists(atPath: BootstrapService.databasePath().path) {
                screen = .library(creds)
            } else {
                screen = .bootstrapping(creds)
            }
            if let url = URL(string: creds.proxyUrl) {
                let client = CloudHomeClient(baseURL: url)
                cloudClient = client
                let crypto = PlaceholderCryptoService()
                imageService = ImageService(
                    cloudClient: client, crypto: crypto, encryptionKey: creds.encryptionKey)
            } else {
                let crypto = PlaceholderCryptoService()
                imageService = ImageService(
                    cloudClient: nil, crypto: crypto, encryptionKey: creds.encryptionKey)
            }
        } else {
            screen = .onboarding
        }
    }
}
