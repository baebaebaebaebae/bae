import SwiftUI

struct ImportView: View {
    let appService: AppService
    @Binding var isPresented: Bool

    @State private var selectedCandidate: BridgeImportCandidate?
    @State private var searchResults: [BridgeMetadataResult] = []
    @State private var isSearching = false
    @State private var searchArtist = ""
    @State private var searchAlbum = ""

    var body: some View {
        NavigationSplitView {
            candidateList
                .navigationTitle("Scanned Folders")
        } detail: {
            if let candidate = selectedCandidate {
                metadataSearchView(for: candidate)
            } else {
                ContentUnavailableView(
                    "Select a folder",
                    systemImage: "folder",
                    description: Text("Choose a scanned folder to search for metadata")
                )
            }
        }
        .frame(minWidth: 700, minHeight: 450)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close") {
                    isPresented = false
                }
            }
        }
    }

    // MARK: - Candidate list

    private var candidateList: some View {
        Group {
            if appService.scanResults.isEmpty {
                ContentUnavailableView(
                    "No folders found",
                    systemImage: "folder.badge.questionmark",
                    description: Text("No importable music was found in the selected folder")
                )
            } else {
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
                            }
                        }
                    )
                ) { candidate in
                    CandidateRow(candidate: candidate, status: appService.importStatuses[candidate.folderPath])
                }
            }
        }
    }

    // MARK: - Metadata search

    private func metadataSearchView(for candidate: BridgeImportCandidate) -> some View {
        VStack(spacing: 0) {
            candidateHeader(candidate)

            Divider()

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

    private var searchForm: some View {
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

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(candidate.albumTitle)
                    .font(.body)
                    .lineLimit(1)
                Text("\(candidate.trackCount) tracks - \(candidate.format)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if let status {
                statusIndicator(status)
            }
        }
    }

    @ViewBuilder
    private func statusIndicator(_ status: BridgeImportStatus) -> some View {
        switch status {
        case .importing(let percent):
            ProgressView(value: Double(percent), total: 100)
                .frame(width: 40)
        case .complete:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
        case .error:
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
        default:
            EmptyView()
        }
    }
}
