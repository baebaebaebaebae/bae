import SwiftUI

struct ContentView: View {
    @State private var libraries: [BridgeLibraryInfo] = []
    @State private var selectedLibraryId: String?
    @State private var appService: AppService?
    @State private var error: String?

    var body: some View {
        Group {
            if let service = appService {
                LibraryView(appService: service)
            } else {
                libraryPicker
            }
        }
        .frame(minWidth: 900, minHeight: 600)
        .task {
            loadLibraries()
        }
    }

    private var libraryPicker: some View {
        NavigationSplitView {
            List(libraries, id: \.id, selection: $selectedLibraryId) { library in
                VStack(alignment: .leading) {
                    Text(library.name ?? library.id)
                        .font(.headline)
                    Text(library.path)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Libraries")
        } detail: {
            if let error {
                ContentUnavailableView(
                    "Error",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else if selectedLibraryId != nil {
                ProgressView("Loading...")
            } else {
                ContentUnavailableView(
                    "Select a library",
                    systemImage: "books.vertical",
                    description: Text("Choose a library from the sidebar")
                )
            }
        }
        .onChange(of: selectedLibraryId) { _, newValue in
            if let id = newValue {
                openLibrary(id)
            }
        }
    }

    private func loadLibraries() {
        do {
            libraries = try discoverLibraries()
            if libraries.count == 1 {
                selectedLibraryId = libraries.first?.id
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func openLibrary(_ id: String) {
        appService = nil
        error = nil
        Task.detached {
            do {
                let handle = try initApp(libraryId: id)
                let service = AppService(appHandle: handle)
                await MainActor.run {
                    appService = service
                }
            } catch {
                await MainActor.run {
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
