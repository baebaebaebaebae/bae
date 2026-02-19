import SwiftUI

struct ImportView: View {
    let folderURL: URL
    let appService: AppService
    let onDismiss: () -> Void

    @State private var status: ImportStatus = .ready

    var body: some View {
        VStack(spacing: 20) {
            HStack {
                Text("Import Folder")
                    .font(.headline)
                Spacer()
                Button("Cancel") { onDismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                Label(folderURL.lastPathComponent, systemImage: "folder")
                    .font(.body)

                Text(folderURL.path)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            switch status {
            case .ready:
                Text("Folder import is not yet available in the macOS app. Use the desktop app to import music.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            case .error(let message):
                Text(message)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Spacer()
        }
        .padding()
        .frame(minWidth: 400, minHeight: 200)
    }
}

enum ImportStatus {
    case ready
    case error(String)
}
