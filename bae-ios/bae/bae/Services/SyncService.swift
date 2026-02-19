import Foundation

enum SyncStatus: Equatable {
    case idle
    case checking
    case syncing(String)
    case done(Int)  // images downloaded
    case failed(String)
    case upToDate
}

@Observable
class SyncService {
    private(set) var status: SyncStatus = .idle
    private(set) var lastSyncTime: Date?

    private let cloudClient: CloudHomeClient
    private let crypto: CryptoService
    private let encryptionKey: Data
    private var syncTimer: Timer?

    init(cloudClient: CloudHomeClient, crypto: CryptoService, encryptionKey: Data) {
        self.cloudClient = cloudClient
        self.crypto = crypto
        self.encryptionKey = encryptionKey
    }

    func startPeriodicSync() {
        stopPeriodicSync()

        Task { await sync() }

        syncTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            guard let self else { return }
            Task { await self.sync() }
        }
    }

    func stopPeriodicSync() {
        syncTimer?.invalidate()
        syncTimer = nil
    }

    func sync() async {
        guard status != .checking, !isSyncing else { return }

        status = .checking

        do {
            // 1. Download and decrypt remote snapshot metadata
            let remoteMetaEncrypted = try await cloudClient.readBlob(key: "snapshot_meta.json.enc")
            let remoteMetaData = try crypto.decryptFile(ciphertext: remoteMetaEncrypted, key: encryptionKey)
            let remoteMeta = try JSONDecoder().decode(SyncCursors.self, from: remoteMetaData)

            // 2. Load local cursors and compare
            let localCursors = loadLocalCursors()
            if !hasNewerData(remote: remoteMeta.cursors, local: localCursors) {
                status = .upToDate
                lastSyncTime = Date()
                return
            }

            // 3. Download fresh snapshot
            status = .syncing("Downloading database...")
            let snapshotEncrypted = try await cloudClient.readBlob(key: "snapshot.db.enc")

            // 4. Decrypt and validate
            status = .syncing("Decrypting...")
            let snapshotData = try crypto.decryptFile(ciphertext: snapshotEncrypted, key: encryptionKey)
            let sqliteMagic = Data("SQLite format 3\0".utf8)
            guard snapshotData.count >= sqliteMagic.count,
                  snapshotData.prefix(sqliteMagic.count) == sqliteMagic
            else {
                status = .failed("Invalid database in snapshot")
                return
            }

            // 5. Atomic replace of the database file
            let dbPath = BootstrapService.databasePath()
            let tempPath = dbPath.deletingLastPathComponent().appendingPathComponent("library.db.tmp")
            try snapshotData.write(to: tempPath)

            let fm = FileManager.default
            if fm.fileExists(atPath: dbPath.path) {
                try fm.removeItem(at: dbPath)
            }
            try fm.moveItem(at: tempPath, to: dbPath)

            // 6. Save updated cursors
            saveCursors(remoteMeta.cursors)

            // 7. Download new images
            status = .syncing("Updating images...")
            let imageCount = try await downloadNewImages()

            status = .done(imageCount)
            lastSyncTime = Date()
        } catch let error as CloudHomeError {
            if case .notFound = error {
                status = .upToDate
                lastSyncTime = Date()
            } else {
                status = .failed(error.localizedDescription)
            }
        } catch {
            status = .failed(error.localizedDescription)
        }
    }

    // MARK: - Private

    private var isSyncing: Bool {
        if case .syncing = status { return true }
        return false
    }

    private func hasNewerData(remote: [String: Int], local: [String: Int]) -> Bool {
        for (deviceId, remoteSeq) in remote {
            let localSeq = local[deviceId] ?? 0
            if remoteSeq > localSeq {
                return true
            }
        }
        return false
    }

    private func loadLocalCursors() -> [String: Int] {
        let path = BootstrapService.syncCursorsPath()
        guard let data = try? Data(contentsOf: path),
              let cursors = try? JSONDecoder().decode([String: Int].self, from: data)
        else {
            return [:]
        }
        return cursors
    }

    private func saveCursors(_ cursors: [String: Int]) {
        let path = BootstrapService.syncCursorsPath()
        if let data = try? JSONEncoder().encode(cursors) {
            try? data.write(to: path)
        }
    }

    private func downloadNewImages() async throws -> Int {
        let imageKeys: [String]
        do {
            imageKeys = try await cloudClient.listKeys(prefix: "images/")
        } catch {
            return 0
        }

        var downloaded = 0
        for key in imageKeys {
            let imageId = URL(string: key)?.lastPathComponent ?? key
            let cachePath = BootstrapService.imageCachePath().appendingPathComponent(imageId)

            if FileManager.default.fileExists(atPath: cachePath.path) {
                continue
            }

            do {
                let encrypted = try await cloudClient.readBlob(key: key)
                let decrypted = try crypto.decryptFile(ciphertext: encrypted, key: encryptionKey)

                let dir = cachePath.deletingLastPathComponent()
                try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                try decrypted.write(to: cachePath)
                downloaded += 1
            } catch {
                continue
            }
        }

        return downloaded
    }
}
