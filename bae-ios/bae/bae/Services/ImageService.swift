import UIKit

actor ImageService {
    private var memoryCache: [String: UIImage] = [:]
    private let cloudClient: CloudHomeClient?
    private let crypto: CryptoService
    private let encryptionKey: Data

    init(cloudClient: CloudHomeClient?, crypto: CryptoService, encryptionKey: Data) {
        self.cloudClient = cloudClient
        self.crypto = crypto
        self.encryptionKey = encryptionKey
    }

    func image(for imageId: String) async -> UIImage? {
        if let cached = memoryCache[imageId] {
            return cached
        }

        let diskPath = Self.cachePath(for: imageId)
        if let data = try? Data(contentsOf: diskPath),
           let image = UIImage(data: data)
        {
            memoryCache[imageId] = image
            return image
        }

        guard let client = cloudClient else { return nil }

        // Cloud key uses hex-based path: images/{ab}/{cd}/{id}
        // where ab/cd are first 4 hex chars of the UUID with dashes stripped
        let hex = imageId.replacingOccurrences(of: "-", with: "")
        guard hex.count >= 4 else { return nil }
        let ab = String(hex.prefix(2))
        let cd = String(hex.dropFirst(2).prefix(2))
        let key = "images/\(ab)/\(cd)/\(imageId)"

        do {
            let encrypted = try await client.readBlob(key: key)
            let decrypted = try crypto.decryptFile(ciphertext: encrypted, key: encryptionKey)

            guard let image = UIImage(data: decrypted) else { return nil }

            // Save to disk cache
            let dir = diskPath.deletingLastPathComponent()
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try? decrypted.write(to: diskPath)

            memoryCache[imageId] = image
            return image
        } catch {
            return nil
        }
    }

    func clearMemoryCache() {
        memoryCache.removeAll()
    }

    static func cachePath(for imageId: String) -> URL {
        BootstrapService.imageCachePath().appendingPathComponent(imageId)
    }
}
