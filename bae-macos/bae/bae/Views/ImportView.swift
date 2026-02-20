import SwiftUI

// MARK: - State types

enum SearchTab: Hashable {
    case general
    case catalogNumber
    case barcode
}

enum SearchSource: Hashable, CaseIterable {
    case musicbrainz
    case discogs

    var label: String {
        switch self {
        case .musicbrainz: return "MusicBrainz"
        case .discogs: return "Discogs"
        }
    }
}

/// Search state for one (tab, source) slot.
struct TabSearchState {
    var results: [BridgeMetadataResult] = []
    var hasSearched: Bool = false
    var isSearching: Bool = false
}

enum DiscIdLookupState {
    case none
    case loading
    case found
    case notFound
    case error(String)
}

enum CandidateMode: Equatable {
    case identifying
    case loadingDetail
    case confirming
}

/// Per-candidate search and confirmation state.
struct CandidateSearchState {
    // 6 (tab, source) slots
    var generalMb = TabSearchState()
    var generalDiscogs = TabSearchState()
    var catalogMb = TabSearchState()
    var catalogDiscogs = TabSearchState()
    var barcodeMb = TabSearchState()
    var barcodeDiscogs = TabSearchState()

    // Search field values (preserved per candidate)
    var searchArtist: String = ""
    var searchAlbum: String = ""
    var searchYear: String = ""
    var searchLabel: String = ""
    var searchCatalog: String = ""
    var searchBarcode: String = ""

    // Current tab and source
    var activeTab: SearchTab = .general
    var activeSource: SearchSource = .musicbrainz

    // Disc ID
    var discIdLookupState: DiscIdLookupState = .none
    var autoMatches: [BridgeMetadataResult] = []

    // Confirmation
    var mode: CandidateMode = .identifying
    var releaseDetail: BridgeReleaseDetail?
    var selectedCoverUrl: String?
    var prefetchError: String?

    func activeResults() -> TabSearchState {
        switch (activeTab, activeSource) {
        case (.general, .musicbrainz): return generalMb
        case (.general, .discogs): return generalDiscogs
        case (.catalogNumber, .musicbrainz): return catalogMb
        case (.catalogNumber, .discogs): return catalogDiscogs
        case (.barcode, .musicbrainz): return barcodeMb
        case (.barcode, .discogs): return barcodeDiscogs
        }
    }

    mutating func setActiveResults(_ state: TabSearchState) {
        switch (activeTab, activeSource) {
        case (.general, .musicbrainz): generalMb = state
        case (.general, .discogs): generalDiscogs = state
        case (.catalogNumber, .musicbrainz): catalogMb = state
        case (.catalogNumber, .discogs): catalogDiscogs = state
        case (.barcode, .musicbrainz): barcodeMb = state
        case (.barcode, .discogs): barcodeDiscogs = state
        }
    }

    mutating func setResults(_ state: TabSearchState, forTab tab: SearchTab, source: SearchSource) {
        switch (tab, source) {
        case (.general, .musicbrainz): generalMb = state
        case (.general, .discogs): generalDiscogs = state
        case (.catalogNumber, .musicbrainz): catalogMb = state
        case (.catalogNumber, .discogs): catalogDiscogs = state
        case (.barcode, .musicbrainz): barcodeMb = state
        case (.barcode, .discogs): barcodeDiscogs = state
        }
    }
}

// MARK: - ImportView

struct ImportView: View {
    let appService: AppService

    @State private var selectedCandidate: BridgeImportCandidate?
    @State private var candidateFiles: BridgeCandidateFiles?
    @State private var candidateStates: [String: CandidateSearchState] = [:]

    @State private var galleryIndex: Int?
    @State private var audioExpanded = false
    @State private var documentContent: (name: String, text: String)?

    var body: some View {
        ZStack {
            if appService.scanResults.isEmpty {
                emptyState
            } else {
                HSplitView {
                    candidateList
                        .frame(minWidth: 200, idealWidth: 250, maxWidth: 350)
                    if let candidate = selectedCandidate {
                        mainPane(for: candidate)
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

            // Gallery overlay
            if galleryIndex != nil, let files = candidateFiles, !files.artwork.isEmpty {
                Color.black.opacity(0.5)
                    .ignoresSafeArea()
                    .onTapGesture { galleryIndex = nil }
                ImageGalleryView(images: files.artwork, currentIndex: $galleryIndex)
                    .frame(width: 700, height: 550)
                    .background(Theme.surface)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .shadow(radius: 20)
            }

            // Document viewer overlay
            if let doc = documentContent {
                Color.black.opacity(0.5)
                    .ignoresSafeArea()
                    .onTapGesture { documentContent = nil }
                DocumentViewerView(name: doc.name, text: doc.text, onClose: { documentContent = nil })
                    .frame(width: 600, height: 500)
                    .background(Theme.surface)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .shadow(radius: 20)
            }
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

    private var sortedCandidates: [BridgeImportCandidate] {
        let complete = appService.scanResults.filter { $0.badAudioCount == 0 && $0.badImageCount == 0 }
        let incomplete = appService.scanResults.filter { $0.badAudioCount > 0 || $0.badImageCount > 0 }
        return complete + incomplete
    }

    private var candidateSelectionBinding: Binding<String?> {
        Binding(
            get: { selectedCandidate?.folderPath },
            set: { path in
                guard let candidate = appService.scanResults.first(where: { $0.folderPath == path }) else {
                    return
                }
                if candidate.badAudioCount > 0 || candidate.badImageCount > 0 {
                    return
                }
                selectCandidate(candidate)
            }
        )
    }

    private func selectCandidate(_ candidate: BridgeImportCandidate) {
        selectedCandidate = candidate
        candidateFiles = appService.getCandidateFiles(folderPath: candidate.folderPath)

        // Initialize per-candidate state if not present
        if candidateStates[candidate.folderPath] == nil {
            var state = CandidateSearchState()
            state.searchArtist = candidate.artistName
            state.searchAlbum = candidate.albumTitle
            candidateStates[candidate.folderPath] = state

            // Auto-trigger disc ID lookup if available
            if let discId = candidate.mbDiscid {
                candidateStates[candidate.folderPath]?.discIdLookupState = .loading
                Task {
                    let result = await appService.lookupDiscId(discid: discId)
                    await MainActor.run {
                        if let result {
                            handleDiscIdResult(result, for: candidate.folderPath)
                        } else {
                            candidateStates[candidate.folderPath]?.discIdLookupState = .error("Lookup failed")
                        }
                    }
                }
            }
        }
    }

    private func handleDiscIdResult(_ result: BridgeDiscIdResult, for folderPath: String) {
        switch result {
        case .noMatches:
            candidateStates[folderPath]?.discIdLookupState = .notFound
        case .singleMatch(let match):
            candidateStates[folderPath]?.discIdLookupState = .found
            candidateStates[folderPath]?.autoMatches = [match]
            prefetchAndConfirm(folderPath: folderPath, result: match)
        case .multipleMatches(let matches):
            candidateStates[folderPath]?.discIdLookupState = .found
            candidateStates[folderPath]?.autoMatches = matches
        }
    }

    // MARK: - Candidate list UI

    private var candidateList: some View {
        VStack(spacing: 0) {
            candidateListHeader
            Divider()
            List(
                sortedCandidates,
                id: \.folderPath,
                selection: candidateSelectionBinding
            ) { candidate in
                CandidateRow(
                    candidate: candidate,
                    status: appService.importStatuses[candidate.folderPath],
                    onRemove: {
                        if selectedCandidate?.folderPath == candidate.folderPath {
                            selectedCandidate = nil
                        }
                        candidateStates.removeValue(forKey: candidate.folderPath)
                        appService.removeCandidate(folderPath: candidate.folderPath)
                    }
                )
                .padding(.vertical, 4)
            }
            .scrollContentBackground(.hidden)
            .background(Theme.surface)
        }
    }

    private var candidateListHeader: some View {
        HStack {
            Button(action: { openFolderAndAppend() }) {
                Image(systemName: "plus")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Spacer()
            Menu {
                Button("Clear All") {
                    appService.clearAllCandidates()
                    selectedCandidate = nil
                    candidateStates = [:]
                }
                Button("Clear Completed") {
                    let wasSelected = selectedCandidate
                    appService.clearCompletedCandidates()
                    if let wasSelected,
                       !appService.scanResults.contains(where: { $0.folderPath == wasSelected.folderPath }) {
                        selectedCandidate = nil
                    }
                    // Clean up orphaned candidate states
                    let validPaths = Set(appService.scanResults.map(\.folderPath))
                    candidateStates = candidateStates.filter { validPaths.contains($0.key) }
                }
                Button("Clear Incomplete") {
                    let wasSelected = selectedCandidate
                    appService.clearIncompleteCandidates()
                    if let wasSelected,
                       !appService.scanResults.contains(where: { $0.folderPath == wasSelected.folderPath }) {
                        selectedCandidate = nil
                    }
                    let validPaths = Set(appService.scanResults.map(\.folderPath))
                    candidateStates = candidateStates.filter { validPaths.contains($0.key) }
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Theme.surface)
    }

    private func openFolderAndAppend() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a folder containing music to import"
        panel.prompt = "Scan"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        appService.scanAdditionalFolder(path: url.path)
    }

    // MARK: - Main pane (routes to search or confirmation)

    @ViewBuilder
    private func mainPane(for candidate: BridgeImportCandidate) -> some View {
        let folderPath = candidate.folderPath
        let mode = candidateStates[folderPath]?.mode ?? .identifying

        VStack(spacing: 0) {
            candidateHeader(candidate)
            Divider()
            switch mode {
            case .identifying:
                searchPane(for: candidate)
            case .loadingDetail:
                ProgressView("Loading release details...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .confirming:
                if let detail = candidateStates[folderPath]?.releaseDetail {
                    confirmationView(for: candidate, detail: detail)
                } else {
                    // Shouldn't happen, but fall back to search
                    searchPane(for: candidate)
                }
            }
        }
        .animation(nil, value: selectedCandidate?.folderPath)
    }

    private func candidateHeader(_ candidate: BridgeImportCandidate) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(candidate.albumTitle)
                    .font(.headline)
                    .textSelection(.enabled)
                    .help(candidate.folderPath)
                HStack(spacing: 4) {
                    if !candidate.artistName.isEmpty {
                        Text(candidate.artistName)
                            .foregroundStyle(.secondary)
                    }
                    Text("\(candidate.trackCount) tracks")
                    Text(candidate.format)
                    Text(formatBytes(candidate.totalSizeBytes))
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

    // MARK: - Search pane

    private func searchPane(for candidate: BridgeImportCandidate) -> some View {
        let folderPath = candidate.folderPath
        let state = candidateStates[folderPath] ?? CandidateSearchState()

        return Group {
            if let files = candidateFiles {
                HSplitView {
                    filePane(files)
                        .frame(minWidth: 200)
                    searchAndResultsPane(for: candidate, state: state)
                        .frame(minWidth: 300)
                }
            } else {
                searchAndResultsPane(for: candidate, state: state)
            }
        }
    }

    @ViewBuilder
    private func searchAndResultsPane(for candidate: BridgeImportCandidate, state: CandidateSearchState) -> some View {
        let folderPath = candidate.folderPath
        let tabState = state.activeResults()

        VStack(spacing: 0) {
            // Disc ID banner
            discIdBanner(for: candidate, state: state)

            // Search form
            importSearchForm(for: folderPath)
            Divider()

            // Prefetch error
            if let error = state.prefetchError {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(error)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal)
                .padding(.vertical, 6)
            }

            // Auto-match results (if disc ID found multiple)
            if !state.autoMatches.isEmpty && state.autoMatches.count > 1 {
                autoMatchSection(for: candidate, matches: state.autoMatches)
            }

            // Search results
            if tabState.isSearching {
                ProgressView("Searching...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if !tabState.hasSearched && state.autoMatches.count <= 1 {
                ContentUnavailableView(
                    "No results",
                    systemImage: "magnifyingglass",
                    description: Text("Search MusicBrainz or Discogs to find metadata")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if tabState.results.isEmpty && tabState.hasSearched {
                ContentUnavailableView(
                    "No matches found",
                    systemImage: "magnifyingglass",
                    description: Text("Try different search terms or another source")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                resultsList(for: candidate, results: tabState.results)
            }
        }
    }

    @ViewBuilder
    private func discIdBanner(for candidate: BridgeImportCandidate, state: CandidateSearchState) -> some View {
        if candidate.mbDiscid != nil {
            HStack(spacing: 8) {
                switch state.discIdLookupState {
                case .none:
                    EmptyView()
                case .loading:
                    ProgressView()
                        .controlSize(.small)
                    Text("Looking up disc ID...")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                case .found:
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    if state.autoMatches.count == 1 {
                        Text("Disc ID matched one release")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        Text("Disc ID matched \(state.autoMatches.count) releases")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                case .notFound:
                    Image(systemName: "info.circle")
                        .foregroundStyle(.orange)
                    Text("No releases found for disc ID")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                case .error(let msg):
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                    Text(msg)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal)
            .padding(.vertical, 6)
            .background(Theme.surface.opacity(0.5))
        }
    }

    private func autoMatchSection(for candidate: BridgeImportCandidate, matches: [BridgeMetadataResult]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Disc ID matches")
                .font(.caption)
                .fontWeight(.semibold)
                .foregroundStyle(.secondary)
                .padding(.horizontal)
                .padding(.top, 8)
            List(matches, id: \.releaseId) { result in
                MetadataResultRow(
                    result: result,
                    localTrackCount: candidate.trackCount,
                    isImporting: isImporting(candidate.folderPath),
                    onSelect: { prefetchAndConfirm(folderPath: candidate.folderPath, result: result) }
                )
            }
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            .frame(maxHeight: 200)
        }
    }

    // MARK: - Search form

    private func importSearchForm(for folderPath: String) -> some View {
        let state = candidateStates[folderPath] ?? CandidateSearchState()

        return VStack(spacing: 8) {
            // Tab picker
            Picker("", selection: activeTabBinding(for: folderPath)) {
                Text("General").tag(SearchTab.general)
                Text("Catalog #").tag(SearchTab.catalogNumber)
                Text("Barcode").tag(SearchTab.barcode)
            }
            .pickerStyle(.segmented)
            .controlSize(.small)

            switch state.activeTab {
            case .general:
                VStack(spacing: 6) {
                    HStack {
                        TextField("Artist", text: searchFieldBinding(for: folderPath, keyPath: \.searchArtist))
                            .textFieldStyle(.roundedBorder)
                        TextField("Album", text: searchFieldBinding(for: folderPath, keyPath: \.searchAlbum))
                            .textFieldStyle(.roundedBorder)
                    }
                    HStack {
                        TextField("Year", text: searchFieldBinding(for: folderPath, keyPath: \.searchYear))
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 80)
                        TextField("Label", text: searchFieldBinding(for: folderPath, keyPath: \.searchLabel))
                            .textFieldStyle(.roundedBorder)
                        Spacer()

                        // Source picker
                        Picker("", selection: activeSourceBinding(for: folderPath)) {
                            ForEach(SearchSource.allCases, id: \.self) { source in
                                Text(source.label).tag(source)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 200)

                        Button("Search") {
                            searchGeneral(for: folderPath)
                        }
                        .disabled(state.searchArtist.isEmpty && state.searchAlbum.isEmpty)
                    }
                }
            case .catalogNumber:
                HStack {
                    TextField("e.g. WPCR-80001", text: searchFieldBinding(for: folderPath, keyPath: \.searchCatalog))
                        .textFieldStyle(.roundedBorder)
                    Picker("", selection: activeSourceBinding(for: folderPath)) {
                        ForEach(SearchSource.allCases, id: \.self) { source in
                            Text(source.label).tag(source)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 200)
                    Button("Search") {
                        searchByCatalog(for: folderPath)
                    }
                    .disabled(state.searchCatalog.isEmpty)
                }
            case .barcode:
                HStack {
                    TextField("e.g. 4943674251780", text: searchFieldBinding(for: folderPath, keyPath: \.searchBarcode))
                        .textFieldStyle(.roundedBorder)
                    Picker("", selection: activeSourceBinding(for: folderPath)) {
                        ForEach(SearchSource.allCases, id: \.self) { source in
                            Text(source.label).tag(source)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 200)
                    Button("Search") {
                        searchByBarcode(for: folderPath)
                    }
                    .disabled(state.searchBarcode.isEmpty)
                }
            }
        }
        .padding()
        .animation(nil, value: state.activeTab)
    }

    // MARK: - Bindings into per-candidate state

    private func activeTabBinding(for folderPath: String) -> Binding<SearchTab> {
        Binding(
            get: { candidateStates[folderPath]?.activeTab ?? .general },
            set: { candidateStates[folderPath]?.activeTab = $0 }
        )
    }

    private func activeSourceBinding(for folderPath: String) -> Binding<SearchSource> {
        Binding(
            get: { candidateStates[folderPath]?.activeSource ?? .musicbrainz },
            set: { candidateStates[folderPath]?.activeSource = $0 }
        )
    }

    private func searchFieldBinding(for folderPath: String, keyPath: WritableKeyPath<CandidateSearchState, String>) -> Binding<String> {
        Binding(
            get: { candidateStates[folderPath]?[keyPath: keyPath] ?? "" },
            set: { candidateStates[folderPath]?[keyPath: keyPath] = $0 }
        )
    }

    // MARK: - Results list

    private func resultsList(for candidate: BridgeImportCandidate, results: [BridgeMetadataResult]) -> some View {
        List(results, id: \.releaseId) { result in
            MetadataResultRow(
                result: result,
                localTrackCount: candidate.trackCount,
                isImporting: isImporting(candidate.folderPath),
                onSelect: { prefetchAndConfirm(folderPath: candidate.folderPath, result: result) }
            )
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
    }

    // MARK: - Search actions

    private func searchGeneral(for folderPath: String) {
        guard var state = candidateStates[folderPath] else { return }
        let capturedTab = state.activeTab
        let capturedSource = state.activeSource
        var tabState = state.activeResults()
        tabState.isSearching = true
        state.setActiveResults(tabState)
        state.prefetchError = nil
        candidateStates[folderPath] = state

        let artist = state.searchArtist
        let album = state.searchAlbum
        let year = state.searchYear.isEmpty ? nil : state.searchYear
        let label = state.searchLabel.isEmpty ? nil : state.searchLabel

        Task {
            let results: [BridgeMetadataResult]
            switch capturedSource {
            case .musicbrainz:
                results = await appService.searchMusicbrainz(artist: artist, album: album, year: year, label: label)
            case .discogs:
                results = await appService.searchDiscogs(artist: artist, album: album, year: year, label: label)
            }

            await MainActor.run {
                guard var state = candidateStates[folderPath] else { return }
                var tabState = TabSearchState()
                tabState.results = results
                tabState.hasSearched = true
                tabState.isSearching = false
                state.setResults(tabState, forTab: capturedTab, source: capturedSource)
                candidateStates[folderPath] = state
            }
        }
    }

    private func searchByCatalog(for folderPath: String) {
        guard var state = candidateStates[folderPath] else { return }
        let capturedTab = state.activeTab
        let capturedSource = state.activeSource
        var tabState = state.activeResults()
        tabState.isSearching = true
        state.setActiveResults(tabState)
        state.prefetchError = nil
        candidateStates[folderPath] = state

        let catalog = state.searchCatalog
        let sourceString = capturedSource == .musicbrainz ? "musicbrainz" : "discogs"

        Task {
            let results = await appService.searchByCatalogNumber(catalog: catalog, source: sourceString)

            await MainActor.run {
                guard var state = candidateStates[folderPath] else { return }
                var tabState = TabSearchState()
                tabState.results = results
                tabState.hasSearched = true
                tabState.isSearching = false
                state.setResults(tabState, forTab: capturedTab, source: capturedSource)
                candidateStates[folderPath] = state
            }
        }
    }

    private func searchByBarcode(for folderPath: String) {
        guard var state = candidateStates[folderPath] else { return }
        let capturedTab = state.activeTab
        let capturedSource = state.activeSource
        var tabState = state.activeResults()
        tabState.isSearching = true
        state.setActiveResults(tabState)
        state.prefetchError = nil
        candidateStates[folderPath] = state

        let barcode = state.searchBarcode
        let sourceString = capturedSource == .musicbrainz ? "musicbrainz" : "discogs"

        Task {
            let results = await appService.searchByBarcode(barcode: barcode, source: sourceString)

            await MainActor.run {
                guard var state = candidateStates[folderPath] else { return }
                var tabState = TabSearchState()
                tabState.results = results
                tabState.hasSearched = true
                tabState.isSearching = false
                state.setResults(tabState, forTab: capturedTab, source: capturedSource)
                candidateStates[folderPath] = state
            }
        }
    }

    // MARK: - Confirmation flow

    private func prefetchAndConfirm(folderPath: String, result: BridgeMetadataResult) {
        candidateStates[folderPath]?.mode = .loadingDetail
        candidateStates[folderPath]?.prefetchError = nil
        Task {
            if let detail = await appService.prefetchRelease(releaseId: result.releaseId, source: result.source) {
                await MainActor.run {
                    candidateStates[folderPath]?.releaseDetail = detail
                    candidateStates[folderPath]?.selectedCoverUrl = detail.coverArt.first?.url
                    candidateStates[folderPath]?.mode = .confirming
                }
            } else {
                await MainActor.run {
                    candidateStates[folderPath]?.mode = .identifying
                    candidateStates[folderPath]?.prefetchError = "Failed to load release details. Try again or choose a different result."
                }
            }
        }
    }

    private func confirmationView(for candidate: BridgeImportCandidate, detail: BridgeReleaseDetail) -> some View {
        let folderPath = candidate.folderPath
        let trackCountMismatch = detail.trackCount != candidate.trackCount

        return ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // Release metadata
                VStack(alignment: .leading, spacing: 8) {
                    Text(detail.title)
                        .font(.title2)
                        .fontWeight(.semibold)
                    Text(detail.artist)
                        .font(.title3)
                        .foregroundStyle(.secondary)
                    HStack(spacing: 16) {
                        if let year = detail.year {
                            metadataTag(String(year))
                        }
                        if let format = detail.format {
                            metadataTag(format)
                        }
                        if let label = detail.label {
                            metadataTag(label)
                        }
                        if let catno = detail.catalogNumber {
                            metadataTag(catno)
                        }
                    }
                    Text(detail.source == "musicbrainz" ? "MusicBrainz" : "Discogs")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }

                Divider()

                // Track count comparison
                if trackCountMismatch {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text("Track count mismatch: local files have \(candidate.trackCount) tracks, release has \(detail.trackCount)")
                            .font(.callout)
                            .foregroundStyle(.orange)
                    }
                    .padding(10)
                    .background(Color.orange.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }

                // Track listing
                VStack(alignment: .leading, spacing: 4) {
                    Text("Tracks (\(detail.trackCount))")
                        .font(.caption)
                        .fontWeight(.semibold)
                        .foregroundStyle(.secondary)
                    ForEach(Array(detail.tracks.enumerated()), id: \.offset) { _, track in
                        HStack {
                            Text(track.position)
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                                .frame(width: 30, alignment: .trailing)
                            Text(track.title)
                                .font(.caption)
                                .lineLimit(1)
                            Spacer()
                            if let ms = track.durationMs {
                                Text(formatDuration(ms))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                // Cover art selection
                if !detail.coverArt.isEmpty || (candidateFiles?.artwork.isEmpty == false) {
                    Divider()
                    coverArtSection(for: folderPath, detail: detail)
                }

                Divider()

                // Action buttons
                HStack {
                    Button("Back to Search") {
                        candidateStates[folderPath]?.mode = .identifying
                        candidateStates[folderPath]?.releaseDetail = nil
                    }
                    Spacer()
                    Button("Confirm Import") {
                        commitConfirmedImport(candidate: candidate, detail: detail)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isImporting(folderPath))
                }
            }
            .padding()
        }
    }

    @ViewBuilder
    private func coverArtSection(for folderPath: String, detail: BridgeReleaseDetail) -> some View {
        let selectedUrl = candidateStates[folderPath]?.selectedCoverUrl
        let localArtwork = candidateFiles?.artwork ?? []

        VStack(alignment: .leading, spacing: 8) {
            Text("Cover Art")
                .font(.caption)
                .fontWeight(.semibold)
                .foregroundStyle(.secondary)

            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], spacing: 8) {
                // Remote cover options
                ForEach(Array(detail.coverArt.enumerated()), id: \.offset) { _, cover in
                    coverOption(
                        isSelected: selectedUrl == cover.url,
                        label: cover.source == "musicbrainz" ? "Cover Art Archive" : "Discogs"
                    ) {
                        AsyncImage(url: URL(string: cover.url)) { image in
                            image
                                .resizable()
                                .aspectRatio(contentMode: .fill)
                        } placeholder: {
                            ZStack {
                                Theme.placeholder
                                ProgressView()
                                    .controlSize(.small)
                            }
                        }
                    }
                    .onTapGesture {
                        candidateStates[folderPath]?.selectedCoverUrl = cover.url
                    }
                }

                // Local artwork options
                ForEach(Array(localArtwork.enumerated()), id: \.offset) { _, file in
                    let localUrl = "local:\(file.name)"
                    coverOption(
                        isSelected: selectedUrl == localUrl,
                        label: file.name
                    ) {
                        if let nsImage = NSImage(contentsOf: URL(fileURLWithPath: file.path)) {
                            Image(nsImage: nsImage)
                                .resizable()
                                .aspectRatio(contentMode: .fill)
                        } else {
                            ZStack {
                                Theme.placeholder
                                Image(systemName: "photo")
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                    .onTapGesture {
                        candidateStates[folderPath]?.selectedCoverUrl = localUrl
                    }
                }
            }
        }
    }

    private func coverOption<Content: View>(isSelected: Bool, label: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(spacing: 2) {
            content()
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                .overlay(
                    RoundedRectangle(cornerRadius: 4)
                        .stroke(isSelected ? Color.accentColor : Color.clear, lineWidth: 3)
                )
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 120)
        }
    }

    private func metadataTag(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 4))
    }

    private func commitConfirmedImport(candidate: BridgeImportCandidate, detail: BridgeReleaseDetail) {
        let folderPath = candidate.folderPath
        let selectedUrl = candidateStates[folderPath]?.selectedCoverUrl

        let coverSelection: BridgeCoverSelection?
        if let url = selectedUrl {
            if url.hasPrefix("local:") {
                let filename = String(url.dropFirst("local:".count))
                coverSelection = .releaseImage(fileId: filename)
            } else {
                coverSelection = .remoteCover(url: url, source: detail.source)
            }
        } else {
            coverSelection = nil
        }

        appService.commitImport(
            folderPath: folderPath,
            releaseId: detail.releaseId,
            source: detail.source,
            selectedCover: coverSelection
        )
    }

    // MARK: - File pane

    private func filePane(_ files: BridgeCandidateFiles) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                switch files.audio {
                case .cueFlacPairs(let pairs):
                    DisclosureGroup(isExpanded: $audioExpanded) {
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
                        }
                    } label: {
                        Text("Audio (\(pairs.count) disc\(pairs.count == 1 ? "" : "s"))")
                            .font(.caption)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                case .trackFiles(let tracks):
                    DisclosureGroup(isExpanded: $audioExpanded) {
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
                        }
                    } label: {
                        Text("Audio (\(tracks.count) tracks)")
                            .font(.caption)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                }
                if !files.artwork.isEmpty {
                    fileSection("Images (\(files.artwork.count))") {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], spacing: 8) {
                            ForEach(Array(files.artwork.enumerated()), id: \.offset) { index, file in
                                imageThumb(file)
                                    .onTapGesture { galleryIndex = index }
                            }
                        }
                    }
                }
                if !files.documents.isEmpty {
                    fileSection("Documents (\(files.documents.count))") {
                        ForEach(Array(files.documents.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Image(systemName: "doc.text")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text(file.name)
                                    .font(.caption)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .contentShape(Rectangle())
                            .onTapGesture {
                                if let text = try? String(contentsOfFile: file.path, encoding: .utf8) {
                                    documentContent = (name: file.name, text: text)
                                } else if let text = try? String(contentsOfFile: file.path, encoding: .shiftJIS) {
                                    documentContent = (name: file.name, text: text)
                                }
                            }
                            .onHover { hovering in
                                if hovering { NSCursor.pointingHand.push() } else { NSCursor.pop() }
                            }
                        }
                    }
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
        }
    }

    private func fileSection<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption)
                .fontWeight(.semibold)
                .foregroundStyle(.secondary)
            content()
        }
    }

    private func imageThumb(_ file: BridgeFileInfo) -> some View {
        VStack(spacing: 2) {
            if let url = URL(string: "file://\(file.path)"),
               let nsImage = NSImage(contentsOf: url) {
                Image(nsImage: nsImage)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 120, height: 120)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            } else {
                ZStack {
                    Theme.placeholder
                    Image(systemName: "photo")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            Text(file.name)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 120)
        }
    }

    // MARK: - Helpers

    private func isImporting(_ folderPath: String) -> Bool {
        guard let status = appService.importStatuses[folderPath] else { return false }
        switch status {
        case .importing, .complete:
            return true
        default:
            return false
        }
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        let mb = Double(bytes) / (1024 * 1024)
        if mb >= 1024 {
            return String(format: "%.1f GB", mb / 1024)
        }
        return String(format: "%.0f MB", mb)
    }

    private func formatDuration(_ ms: UInt64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return String(format: "%d:%02d", minutes, seconds)
    }
}

// MARK: - MetadataResultRow

struct MetadataResultRow: View {
    let result: BridgeMetadataResult
    let localTrackCount: UInt32
    let isImporting: Bool
    let onSelect: () -> Void

    private var trackCountMismatch: Bool {
        result.trackCount > 0 && result.trackCount != localTrackCount
    }

    var body: some View {
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
                    if result.trackCount > 0 {
                        Text("\(result.trackCount) tracks")
                            .foregroundStyle(trackCountMismatch ? .orange : .secondary)
                    }
                }
                .font(.caption)
                Text(result.source == "musicbrainz" ? "MusicBrainz" : "Discogs")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Button("Select") {
                onSelect()
            }
            .disabled(isImporting)
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Candidate Row

struct CandidateRow: View {
    let candidate: BridgeImportCandidate
    let status: BridgeImportStatus?
    let onRemove: () -> Void

    @State private var isHovered = false

    private var isIncomplete: Bool {
        candidate.badAudioCount > 0 || candidate.badImageCount > 0
    }

    private var folderName: String {
        URL(fileURLWithPath: candidate.folderPath).lastPathComponent
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
        HStack(spacing: 10) {
            statusIcon
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(folderName)
                    .font(.body)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if candidate.albumTitle != folderName {
                    Text(candidate.albumTitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                if let message = incompleteMessage {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(1)
                }
            }
            .opacity(isIncomplete ? 0.5 : 1.0)
            Spacer()
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Remove from list")
            .opacity(isHovered ? 1 : 0)
        }
        .onHover { hovering in
            isHovered = hovering
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
            Button(action: {
                NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: candidate.folderPath)
            }) {
                Image(systemName: "folder")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Reveal in Finder")
        }
    }
}

// MARK: - Image Gallery

struct ImageGalleryView: View {
    let images: [BridgeFileInfo]
    @Binding var currentIndex: Int?

    private var safeIndex: Int {
        guard let idx = currentIndex, idx >= 0, idx < images.count else { return 0 }
        return idx
    }

    private var currentFile: BridgeFileInfo {
        images[safeIndex]
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
            ZStack {
                Color(nsColor: .controlBackgroundColor)
                imageContent
            }
            Divider()
            caption
        }
        .frame(minWidth: 600, minHeight: 500)
        .onKeyPress(.leftArrow) {
            navigatePrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            navigateNext()
            return .handled
        }
    }

    private var toolbar: some View {
        HStack {
            Text("\(safeIndex + 1) of \(images.count)")
                .font(.callout)
                .foregroundStyle(.secondary)
            Spacer()
            Button("Done") {
                currentIndex = nil
            }
            .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private var imageContent: some View {
        HStack(spacing: 0) {
            Button(action: navigatePrevious) {
                Image(systemName: "chevron.left")
                    .font(.title2)
                    .frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .disabled(safeIndex == 0)
            .opacity(safeIndex == 0 ? 0.3 : 1.0)
            Spacer()
            if let nsImage = NSImage(contentsOf: URL(fileURLWithPath: currentFile.path)) {
                Image(nsImage: nsImage)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .padding(16)
            } else {
                ContentUnavailableView(
                    "Cannot load image",
                    systemImage: "photo",
                    description: Text(currentFile.name)
                )
            }
            Spacer()
            Button(action: navigateNext) {
                Image(systemName: "chevron.right")
                    .font(.title2)
                    .frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .disabled(safeIndex >= images.count - 1)
            .opacity(safeIndex >= images.count - 1 ? 0.3 : 1.0)
        }
        .padding(.horizontal, 8)
    }

    private var caption: some View {
        Text(currentFile.name)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .padding(.vertical, 8)
    }

    private func navigatePrevious() {
        if safeIndex > 0 {
            currentIndex = safeIndex - 1
        }
    }

    private func navigateNext() {
        if safeIndex < images.count - 1 {
            currentIndex = safeIndex + 1
        }
    }
}

// MARK: - Document Viewer

struct DocumentViewerView: View {
    let name: String
    let text: String
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(name)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Done") { onClose() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            Divider()
            ScrollView {
                Text(text)
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding()
            }
        }
    }
}

// MARK: - Previews

#Preview("Candidate Row - Pending") {
    CandidateRow(
        candidate: BridgeImportCandidate(
            folderPath: "/path/to/Album Title",
            artistName: "Artist Name",
            albumTitle: "Album Title",
            trackCount: 12,
            format: "FLAC",
            totalSizeBytes: 524_288_000,
            badAudioCount: 0,
            badImageCount: 0,
            mbDiscid: nil
        ),
        status: nil,
        onRemove: {}
    )
    .padding()
}

#Preview("Candidate Row - Complete") {
    CandidateRow(
        candidate: BridgeImportCandidate(
            folderPath: "/path/to/Album Title",
            artistName: "Artist Name",
            albumTitle: "Album Title",
            trackCount: 12,
            format: "FLAC",
            totalSizeBytes: 524_288_000,
            badAudioCount: 0,
            badImageCount: 0,
            mbDiscid: nil
        ),
        status: .complete,
        onRemove: {}
    )
    .padding()
}

#Preview("Candidate Row - Incomplete") {
    CandidateRow(
        candidate: BridgeImportCandidate(
            folderPath: "/path/to/Album Title",
            artistName: "Artist Name",
            albumTitle: "Album Title",
            trackCount: 10,
            format: "FLAC",
            totalSizeBytes: 524_288_000,
            badAudioCount: 2,
            badImageCount: 1,
            mbDiscid: nil
        ),
        status: nil,
        onRemove: {}
    )
    .padding()
}

#Preview("Candidate Row - Folder differs from title") {
    CandidateRow(
        candidate: BridgeImportCandidate(
            folderPath: "/path/to/CD1",
            artistName: "Artist Name",
            albumTitle: "Album Title",
            trackCount: 8,
            format: "FLAC",
            totalSizeBytes: 380_000_000,
            badAudioCount: 0,
            badImageCount: 0,
            mbDiscid: nil
        ),
        status: nil,
        onRemove: {}
    )
    .padding()
}

#Preview("Metadata Result Row") {
    MetadataResultRow(
        result: BridgeMetadataResult(
            source: "musicbrainz",
            releaseId: "rel-123",
            title: "Album Title",
            artist: "Artist Name",
            year: 2024,
            format: "CD",
            label: "Label Name",
            trackCount: 12
        ),
        localTrackCount: 12,
        isImporting: false,
        onSelect: {}
    )
    .padding()
}

#Preview("Document Viewer") {
    DocumentViewerView(
        name: "info.txt",
        text: "This is sample document content.\nLine 2 of the document.\nLine 3 with more text.",
        onClose: {}
    )
    .frame(width: 600, height: 500)
    .background(Theme.surface)
}
