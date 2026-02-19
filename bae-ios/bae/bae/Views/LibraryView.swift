import SwiftUI

struct LibraryView: View {
    let databaseService: DatabaseService
    let imageService: ImageService?
    let playbackService: PlaybackService?
    let syncService: SyncService?
    let networkMonitor: NetworkMonitor
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
            SearchView(
                databaseService: databaseService, imageService: imageService,
                playbackService: playbackService
            )
            .tabItem {
                Label("Search", systemImage: "magnifyingglass")
            }
            SettingsView(
                credentials: credentials, syncService: syncService,
                playbackService: playbackService, onUnlink: onUnlink
            )
            .tabItem {
                Label("Settings", systemImage: "gear")
            }
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 0) {
                if !networkMonitor.isConnected {
                    Text("Offline — showing cached data")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 4)
                        .background(.ultraThinMaterial)
                }
                if let playbackService, playbackService.currentTrack != nil {
                    MiniPlayerView(playbackService: playbackService, imageService: imageService)
                }
            }
        }
        .onAppear { syncService?.startPeriodicSync() }
        .onDisappear { syncService?.stopPeriodicSync() }
    }
}
