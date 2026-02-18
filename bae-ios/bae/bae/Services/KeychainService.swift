import Foundation
import Security

enum KeychainError: LocalizedError {
    case saveFailed(OSStatus)
    case deleteFailed(OSStatus)
    case encodingFailed

    var errorDescription: String? {
        switch self {
        case .saveFailed(let status):
            "Failed to save to Keychain (status \(status))"
        case .deleteFailed(let status):
            "Failed to delete from Keychain (status \(status))"
        case .encodingFailed:
            "Failed to encode credentials"
        }
    }
}

class KeychainService {
    private let service = "fm.bae.bae"
    private let account = "library_credentials"

    func saveCredentials(_ creds: LibraryCredentials) throws {
        let payload: [String: String] = [
            "proxy_url": creds.proxyUrl,
            "encryption_key": creds.encryptionKey.base64URLEncodedString(),
            "signing_key": creds.signingKey.base64URLEncodedString(),
            "library_id": creds.libraryId,
        ]

        guard let data = try? JSONSerialization.data(withJSONObject: payload) else {
            throw KeychainError.encodingFailed
        }

        // Delete any existing entry first
        let deleteQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(deleteQuery as CFDictionary)

        let addQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock,
        ]

        let status = SecItemAdd(addQuery as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status)
        }
    }

    func loadCredentials() -> LibraryCredentials? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess,
              let data = result as? Data,
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: String],
              let proxyUrl = obj["proxy_url"],
              let encKeyB64 = obj["encryption_key"],
              let signKeyB64 = obj["signing_key"],
              let libraryId = obj["library_id"],
              let encKey = Data(base64URLEncoded: encKeyB64),
              let signKey = Data(base64URLEncoded: signKeyB64)
        else {
            return nil
        }

        return LibraryCredentials(
            proxyUrl: proxyUrl,
            encryptionKey: encKey,
            signingKey: signKey,
            libraryId: libraryId
        )
    }

    func deleteCredentials() throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]

        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError.deleteFailed(status)
        }
    }
}
