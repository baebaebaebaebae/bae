import SwiftUI

struct WelcomeView: View {
    let onLibraryReady: (String) -> Void

    enum Mode {
        case choose
        case restore
    }

    @State private var mode: Mode = .choose
    @State private var isCreating = false
    @State private var error: String?

    // Restore form fields
    @State private var libraryId = ""
    @State private var bucket = ""
    @State private var region = ""
    @State private var endpoint = ""
    @State private var accessKey = ""
    @State private var secretKey = ""
    @State private var encryptionKey = ""
    @State private var isRestoring = false

    var body: some View {
        switch mode {
        case .choose:
            chooseView
        case .restore:
            restoreView
        }
    }

    private var chooseView: some View {
        VStack(spacing: 32) {
            Spacer()
            Text("bae")
                .font(.system(size: 48, weight: .bold, design: .rounded))
            Text("Get started with your music library.")
                .font(.title3)
                .foregroundStyle(.secondary)
            VStack(spacing: 12) {
                Button(action: doCreate) {
                    if isCreating {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Create new library")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isCreating)
                .keyboardShortcut(.defaultAction)
                Button(action: { mode = .restore }) {
                    Text("Restore from cloud")
                }
                .buttonStyle(.bordered)
                .disabled(isCreating)
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

    private var restoreView: some View {
        VStack(spacing: 0) {
            Text("Restore from cloud")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text("Enter the S3 bucket details and encryption key from your other device.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.bottom, 16)
            Form {
                TextField("Library ID", text: $libraryId)
                    .textContentType(.none)
                    .help("The UUID from your other device's library")
                TextField("S3 Bucket", text: $bucket)
                TextField("Region", text: $region)
                TextField("Endpoint (optional)", text: $endpoint)
                    .help("Leave empty for standard AWS S3")
                SecureField("Access Key", text: $accessKey)
                SecureField("Secret Key", text: $secretKey)
                SecureField("Encryption Key", text: $encryptionKey)
                    .help("64-character hex-encoded encryption key")
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }
            if isRestoring {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Downloading and decrypting your library...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 12)
            }
            HStack(spacing: 12) {
                Button("Back") {
                    mode = .choose
                    error = nil
                }
                .buttonStyle(.bordered)
                .disabled(isRestoring)
                Button("Restore") {
                    doRestore()
                }
                .buttonStyle(.borderedProminent)
                .disabled(isRestoring || !restoreFormValid)
                .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
        }
        .padding(.horizontal)
    }

    private var restoreFormValid: Bool {
        !libraryId.isEmpty && !bucket.isEmpty && !region.isEmpty
            && !accessKey.isEmpty && !secretKey.isEmpty && !encryptionKey.isEmpty
    }

    private func doCreate() {
        isCreating = true
        error = nil
        Task.detached {
            do {
                let info = try createLibrary(name: nil)
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

    private func doRestore() {
        isRestoring = true
        error = nil
        let lid = libraryId
        let b = bucket
        let r = region
        let ep = endpoint
        let ak = accessKey
        let sk = secretKey
        let ek = encryptionKey
        Task.detached {
            do {
                let info = try restoreFromCloud(
                    libraryId: lid,
                    bucket: b,
                    region: r,
                    endpoint: ep,
                    accessKey: ak,
                    secretKey: sk,
                    encryptionKeyHex: ek
                )
                await MainActor.run {
                    isRestoring = false
                    onLibraryReady(info.id)
                }
            } catch {
                await MainActor.run {
                    isRestoring = false
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
