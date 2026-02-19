import SwiftUI

struct ImportView: View {
    let appService: AppService

    @State private var selectedCandidate: BridgeImportCandidate?
    @State private var searchResults: [BridgeMetadataResult] = []
    @State private var isSearching = false
    @State private var searchArtist = ""
    @State private var searchAlbum = ""
    @State private var candidateFiles: BridgeCandidateFiles?

    // Search tabs
    enum SearchTab {
        case general
        case catalogNumber
        case barcode
    }

    @State private var searchTab: SearchTab = .general
    @State private var searchCatalog = ""
    @State private var searchBarcode = ""

    var body: some View {
        if appService.scanResults.isEmpty {
            emptyState
        } else {
            HSplitView {
                candidateList
                    .frame(minWidth: 200, idealWidth: 250, maxWidth: 350)
                if let candidate = selectedCandidate {
                    metadataSearchView(for: candidate)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ContentUnavailableView(
                        "Select a folder",
                        systemImage: "folder",
                        description: Text("Choose a scanned folder to search for metadata")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: 12) {
            Button(action: { openFolderAndScan() }) {
                Image(systemName: "plus.circle")
                    .font(.system(size: 48, weight: .thin))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Text("Scan a folder to import music")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func openFolderAndScan() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder containing music to import"
        panel.prompt = "Scan"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        appService.scanFolder(path: url.path)
    }

    // MARK: - Candidate list

    private var candidateList: some View {
        List(
            appService.scanResults,
            id: \.folderPath,
            selection: Binding(
                get: { selectedCandidate?.folderPath },
                set: { path in
                    selectedCandidate = appService.scanResults.first { $0.folderPath == path }
                    if let candidate = selectedCandidate {
                        searchArtist = candidate.artistName
                        searchAlbum = candidate.albumTitle
                        searchResults = []
                        candidateFiles = appService.getCandidateFiles(folderPath: candidate.folderPath)
                    }
                }
            )
        ) { candidate in
            CandidateRow(candidate: candidate, status: appService.importStatuses[candidate.folderPath])
                .disabled(candidate.badAudioCount > 0 || candidate.badImageCount > 0)
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    // MARK: - Metadata search

    private func metadataSearchView(for candidate: BridgeImportCandidate) -> some View {
        VStack(spacing: 0) {
            candidateHeader(candidate)
            Divider()
            if let files = candidateFiles {
                filePane(files)
                Divider()
            }
            searchForm
            Divider()
            if isSearching {
                ProgressView("Searching...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if searchResults.isEmpty {
                ContentUnavailableView(
                    "No results",
                    systemImage: "magnifyingglass",
                    description: Text("Search MusicBrainz or Discogs to find metadata")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                resultsList(for: candidate)
            }
        }
    }

    private func candidateHeader(_ candidate: BridgeImportCandidate) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(candidate.albumTitle)
                    .font(.headline)
                HStack(spacing: 12) {
                    Label("\(candidate.trackCount) tracks", systemImage: "list.bullet")
                    Label(candidate.format, systemImage: "waveform")
                    Label(formatBytes(candidate.totalSizeBytes), systemImage: "internaldrive")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer()
            importStatusBadge(for: candidate.folderPath)
        }
        .padding()
    }

    @ViewBuilder
    private func importStatusBadge(for folderPath: String) -> some View {
        if let status = appService.importStatuses[folderPath] {
            switch status {
            case .importing(let percent):
                ProgressView(value: Double(percent), total: 100)
                    .frame(width: 80)
            case .complete:
                Label("Done", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            case .error(let message):
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .lineLimit(1)
            }
        }
    }

    // MARK: - File pane

    private func filePane(_ files: BridgeCandidateFiles) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                switch files.audio {
                case .cueFlacPairs(let pairs):
                    DisclosureGroup("Audio (\(pairs.count) disc\(pairs.count == 1 ? "" : "s"))") {
                        ForEach(Array(pairs.enumerated()), id: \.offset) { _, pair in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(pair.cueName)
                                    .font(.caption)
                                Text("\(pair.flacName) (\(formatBytes(pair.totalSize)))")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("\(pair.trackCount) tracks")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                            .padding(.leading, 4)
                        }
                    }
                case .trackFiles(let tracks):
                    DisclosureGroup("Audio (\(tracks.count) tracks)") {
                        ForEach(Array(tracks.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Text(file.name)
                                    .font(.caption)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.leading, 4)
                        }
                    }
                }
                if !files.artwork.isEmpty {
                    DisclosureGroup("Images (\(files.artwork.count))") {
                        ForEach(Array(files.artwork.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Text(file.name)
                                    .font(.caption)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.leading, 4)
                        }
                    }
                }
                if !files.documents.isEmpty {
                    DisclosureGroup("Documents (\(files.documents.count))") {
                        ForEach(Array(files.documents.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Text(file.name)
                                    .font(.caption)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.leading, 4)
                        }
                    }
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
        }
        .frame(maxHeight: 200)
    }

    // MARK: - Search form

    private var searchForm: some View {
        VStack(spacing: 8) {
            Picker("", selection: $searchTab) {
                Text("General").tag(SearchTab.general)
                Text("Catalog #").tag(SearchTab.catalogNumber)
                Text("Barcode").tag(SearchTab.barcode)
            }
            .pickerStyle(.segmented)
            .controlSize(.small)
            switch searchTab {
            case .general:
                HStack {
                    TextField("Artist", text: $searchArtist)
                        .textFieldStyle(.roundedBorder)
                    TextField("Album", text: $searchAlbum)
                        .textFieldStyle(.roundedBorder)
                    Button("MusicBrainz") {
                        searchMusicbrainz()
                    }
                    .disabled(searchArtist.isEmpty && searchAlbum.isEmpty)
                    Button("Discogs") {
                        searchDiscogs()
                    }
                    .disabled(searchArtist.isEmpty && searchAlbum.isEmpty)
                }
            case .catalogNumber:
                HStack {
                    TextField("e.g. WPCR-80001", text: $searchCatalog)
                        .textFieldStyle(.roundedBorder)
                    Button("Search") {
                        searchByCatalogNumber()
                    }
                    .disabled(searchCatalog.isEmpty)
                }
            case .barcode:
                HStack {
                    TextField("e.g. 4943674251780", text: $searchBarcode)
                        .textFieldStyle(.roundedBorder)
                    Button("Search") {
                        searchByBarcode()
                    }
                    .disabled(searchBarcode.isEmpty)
                }
            }
        }
        .padding()
    }

    private func resultsList(for candidate: BridgeImportCandidate) -> some View {
        List(searchResults, id: \.releaseId) { result in
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(result.title)
                        .font(.body)
                    HStack(spacing: 8) {
                        if !result.artist.isEmpty {
                            Text(result.artist)
                                .foregroundStyle(.secondary)
                        }
                        if let year = result.year {
                            Text(String(year))
                                .foregroundStyle(.secondary)
                        }
                        if let format = result.format {
                            Text(format)
                                .foregroundStyle(.secondary)
                        }
                        if let label = result.label {
                            Text(label)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .font(.caption)
                    Text(result.source == "musicbrainz" ? "MusicBrainz" : "Discogs")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                Spacer()
                Button("Import") {
                    commitImport(candidate: candidate, result: result)
                }
                .buttonStyle(.borderedProminent)
                .disabled(isImporting(candidate.folderPath))
            }
            .padding(.vertical, 2)
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
    }

    // MARK: - Actions

    private func searchMusicbrainz() {
        isSearching = true
        searchResults = []
        Task {
            let results = await appService.searchMusicbrainz(
                artist: searchArtist,
                album: searchAlbum
            )
            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }

    private func searchDiscogs() {
        isSearching = true
        searchResults = []
        Task {
            let results = await appService.searchDiscogs(
                artist: searchArtist,
                album: searchAlbum
            )
            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }

    private func searchByCatalogNumber() {
        isSearching = true
        searchResults = []
        Task {
            var results = await appService.searchByCatalogNumber(
                catalog: searchCatalog,
                source: "musicbrainz"
            )

            let config = appService.getConfig()
            if config.hasDiscogsToken {
                let discogsResults = await appService.searchByCatalogNumber(
                    catalog: searchCatalog,
                    source: "discogs"
                )
                results.append(contentsOf: discogsResults)
            }

            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }

    private func searchByBarcode() {
        isSearching = true
        searchResults = []
        Task {
            var results = await appService.searchByBarcode(
                barcode: searchBarcode,
                source: "musicbrainz"
            )

            let config = appService.getConfig()
            if config.hasDiscogsToken {
                let discogsResults = await appService.searchByBarcode(
                    barcode: searchBarcode,
                    source: "discogs"
                )
                results.append(contentsOf: discogsResults)
            }

            await MainActor.run {
                searchResults = results
                isSearching = false
            }
        }
    }

    private func commitImport(candidate: BridgeImportCandidate, result: BridgeMetadataResult) {
        appService.commitImport(
            folderPath: candidate.folderPath,
            releaseId: result.releaseId,
            source: result.source
        )
    }

    private func isImporting(_ folderPath: String) -> Bool {
        guard let status = appService.importStatuses[folderPath] else { return false }
        switch status {
        case .importing, .complete:
            return true
        default:
            return false
        }
    }

    // MARK: - Helpers

    private func formatBytes(_ bytes: UInt64) -> String {
        let mb = Double(bytes) / (1024 * 1024)
        if mb >= 1024 {
            return String(format: "%.1f GB", mb / 1024)
        }
        return String(format: "%.0f MB", mb)
    }
}

// MARK: - Candidate Row

private struct CandidateRow: View {
    let candidate: BridgeImportCandidate
    let status: BridgeImportStatus?

    private var isIncomplete: Bool {
        candidate.badAudioCount > 0 || candidate.badImageCount > 0
    }

    private var incompleteMessage: String? {
        let badAudio = candidate.badAudioCount
        let badImages = candidate.badImageCount
        let totalAudio = candidate.trackCount + badAudio
        if badAudio > 0 && badImages > 0 {
            return "\(badAudio) of \(totalAudio) tracks incomplete, \(badImages) corrupt image(s)"
        } else if badAudio > 0 {
            return "\(badAudio) of \(totalAudio) tracks incomplete"
        } else if badImages > 0 {
            return "\(badImages) corrupt image(s)"
        }
        return nil
    }

    var body: some View {
        HStack(spacing: 8) {
            statusIcon
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(candidate.albumTitle)
                    .font(.body)
                    .lineLimit(1)
                if let message = incompleteMessage {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(1)
                }
            }
            .opacity(isIncomplete ? 0.5 : 1.0)
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let status {
            switch status {
            case .importing:
                ProgressView()
                    .controlSize(.small)
            case .complete:
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            case .error:
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
            }
        } else {
            Image(systemName: "folder")
                .foregroundStyle(.secondary)
        }
    }
}
