import Foundation
import SQLite3

enum DatabaseError: LocalizedError {
    case openFailed(String)
    case queryFailed(String)

    var errorDescription: String? {
        switch self {
        case .openFailed(let msg): "Failed to open database: \(msg)"
        case .queryFailed(let msg): "Query failed: \(msg)"
        }
    }
}

class DatabaseService {
    private var db: OpaquePointer?

    init(path: URL) throws {
        var db: OpaquePointer?
        let rc = sqlite3_open_v2(path.path, &db, SQLITE_OPEN_READONLY, nil)
        guard rc == SQLITE_OK else {
            let msg = db.flatMap { String(cString: sqlite3_errmsg($0)) } ?? "Unknown error"
            if let db { sqlite3_close(db) }
            throw DatabaseError.openFailed(msg)
        }
        self.db = db
    }

    deinit {
        if let db { sqlite3_close(db) }
    }

    // MARK: - Public Queries

    func allArtists() throws -> [Artist] {
        let sql = """
            SELECT a.id, a.name, a.sort_name
            FROM artists a
            WHERE EXISTS (SELECT 1 FROM album_artists aa WHERE aa.artist_id = a.id)
            ORDER BY COALESCE(a.sort_name, a.name) COLLATE NOCASE
            """
        return try query(sql: sql) { stmt in
            Artist(
                id: columnText(stmt, index: 0),
                name: columnText(stmt, index: 1),
                sortName: columnOptionalText(stmt, index: 2)
            )
        }
    }

    func allAlbums() throws -> [Album] {
        let sql = """
            SELECT al.id, al.title, al.year, al.cover_release_id, al.is_compilation,
                   GROUP_CONCAT(a.name, ', ') as artist_names
            FROM albums al
            LEFT JOIN album_artists aa ON aa.album_id = al.id
            LEFT JOIN artists a ON a.id = aa.artist_id
            GROUP BY al.id
            ORDER BY artist_names COLLATE NOCASE, al.year
            """
        return try query(sql: sql) { stmt in
            Album(
                id: columnText(stmt, index: 0),
                title: columnText(stmt, index: 1),
                year: columnOptionalInt(stmt, index: 2),
                coverReleaseId: columnOptionalText(stmt, index: 3),
                isCompilation: sqlite3_column_int(stmt, 4) != 0,
                artistNames: columnOptionalText(stmt, index: 5) ?? "Unknown Artist"
            )
        }
    }

    func albumsByArtist(artistId: String) throws -> [Album] {
        let sql = """
            SELECT al.id, al.title, al.year, al.cover_release_id, al.is_compilation,
                   GROUP_CONCAT(a2.name, ', ') as artist_names
            FROM albums al
            INNER JOIN album_artists aa ON aa.album_id = al.id AND aa.artist_id = ?1
            LEFT JOIN album_artists aa2 ON aa2.album_id = al.id
            LEFT JOIN artists a2 ON a2.id = aa2.artist_id
            GROUP BY al.id
            ORDER BY al.year
            """
        return try query(sql: sql, bind: { [self] stmt in
            bindText(stmt, index: 1, value: artistId)
        }) { stmt in
            Album(
                id: columnText(stmt, index: 0),
                title: columnText(stmt, index: 1),
                year: columnOptionalInt(stmt, index: 2),
                coverReleaseId: columnOptionalText(stmt, index: 3),
                isCompilation: sqlite3_column_int(stmt, 4) != 0,
                artistNames: columnOptionalText(stmt, index: 5) ?? "Unknown Artist"
            )
        }
    }

    func albumDetail(albumId: String) throws -> AlbumDetail? {
        // 1. Get album
        let albums: [Album] = try query(sql: """
            SELECT al.id, al.title, al.year, al.cover_release_id, al.is_compilation,
                   GROUP_CONCAT(a.name, ', ') as artist_names
            FROM albums al
            LEFT JOIN album_artists aa ON aa.album_id = al.id
            LEFT JOIN artists a ON a.id = aa.artist_id
            WHERE al.id = ?1
            GROUP BY al.id
            """, bind: { [self] stmt in
                bindText(stmt, index: 1, value: albumId)
            }) { stmt in
                Album(
                    id: columnText(stmt, index: 0),
                    title: columnText(stmt, index: 1),
                    year: columnOptionalInt(stmt, index: 2),
                    coverReleaseId: columnOptionalText(stmt, index: 3),
                    isCompilation: sqlite3_column_int(stmt, 4) != 0,
                    artistNames: columnOptionalText(stmt, index: 5) ?? "Unknown Artist"
                )
            }

        guard let album = albums.first else { return nil }

        // 2. Get album artists
        let artists: [Artist] = try query(sql: """
            SELECT a.id, a.name, a.sort_name
            FROM artists a
            INNER JOIN album_artists aa ON aa.artist_id = a.id
            WHERE aa.album_id = ?1
            ORDER BY aa.position
            """, bind: { [self] stmt in
                bindText(stmt, index: 1, value: albumId)
            }) { stmt in
                Artist(
                    id: columnText(stmt, index: 0),
                    name: columnText(stmt, index: 1),
                    sortName: columnOptionalText(stmt, index: 2)
                )
            }

        // 3. Get releases
        let releases: [Release] = try query(sql: """
            SELECT id, album_id, release_name, year, format, label
            FROM releases
            WHERE album_id = ?1
            ORDER BY year, release_name
            """, bind: { [self] stmt in
                bindText(stmt, index: 1, value: albumId)
            }) { stmt in
                let releaseId = columnText(stmt, index: 0)
                let tracks = (try? self.tracksForRelease(releaseId: releaseId, albumArtists: artists)) ?? []
                return Release(
                    id: releaseId,
                    albumId: columnText(stmt, index: 1),
                    releaseName: columnOptionalText(stmt, index: 2),
                    year: columnOptionalInt(stmt, index: 3),
                    format: columnOptionalText(stmt, index: 4),
                    label: columnOptionalText(stmt, index: 5),
                    tracks: tracks
                )
            }

        return AlbumDetail(album: album, artists: artists, releases: releases)
    }

    func tracksForRelease(releaseId: String, albumArtists: [Artist] = []) throws -> [Track] {
        let albumArtistNames = Set(albumArtists.map(\.name))

        return try query(sql: """
            SELECT t.id, t.release_id, t.title, t.disc_number, t.track_number, t.duration_ms,
                   GROUP_CONCAT(a.name, ', ') as track_artists
            FROM tracks t
            LEFT JOIN track_artists ta ON ta.track_id = t.id
            LEFT JOIN artists a ON a.id = ta.artist_id
            WHERE t.release_id = ?1
            GROUP BY t.id
            ORDER BY t.disc_number, t.track_number
            """, bind: { [self] stmt in
                bindText(stmt, index: 1, value: releaseId)
            }) { stmt in
                let rawArtists = columnOptionalText(stmt, index: 6)
                // Only show track artists if different from album artists
                let trackArtistNames: String?
                if let rawArtists {
                    let trackNames = Set(rawArtists.split(separator: ", ").map(String.init))
                    trackArtistNames = trackNames == albumArtistNames ? nil : rawArtists
                } else {
                    trackArtistNames = nil
                }

                return Track(
                    id: columnText(stmt, index: 0),
                    releaseId: columnText(stmt, index: 1),
                    title: columnText(stmt, index: 2),
                    discNumber: columnOptionalInt(stmt, index: 3),
                    trackNumber: columnOptionalInt(stmt, index: 4),
                    durationMs: columnOptionalInt(stmt, index: 5),
                    artistNames: trackArtistNames
                )
            }
    }

    func audioFormatForTrack(trackId: String) throws -> AudioFormat? {
        let results: [AudioFormat] = try query(
            sql: """
                SELECT af.id, af.track_id, af.content_type, af.flac_headers, af.needs_headers,
                       af.start_byte_offset, af.end_byte_offset, af.audio_data_start, af.file_id,
                       af.sample_rate, af.bits_per_sample
                FROM audio_formats af
                WHERE af.track_id = ?1
                """,
            bind: { [self] stmt in
                bindText(stmt, index: 1, value: trackId)
            }
        ) { stmt in
            AudioFormat(
                id: columnText(stmt, index: 0),
                trackId: columnText(stmt, index: 1),
                contentType: columnText(stmt, index: 2),
                flacHeaders: columnOptionalBlob(stmt, index: 3),
                needsHeaders: sqlite3_column_int(stmt, 4) != 0,
                startByteOffset: columnOptionalInt(stmt, index: 5),
                endByteOffset: columnOptionalInt(stmt, index: 6),
                audioDataStart: Int(sqlite3_column_int64(stmt, 7)),
                fileId: columnOptionalText(stmt, index: 8),
                sampleRate: Int(sqlite3_column_int64(stmt, 9)),
                bitsPerSample: Int(sqlite3_column_int64(stmt, 10))
            )
        }
        return results.first
    }

    // MARK: - Private Helpers

    private func query<T>(
        sql: String,
        bind: ((OpaquePointer?) -> Void)? = nil,
        map: (OpaquePointer?) throws -> T
    ) throws -> [T] {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
            let msg = db.flatMap { String(cString: sqlite3_errmsg($0)) } ?? "Unknown error"
            throw DatabaseError.queryFailed(msg)
        }
        defer { sqlite3_finalize(stmt) }

        bind?(stmt)

        var results: [T] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            try results.append(map(stmt))
        }
        return results
    }

    private func bindText(_ stmt: OpaquePointer?, index: Int32, value: String) {
        value.withCString { cStr in
            _ = sqlite3_bind_text(stmt, index, cStr, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
        }
    }

    private func columnText(_ stmt: OpaquePointer?, index: Int32) -> String {
        if let cStr = sqlite3_column_text(stmt, index) {
            return String(cString: cStr)
        }
        return ""
    }

    private func columnOptionalText(_ stmt: OpaquePointer?, index: Int32) -> String? {
        guard sqlite3_column_type(stmt, index) != SQLITE_NULL else { return nil }
        if let cStr = sqlite3_column_text(stmt, index) {
            return String(cString: cStr)
        }
        return nil
    }

    private func columnOptionalInt(_ stmt: OpaquePointer?, index: Int32) -> Int? {
        guard sqlite3_column_type(stmt, index) != SQLITE_NULL else { return nil }
        return Int(sqlite3_column_int64(stmt, index))
    }

    private func columnOptionalBlob(_ stmt: OpaquePointer?, index: Int32) -> Data? {
        guard sqlite3_column_type(stmt, index) != SQLITE_NULL else { return nil }
        guard let bytes = sqlite3_column_blob(stmt, index) else { return nil }
        let count = Int(sqlite3_column_bytes(stmt, index))
        return Data(bytes: bytes, count: count)
    }
}
