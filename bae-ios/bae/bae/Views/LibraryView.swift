import SwiftUI

struct LibraryView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let syncService: SyncService?
    let credentials: LibraryCredentials
    let onUnlink: () -> Void

    var body: some View {
        TabView {
            ArtistListView(
                databaseService: databaseService, imageService: imageService,
                playbackService: playbackService, syncService: syncService
            )
            .tabItem {
                Label("Library", systemImage: "books.vertical")
            }
            Text("Search coming soon")
                .foregroundStyle(.secondary)
                .tabItem {
                    Label("Search", systemImage: "magnifyingglass")
                }
            SettingsPlaceholderView(credentials: credentials, onUnlink: onUnlink)
                .tabItem {
                    Label("Settings", systemImage: "gear")
                }
        }
        .safeAreaInset(edge: .bottom) {
            if let playbackService, playbackService.currentTrack != nil {
                MiniPlayerView(playbackService: playbackService, imageService: imageService)
            }
        }
        .onAppear { syncService?.startPeriodicSync() }
        .onDisappear { syncService?.stopPeriodicSync() }
    }
}

struct SettingsPlaceholderView: View {
    let credentials: LibraryCredentials
    let onUnlink: () -> Void

    var body: some View {
        NavigationStack {
            List {
                Section("Library") {
                    LabeledContent("Library ID", value: credentials.libraryId)
                    LabeledContent("Server", value: credentials.proxyUrl)
                }
                Section {
                    Button("Unlink Library", role: .destructive) {
                        onUnlink()
                    }
                }
            }
            .navigationTitle("Settings")
        }
    }
}
