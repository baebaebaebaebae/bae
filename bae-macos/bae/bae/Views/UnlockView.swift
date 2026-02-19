import SwiftUI

struct UnlockView: View {
    let libraryId: String
    let libraryName: String?
    let fingerprint: String?
    let onUnlocked: () -> Void

    @State private var keyHex: String = ""
    @State private var isUnlocking = false
    @State private var error: String?

    private var isValidHex: Bool {
        keyHex.count == 64 && keyHex.allSatisfy { $0.isHexDigit }
    }

    var body: some View {
        VStack(spacing: 32) {
            Spacer()
            Image(systemName: "lock.fill")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            VStack(spacing: 8) {
                Text("Library Locked")
                    .font(.title)
                if let name = libraryName {
                    Text(name)
                        .font(.title3)
                        .foregroundStyle(.secondary)
                }
                if let fp = fingerprint {
                    Text("Key fingerprint: \(fp)")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .monospaced()
                }
            }
            Text("The encryption key for this library is not in the keyring. Enter the 64-character hex key to unlock.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 400)
            VStack(spacing: 16) {
                SecureField("Encryption key (64 hex characters)", text: $keyHex)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 400)
                    .monospaced()
                Button(action: unlock) {
                    if isUnlocking {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Unlock")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValidHex || isUnlocking)
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

    private func unlock() {
        isUnlocking = true
        error = nil
        Task.detached { [libraryId, keyHex] in
            do {
                try unlockLibrary(libraryId: libraryId, keyHex: keyHex)
                await MainActor.run {
                    isUnlocking = false
                    onUnlocked()
                }
            } catch {
                await MainActor.run {
                    isUnlocking = false
                    self.error = error.localizedDescription
                }
            }
        }
    }
}
