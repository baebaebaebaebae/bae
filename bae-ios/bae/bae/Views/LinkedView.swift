import SwiftUI

struct LinkedView: View {
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
            .navigationTitle("bae")
        }
    }
}
