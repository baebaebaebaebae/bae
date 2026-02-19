import SwiftUI

struct ContentView: View {
    @State private var libraries: [BridgeLibraryInfo] = []
    @State private var selectedLibraryId: String?
    @State private var appHandle: AppHandle?
    @State private var error: String?

    var body: some View {
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
            if let handle = appHandle {
                Text("Library loaded: \(handle.libraryId())")
                    .font(.title2)
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
        .frame(minWidth: 700, minHeight: 500)
        .task {
            loadLibraries()
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
        appHandle = nil
        error = nil
        Task.detached {
            do {
                let handle = try initApp(libraryId: id)
                await MainActor.run {
                    appHandle = handle
                }
            } catch {
                await MainActor.run {
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
