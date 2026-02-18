import Foundation

enum DeviceLinkError: LocalizedError {
    case invalidFormat
    case invalidKey(String)

    var errorDescription: String? {
        switch self {
        case .invalidFormat:
            "Invalid device link format"
        case .invalidKey(let msg):
            msg
        }
    }
}

protocol DeviceLinkParser {
    func parse(json: String) throws -> LibraryCredentials
}

/// Parses device link JSON directly in Swift.
/// Will be replaced by BaeCrypto FFI once the Rust library is compiled as an xcframework.
class DeviceLinkParserImpl: DeviceLinkParser {
    func parse(json: String) throws -> LibraryCredentials {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: String],
              let proxyUrl = obj["proxy_url"],
              let encKeyB64 = obj["encryption_key"],
              let signKeyB64 = obj["signing_key"],
              let libraryId = obj["library_id"]
        else {
            throw DeviceLinkError.invalidFormat
        }

        guard let encKey = Data(base64URLEncoded: encKeyB64), encKey.count == 32 else {
            throw DeviceLinkError.invalidKey("encryption key must be 32 bytes")
        }

        guard let signKey = Data(base64URLEncoded: signKeyB64), signKey.count == 64 else {
            throw DeviceLinkError.invalidKey("signing key must be 64 bytes")
        }

        return LibraryCredentials(
            proxyUrl: proxyUrl,
            encryptionKey: encKey,
            signingKey: signKey,
            libraryId: libraryId
        )
    }
}
