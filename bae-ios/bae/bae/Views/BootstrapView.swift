import SwiftUI

struct BootstrapView: View {
    let bootstrapService: BootstrapService
    let onComplete: (BootstrapResult) -> Void
    let onError: (String) -> Void
    @State private var retryTrigger = UUID()

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            switch bootstrapService.progress {
            case .idle:
                Text("Preparing to sync...")
                    .font(.headline)
                ProgressView()
            case .downloading(let detail), .decrypting(let detail):
                Text("Syncing library")
                    .font(.headline)
                ProgressView()
                Text(detail)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            case .done:
                Image(systemName: "checkmark.circle")
                    .font(.system(size: 48))
                    .foregroundStyle(.green)
                Text("Library synced")
                    .font(.headline)
            case .failed(let msg):
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 48))
                    .foregroundStyle(.orange)
                Text("Sync failed")
                    .font(.headline)
                Text(msg)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 32)
                Button("Try Again") {
                    bootstrapService.progress = .idle
                    retryTrigger = UUID()
                }
                .buttonStyle(.bordered)
            }

            Spacer()
        }
        .task(id: retryTrigger) {
            do {
                let result = try await bootstrapService.bootstrap()
                onComplete(result)
            } catch {
                bootstrapService.progress = .failed(error.localizedDescription)
                onError(error.localizedDescription)
            }
        }
    }
}
