import Foundation

struct Artist: Identifiable, Hashable {
    let id: String
    let name: String
    let sortName: String?
}

struct Album: Identifiable, Hashable {
    let id: String
    let title: String
    let year: Int?
    let coverReleaseId: String?
    let isCompilation: Bool
    let artistNames: String
}

struct AlbumDetail {
    let album: Album
    let artists: [Artist]
    let releases: [Release]
}

struct Release: Identifiable, Hashable {
    let id: String
    let albumId: String
    let releaseName: String?
    let year: Int?
    let format: String?
    let label: String?
    let tracks: [Track]
}

struct Track: Identifiable, Hashable {
    let id: String
    let releaseId: String
    let title: String
    let discNumber: Int?
    let trackNumber: Int?
    let durationMs: Int?
    let artistNames: String?
}
