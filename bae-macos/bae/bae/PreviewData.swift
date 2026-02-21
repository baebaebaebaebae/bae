import Foundation

// Shared sample data for SwiftUI #Preview blocks.
// Album/artist/track names match bae-mocks/fixtures/data.json.
// Cover art loaded from bae-mocks/public/covers/ via file:// URLs.

enum PreviewData {
    private static func coverURL(_ artist: String, _ title: String) -> URL? {
        let slug = { (s: String) in
            s.lowercased()
                .replacingOccurrences(of: " ", with: "-")
                .replacingOccurrences(of: "'", with: "")
        }
        let filename = "\(slug(artist))_\(slug(title)).png"
        // Walk up from the source file to find the repo root
        let source = URL(fileURLWithPath: #filePath)
        let repoRoot = source
            .deletingLastPathComponent() // bae/
            .deletingLastPathComponent() // bae/
            .deletingLastPathComponent() // bae/
            .deletingLastPathComponent() // bae-macos/
        let coverPath = repoRoot.appendingPathComponent("bae-mocks/public/covers/\(filename)")
        return FileManager.default.fileExists(atPath: coverPath.path) ? coverPath : nil
    }

    // MARK: - Albums

    static let albums: [AlbumCardViewModel] = [
        AlbumCardViewModel(id: "a-01", title: "Neon Frequencies", artistNames: "The Midnight Signal", year: 2023, coverArtURL: coverURL("The Midnight Signal", "Neon Frequencies")),
        AlbumCardViewModel(id: "a-02", title: "Pacific Standard", artistNames: "Glass Harbor", year: 2019, coverArtURL: coverURL("Glass Harbor", "Pacific Standard")),
        AlbumCardViewModel(id: "a-03", title: "Proof by Induction", artistNames: "Velvet Mathematics", year: 2021, coverArtURL: coverURL("Velvet Mathematics", "Proof by Induction")),
        AlbumCardViewModel(id: "a-04", title: "Seconds", artistNames: "The Borrowed Time", year: 2022, coverArtURL: coverURL("The Borrowed Time", "Seconds")),
        AlbumCardViewModel(id: "a-05", title: "Window Sill", artistNames: "Apartment Garden", year: 2020, coverArtURL: coverURL("Apartment Garden", "Window Sill")),
        AlbumCardViewModel(id: "a-06", title: "Fuel Weight", artistNames: "The Cold Equations", year: 2018, coverArtURL: coverURL("The Cold Equations", "Fuel Weight")),
        AlbumCardViewModel(id: "a-07", title: "Tomorrow's Forecast", artistNames: "Newspaper Weather", year: 2023, coverArtURL: coverURL("Newspaper Weather", "Tomorrows Forecast")),
        AlbumCardViewModel(id: "a-08", title: "Alphabetical", artistNames: "The Filing Cabinets", year: 2017, coverArtURL: coverURL("The Filing Cabinets", "Alphabetical")),
        AlbumCardViewModel(id: "a-09", title: "Level 4", artistNames: "Parking Structure", year: 2021, coverArtURL: coverURL("Parking Structure", "Level 4")),
        AlbumCardViewModel(id: "a-10", title: "Dial Tone", artistNames: "The Last Payphone", year: 2019, coverArtURL: coverURL("The Last Payphone", "Dial Tone")),
        AlbumCardViewModel(id: "a-11", title: "Set Theory", artistNames: "Velvet Mathematics", year: 2019, coverArtURL: coverURL("Velvet Mathematics", "Set Theory")),
        AlbumCardViewModel(id: "a-12", title: "Interest", artistNames: "The Borrowed Time", year: 2020, coverArtURL: coverURL("The Borrowed Time", "Interest")),
        AlbumCardViewModel(id: "a-13", title: "Grow Light", artistNames: "Apartment Garden", year: 2022, coverArtURL: coverURL("Apartment Garden", "Grow Light")),
        AlbumCardViewModel(id: "a-14", title: "Landlocked", artistNames: "Glass Harbor", year: 2022, coverArtURL: coverURL("Glass Harbor", "Landlocked")),
        AlbumCardViewModel(id: "a-15", title: "Express", artistNames: "The Checkout Lane", year: 2023, coverArtURL: coverURL("The Checkout Lane", "Express")),
        AlbumCardViewModel(id: "a-16", title: "Floors 1-12", artistNames: "Stairwell Echo", year: 2018, coverArtURL: coverURL("Stairwell Echo", "Floors 1-12")),
        AlbumCardViewModel(id: "a-17", title: "Your Number", artistNames: "The Waiting Room", year: 2021, coverArtURL: coverURL("The Waiting Room", "Your Number")),
        AlbumCardViewModel(id: "a-18", title: "Back Page", artistNames: "Newspaper Weather", year: 2021, coverArtURL: coverURL("Newspaper Weather", "Back Page")),
        AlbumCardViewModel(id: "a-19", title: "Mission Control", artistNames: "The Cold Equations", year: 2021, coverArtURL: coverURL("The Cold Equations", "Mission Control")),
        AlbumCardViewModel(id: "a-20", title: "Collated", artistNames: "Copy Machine", year: 2020, coverArtURL: coverURL("Copy Machine", "Collated")),
    ]

    // MARK: - Queue

    static let queueItems: [QueueItemViewModel] = [
        QueueItemViewModel(id: "t-01", title: "Static Dreams", artistNames: "The Midnight Signal", albumTitle: "Neon Frequencies", durationMs: 210_000, coverArtURL: coverURL("The Midnight Signal", "Neon Frequencies")),
        QueueItemViewModel(id: "t-02", title: "Frequency Drift", artistNames: "The Midnight Signal", albumTitle: "Neon Frequencies", durationMs: 240_000, coverArtURL: coverURL("The Midnight Signal", "Neon Frequencies")),
        QueueItemViewModel(id: "t-03", title: "Tide Pool", artistNames: "Glass Harbor", albumTitle: "Pacific Standard", durationMs: 198_000, coverArtURL: coverURL("Glass Harbor", "Pacific Standard")),
        QueueItemViewModel(id: "t-04", title: "Harbor Lights", artistNames: "Glass Harbor", albumTitle: "Pacific Standard", durationMs: 225_000, coverArtURL: coverURL("Glass Harbor", "Pacific Standard")),
        QueueItemViewModel(id: "t-05", title: "Axiom", artistNames: "Velvet Mathematics", albumTitle: "Proof by Induction", durationMs: 187_000, coverArtURL: coverURL("Velvet Mathematics", "Proof by Induction")),
    ]

    // MARK: - Now Playing

    static let nowPlayingTitle = "Broadcast"
    static let nowPlayingArtist = "The Midnight Signal"
    static let nowPlayingAlbum = "Neon Frequencies"
    static let nowPlayingCoverURL = coverURL("The Midnight Signal", "Neon Frequencies")

    // MARK: - Album Details

    private static func makeTracks(_ names: [String], artist: String) -> [BridgeTrack] {
        names.enumerated().map { index, name in
            // Deterministic durations: 170-340s based on track index
            let durationMs = Int64((170 + (index * 37) % 170) * 1000)
            return BridgeTrack(
                id: "t-\(index + 1)",
                title: name,
                discNumber: 1,
                trackNumber: Int32(index + 1),
                durationMs: durationMs,
                artistNames: artist,
            )
        }
    }

    private static func makeDetail(
        id: String, title: String, artist: String, year: Int32,
        tracks: [String], format: String,
    ) -> BridgeAlbumDetail {
        BridgeAlbumDetail(
            album: BridgeAlbum(
                id: id, title: title, year: year,
                isCompilation: false, coverReleaseId: nil,
                artistNames: artist,
            ),
            artists: [BridgeArtist(id: "ar-\(id)", name: artist)],
            releases: [
                BridgeRelease(
                    id: "rel-\(id)", albumId: id, releaseName: nil,
                    year: year, format: format, label: nil,
                    catalogNumber: nil, country: nil,
                    managedLocally: true, managedInCloud: false,
                    unmanagedPath: nil,
                    tracks: makeTracks(tracks, artist: artist),
                    files: [],
                ),
            ],
        )
    }

    static let albumDetails: [String: BridgeAlbumDetail] = {
        let details = [
            makeDetail(id: "a-01", title: "Neon Frequencies", artist: "The Midnight Signal", year: 2023,
                       tracks: ["Broadcast", "Static Dreams", "Frequency Drift", "Night Transmission", "Signal Lost", "Airwave", "Carrier Wave", "Sign Off"], format: "Digital"),
            makeDetail(id: "a-02", title: "Pacific Standard", artist: "Glass Harbor", year: 2019,
                       tracks: ["Coastal", "Tide Pool", "Harbor Lights", "Salt Air", "Driftwood", "Fog Horn", "Pier 17", "Last Ferry"], format: "Vinyl"),
            makeDetail(id: "a-03", title: "Proof by Induction", artist: "Velvet Mathematics", year: 2021,
                       tracks: ["Axiom", "Recursive", "Limit Theorem", "Derivative", "Integral", "Convergence", "QED"], format: "CD"),
            makeDetail(id: "a-04", title: "Seconds", artist: "The Borrowed Time", year: 2022,
                       tracks: ["Tick", "Borrowed", "Overdue", "Extension", "Final Notice", "Grace Period"], format: "CD"),
            makeDetail(id: "a-05", title: "Window Sill", artist: "Apartment Garden", year: 2020,
                       tracks: ["Basil", "Morning Light", "Terracotta", "Propagation", "Root Bound", "Water Day", "New Growth"], format: "Digital"),
            makeDetail(id: "a-06", title: "Fuel Weight", artist: "The Cold Equations", year: 2018,
                       tracks: ["Launch Window", "Trajectory", "Orbital Decay", "Reentry", "Terminal Velocity", "Escape", "Gravity Well"], format: "Vinyl"),
            makeDetail(id: "a-07", title: "Tomorrow's Forecast", artist: "Newspaper Weather", year: 2023,
                       tracks: ["Partly Cloudy", "High Pressure", "Cold Front", "Scattered Showers", "Clearing Skies", "Weekend Outlook"], format: "Digital"),
            makeDetail(id: "a-08", title: "Alphabetical", artist: "The Filing Cabinets", year: 2017,
                       tracks: ["A-D", "E-H", "I-L", "M-P", "Q-T", "U-Z", "Miscellaneous"], format: "CD"),
            makeDetail(id: "a-09", title: "Level 4", artist: "Parking Structure", year: 2021,
                       tracks: ["Entrance", "Spiral Up", "Compact Only", "Reserved", "Exit Ticket", "Night Rate"], format: "Digital"),
            makeDetail(id: "a-10", title: "Dial Tone", artist: "The Last Payphone", year: 2019,
                       tracks: ["Insert Coin", "Area Code", "Long Distance", "Collect Call", "Busy Signal", "Disconnected"], format: "Cassette"),
            makeDetail(id: "a-11", title: "Set Theory", artist: "Velvet Mathematics", year: 2019,
                       tracks: ["Union", "Intersection", "Complement", "Subset", "Empty Set", "Cardinality"], format: "CD"),
            makeDetail(id: "a-12", title: "Interest", artist: "The Borrowed Time", year: 2020,
                       tracks: ["Principal", "Compound", "Balloon Payment", "Amortization", "Default", "Refinance"], format: "Digital"),
            makeDetail(id: "a-13", title: "Grow Light", artist: "Apartment Garden", year: 2022,
                       tracks: ["Spectrum", "Photosynthesis", "Chlorophyll", "Dormancy", "Spring Bloom", "Perennial"], format: "Vinyl"),
            makeDetail(id: "a-14", title: "Landlocked", artist: "Glass Harbor", year: 2022,
                       tracks: ["Dry Dock", "Anchor", "Barnacles", "Rust", "Restoration", "Launch Day", "Open Water"], format: "CD"),
            makeDetail(id: "a-15", title: "Express", artist: "The Checkout Lane", year: 2023,
                       tracks: ["15 Items", "Price Check", "Coupon", "Self Scan", "Bagging Area", "Receipt"], format: "Digital"),
            makeDetail(id: "a-16", title: "Floors 1-12", artist: "Stairwell Echo", year: 2018,
                       tracks: ["Lobby", "Ascent", "Landing", "Fire Door", "Roof Access"], format: "Vinyl"),
            makeDetail(id: "a-17", title: "Your Number", artist: "The Waiting Room", year: 2021,
                       tracks: ["Take a Ticket", "Now Serving", "Please Wait", "Next Window", "Closed"], format: "CD"),
            makeDetail(id: "a-18", title: "Back Page", artist: "Newspaper Weather", year: 2021,
                       tracks: ["Classifieds", "Obituaries", "Comics", "Crossword", "Horoscope", "Editorial"], format: "Digital"),
            makeDetail(id: "a-19", title: "Mission Control", artist: "The Cold Equations", year: 2021,
                       tracks: ["Countdown", "Ignition", "Max Q", "MECO", "Orbit Achieved", "Houston"], format: "CD"),
            makeDetail(id: "a-20", title: "Collated", artist: "Copy Machine", year: 2020,
                       tracks: ["Warm Up", "Paper Jam", "Toner Low", "Duplex", "Staple", "Output Tray"], format: "Digital"),
        ]
        return Dictionary(uniqueKeysWithValues: details.map { ($0.album.id, $0) })
    }()
}
