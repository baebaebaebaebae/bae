import SwiftUI

struct OnboardingView: View {
    let onLinked: (LibraryCredentials) -> Void

    @State private var showScanner = false
    @State private var error: String?

    private let parser: DeviceLinkParser = DeviceLinkParserImpl()

    var body: some View {
        VStack(spacing: 24) {
            Spacer()
            Text("bae")
                .font(.system(size: 48, weight: .bold))
            Text("Scan a QR code from your library to get started.")
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Spacer()
            Button("Scan QR Code") {
                error = nil
                showScanner = true
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
            }
            Spacer()
        }
        .sheet(isPresented: $showScanner) {
            QRScannerView(
                onScanned: { json in
                    handleScanned(json)
                },
                onError: { msg in
                    error = msg
                    showScanner = false
                }
            )
        }
    }

    private func handleScanned(_ json: String) {
        showScanner = false
        do {
            let creds = try parser.parse(json: json)
            onLinked(creds)
        } catch {
            self.error = error.localizedDescription
        }
    }
}
