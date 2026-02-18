import Foundation

enum BootstrapError: LocalizedError {
    case noSnapshot
    case downloadFailed(String)
    case decryptionFailed(String)
    case invalidDatabase(String)
    case invalidMeta(String)

    var errorDescription: String? {
        switch self {
        case .noSnapshot: "No snapshot available. The library may be empty."
        case .downloadFailed(let msg): "Download failed: \(msg)"
        case .decryptionFailed(let msg): "Decryption failed: \(msg)"
        case .invalidDatabase(let msg): "Invalid database: \(msg)"
        case .invalidMeta(let msg): "Invalid metadata: \(msg)"
        }
    }
}

struct SyncCursors: Codable {
    let cursors: [String: Int]
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case cursors
        case createdAt = "created_at"
    }
}

@Observable
class BootstrapService {
    var progress: BootstrapProgress = .idle

    private let cloudClient: CloudHomeClient
    private let crypto: CryptoService
    private let encryptionKey: Data

    init(cloudClient: CloudHomeClient, crypto: CryptoService, encryptionKey: Data) {
        self.cloudClient = cloudClient
        self.crypto = crypto
        self.encryptionKey = encryptionKey
    }

    func bootstrap() async throws -> BootstrapResult {
        progress = .downloading(detail: "Downloading database snapshot...")

        // 1. Download snapshot.db.enc
        let snapshotEncrypted: Data
        do {
            snapshotEncrypted = try await cloudClient.readBlob(key: "snapshot.db.enc")
        } catch let error as CloudHomeError {
            if case .notFound = error {
                throw BootstrapError.noSnapshot
            }
            throw BootstrapError.downloadFailed(error.localizedDescription)
        }

        // 2. Decrypt snapshot
        progress = .decrypting(detail: "Decrypting database...")
        let snapshotData: Data
        do {
            snapshotData = try crypto.decryptFile(ciphertext: snapshotEncrypted, key: encryptionKey)
        } catch {
            throw BootstrapError.decryptionFailed(error.localizedDescription)
        }

        // 3. Validate it's a SQLite database (starts with "SQLite format 3\0")
        let sqliteMagic = Data("SQLite format 3\0".utf8)
        if snapshotData.count < sqliteMagic.count || snapshotData.prefix(sqliteMagic.count) != sqliteMagic {
            throw BootstrapError.invalidDatabase("Not a valid SQLite database")
        }

        // 4. Write to documents directory
        let dbPath = Self.databasePath()
        let dbDir = dbPath.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: dbDir, withIntermediateDirectories: true)
        try snapshotData.write(to: dbPath)

        // 5. Download and decrypt snapshot_meta.json.enc
        progress = .downloading(detail: "Downloading sync metadata...")
        var cursors: [String: Int] = [:]
        do {
            let metaEncrypted = try await cloudClient.readBlob(key: "snapshot_meta.json.enc")
            let metaData = try crypto.decryptFile(ciphertext: metaEncrypted, key: encryptionKey)
            let syncCursors = try JSONDecoder().decode(SyncCursors.self, from: metaData)
            cursors = syncCursors.cursors
        } catch {
            // Non-fatal: we can still use the DB without sync cursors
        }

        // 6. Save sync cursors
        let cursorsPath = Self.syncCursorsPath()
        if let cursorsData = try? JSONEncoder().encode(cursors) {
            try? cursorsData.write(to: cursorsPath)
        }

        // 7. Download library images
        progress = .downloading(detail: "Downloading images...")
        let imageCount = try await downloadImages()

        progress = .done

        return BootstrapResult(
            databasePath: dbPath,
            syncCursors: cursors,
            imageCount: imageCount
        )
    }

    private func downloadImages() async throws -> Int {
        let imageKeys: [String]
        do {
            imageKeys = try await cloudClient.listKeys(prefix: "images/")
        } catch {
            return 0 // Non-fatal
        }

        var downloaded = 0
        for (index, key) in imageKeys.enumerated() {
            progress = .downloading(detail: "Downloading images (\(index + 1)/\(imageKeys.count))...")

            do {
                let encrypted = try await cloudClient.readBlob(key: key)
                let decrypted = try crypto.decryptFile(ciphertext: encrypted, key: encryptionKey)

                // Extract image ID from key path (e.g., "images/ab/cd/image-id" -> "image-id")
                let imageId = URL(string: key)?.lastPathComponent ?? key
                let imagePath = Self.imageCachePath().appendingPathComponent(imageId)
                try FileManager.default.createDirectory(
                    at: imagePath.deletingLastPathComponent(), withIntermediateDirectories: true)
                try decrypted.write(to: imagePath)
                downloaded += 1
            } catch {
                // Non-fatal: skip individual image failures
                continue
            }
        }

        return downloaded
    }

    // MARK: - Paths

    static func databasePath() -> URL {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        return docs.appendingPathComponent("bae/library.db")
    }

    static func syncCursorsPath() -> URL {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        return docs.appendingPathComponent("bae/sync_cursors.json")
    }

    static func imageCachePath() -> URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        return caches.appendingPathComponent("bae/images")
    }
}

struct BootstrapResult {
    let databasePath: URL
    let syncCursors: [String: Int]
    let imageCount: Int
}

enum BootstrapProgress: Equatable {
    case idle
    case downloading(detail: String)
    case decrypting(detail: String)
    case done
    case failed(String)
}
