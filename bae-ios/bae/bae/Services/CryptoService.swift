import Foundation

protocol CryptoService {
    func decryptFile(ciphertext: Data, key: Data) throws -> Data
    func deriveReleaseKey(masterKey: Data, releaseId: String) throws -> Data
}

enum CryptoServiceError: LocalizedError {
    case decryptionFailed(String)
    case invalidKey(String)

    var errorDescription: String? {
        switch self {
        case .decryptionFailed(let msg): "Decryption failed: \(msg)"
        case .invalidKey(let msg): "Invalid key: \(msg)"
        }
    }
}

/// Placeholder that returns input unchanged. Replace with BaeCrypto FFI later.
class PlaceholderCryptoService: CryptoService {
    func decryptFile(ciphertext: Data, key: Data) throws -> Data {
        return ciphertext
    }

    func deriveReleaseKey(masterKey: Data, releaseId: String) throws -> Data {
        return masterKey
    }
}
