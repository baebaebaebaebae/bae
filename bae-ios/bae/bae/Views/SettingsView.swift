import SwiftUI

struct SettingsView: View {
    let credentials: LibraryCredentials
    let syncService: SyncService?
    let playbackService: PlaybackService?
    let onUnlink: () -> Void
    @State private var showUnlinkConfirmation = false
    @State private var storageInfo: StorageInfo?
    @State private var audioCacheSize: String = "Calculating..."

    var body: some View {
        NavigationStack {
            List {
                Section("Library") {
                    LabeledContent("Library ID", value: credentials.libraryId)
                    LabeledContent("Server", value: credentials.proxyUrl)
                }
                Section("Sync") {
                    if let syncService {
                        LabeledContent("Status") {
                            syncStatusText(syncService.status)
                        }
                        if let lastSync = syncService.lastSyncTime {
                            LabeledContent("Last sync", value: lastSync, format: .relative(presentation: .named))
                        }
                        Button("Sync Now") {
                            Task { await syncService.sync() }
                        }
                        .disabled(isSyncing(syncService.status))
                    } else {
                        Text("Sync not available")
                            .foregroundStyle(.secondary)
                    }
                }
                Section("Storage") {
                    if let info = storageInfo {
                        LabeledContent("Database", value: info.databaseSize)
                        LabeledContent("Images", value: info.imagesSize)
                        LabeledContent("Total", value: info.totalSize)
                    } else {
                        Text("Calculating...")
                            .foregroundStyle(.secondary)
                    }
                    if let playbackService {
                        LabeledContent("Audio Cache", value: audioCacheSize)
                        Button("Clear Audio Cache") {
                            playbackService.clearAudioCache()
                            audioCacheSize = formatBytes(0)
                        }
                    }
                }
                Section("About") {
                    LabeledContent(
                        "Version",
                        value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String
                            ?? "Unknown")
                    LabeledContent(
                        "Build",
                        value: Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "Unknown"
                    )
                }
                Section {
                    Button("Unlink Library", role: .destructive) {
                        showUnlinkConfirmation = true
                    }
                }
            }
            .navigationTitle("Settings")
            .confirmationDialog(
                "Unlink Library?",
                isPresented: $showUnlinkConfirmation,
                titleVisibility: .visible
            ) {
                Button("Unlink", role: .destructive) {
                    onUnlink()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(
                    "This will remove the local database, cached images, and credentials. You can re-link by scanning the QR code again."
                )
            }
            .task {
                storageInfo = calculateStorageInfo()
                if let playbackService {
                    audioCacheSize = formatBytes(playbackService.audioCacheSize())
                }
            }
        }
    }

    @ViewBuilder
    private func syncStatusText(_ status: SyncStatus) -> some View {
        switch status {
        case .idle:
            Text("Idle").foregroundStyle(.secondary)
        case .checking:
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                Text("Checking...")
            }
        case .syncing(let detail):
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                Text(detail)
            }
        case .done(let count):
            Text("Done (\(count) new images)")
        case .failed(let msg):
            Text(msg).foregroundStyle(.red)
        case .upToDate:
            Text("Up to date").foregroundStyle(.secondary)
        }
    }

    private func isSyncing(_ status: SyncStatus) -> Bool {
        switch status {
        case .checking, .syncing: return true
        default: return false
        }
    }

    private func calculateStorageInfo() -> StorageInfo {
        let fm = FileManager.default
        let dbSize = fileSize(at: BootstrapService.databasePath())
        let imagesSize = directorySize(at: BootstrapService.imageCachePath(), fm: fm)
        return StorageInfo(
            databaseSize: formatBytes(dbSize),
            imagesSize: formatBytes(imagesSize),
            totalSize: formatBytes(dbSize + imagesSize)
        )
    }

    private func fileSize(at url: URL) -> Int64 {
        (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int64) ?? 0
    }

    private func directorySize(at url: URL, fm: FileManager) -> Int64 {
        guard let enumerator = fm.enumerator(at: url, includingPropertiesForKeys: [.fileSizeKey])
        else { return 0 }
        var total: Int64 = 0
        for case let fileURL as URL in enumerator {
            if let size = try? fileURL.resourceValues(forKeys: [.fileSizeKey]).fileSize {
                total += Int64(size)
            }
        }
        return total
    }

    private func formatBytes(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: bytes)
    }
}

struct StorageInfo {
    let databaseSize: String
    let imagesSize: String
    let totalSize: String
}
