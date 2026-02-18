import SwiftUI

struct LibraryView: View {
    let databaseService: DatabaseService
    let credentials: LibraryCredentials
    let onUnlink: () -> Void

    var body: some View {
        TabView {
            ArtistListView(databaseService: databaseService)
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
