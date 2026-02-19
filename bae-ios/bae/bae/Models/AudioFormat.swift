import Foundation

struct AudioFormat {
    let id: String
    let trackId: String
    let contentType: String
    let flacHeaders: Data?
    let needsHeaders: Bool
    let startByteOffset: Int?
    let endByteOffset: Int?
    let audioDataStart: Int
    let fileId: String?
    let sampleRate: Int
    let bitsPerSample: Int
}
