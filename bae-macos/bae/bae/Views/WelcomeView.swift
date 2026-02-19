import SwiftUI

struct WelcomeView: View {
    let onLibraryReady: (String) -> Void

    @State private var libraryName: String = ""
    @State private var isCreating = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 32) {
            Spacer()
            Text("bae")
                .font(.system(size: 48, weight: .bold, design: .rounded))
            Text("Create a library to get started.")
                .font(.title3)
                .foregroundStyle(.secondary)
            VStack(spacing: 16) {
                TextField("Library name", text: $libraryName)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 300)
                Button(action: doCreate) {
                    if isCreating {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Create Library")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isCreating)
                .keyboardShortcut(.defaultAction)
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            Spacer()
        }
        .padding()
    }

    private func doCreate() {
        let trimmedName = libraryName.trimmingCharacters(in: .whitespaces)
        isCreating = true
        error = nil
        Task.detached {
            do {
                let name: String? = trimmedName.isEmpty ? nil : trimmedName
                let info = try createLibrary(name: name)
                await MainActor.run {
                    isCreating = false
                    onLibraryReady(info.id)
                }
            } catch {
                await MainActor.run {
                    isCreating = false
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
