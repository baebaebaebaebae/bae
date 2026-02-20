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
    var searchCatalog: String = ""
    var searchBarcode: String = ""

    // Current tab and source
    var activeTab: SearchTab = .general
    var activeSource: SearchSource = .musicbrainz

    // Disc ID
    var discIdLookupState: DiscIdLookupState = .none
    var autoMatches: [BridgeMetadataResult] = []
    var showManualSearch: Bool = false

    // Confirmation
    var mode: CandidateMode = .identifying
    var releaseDetail: BridgeReleaseDetail?
    var selectedCoverUrl: String?
    var prefetchError: String?
    var managed: Bool = true

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
    @State private var imagesExpanded = true
    @State private var documentsExpanded = true
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
                Color.black.opacity(0.6)
                    .ignoresSafeArea()
                    .onTapGesture { galleryIndex = nil }
                ImageGalleryView(images: files.artwork, currentIndex: $galleryIndex)
                    .padding(40)
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
        .onChange(of: appService.scanResults.count) {
            if selectedCandidate == nil,
               let first = appService.scanResults.first(where: { $0.badAudioCount == 0 && $0.badImageCount == 0 }) {
                selectCandidate(first)
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

    // MARK: - Main pane (file pane + search + optional confirmation)

    @ViewBuilder
    private func mainPane(for candidate: BridgeImportCandidate) -> some View {
        let folderPath = candidate.folderPath
        let mode = candidateStates[folderPath]?.mode ?? .identifying
        let state = candidateStates[folderPath] ?? CandidateSearchState()

        VStack(spacing: 0) {
            candidateHeader(candidate)
            Divider()
            HSplitView {
                // File pane (left)
                if let files = candidateFiles {
                    filePane(files)
                        .frame(minWidth: 200)
                }
                // Right pane: confirmation replaces search when confirming
                if mode == .loadingDetail {
                    ProgressView("Loading release details...")
                        .frame(minWidth: 300, maxWidth: .infinity, maxHeight: .infinity)
                } else if mode == .confirming, let detail = candidateStates[folderPath]?.releaseDetail {
                    confirmationView(for: candidate, detail: detail)
                        .frame(minWidth: 300, maxWidth: .infinity)
                } else {
                    searchAndResultsPane(for: candidate, state: state)
                        .frame(minWidth: 300)
                }
            }
        }
        .animation(nil, value: selectedCandidate?.folderPath)
    }

    private func candidateHeader(_ candidate: BridgeImportCandidate) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(candidate.albumTitle)
                    .font(.title3)
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
                .font(.callout)
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

    @ViewBuilder
    private func searchAndResultsPane(for candidate: BridgeImportCandidate, state: CandidateSearchState) -> some View {
        let folderPath = candidate.folderPath
        let tabState = state.activeResults()
        let hasAutoMatches = !state.autoMatches.isEmpty
        let showingAutoMatches = hasAutoMatches && !state.showManualSearch

        VStack(spacing: 0) {
            // Disc ID banner
            discIdBanner(for: candidate, state: state)

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

            if showingAutoMatches {
                // Show auto matches directly (no search form)
                HStack {
                    Button("Search manually") {
                        candidateStates[folderPath]?.showManualSearch = true
                    }
                    .buttonStyle(.link)
                    .font(.caption)
                    Spacer()
                }
                .padding(.horizontal)
                .padding(.vertical, 6)
                Divider()
                autoMatchSection(for: candidate, matches: state.autoMatches)
            } else {
                // Show search form
                if hasAutoMatches {
                    HStack {
                        Button("View disc ID matches (\(state.autoMatches.count))") {
                            candidateStates[folderPath]?.showManualSearch = false
                        }
                        .buttonStyle(.link)
                        .font(.caption)
                        Spacer()
                    }
                    .padding(.horizontal)
                    .padding(.vertical, 6)
                }

                importSearchForm(for: folderPath)
                Divider()

                // Search results
                if tabState.isSearching {
                    ProgressView("Searching...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if !tabState.hasSearched {
                    ContentUnavailableView(
                        "No results",
                        systemImage: "magnifyingglass",
                        description: Text("Search MusicBrainz or Discogs to find metadata")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if tabState.results.isEmpty {
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
                HStack {
                    TextField("Artist", text: searchFieldBinding(for: folderPath, keyPath: \.searchArtist))
                        .textFieldStyle(.roundedBorder)
                    TextField("Album", text: searchFieldBinding(for: folderPath, keyPath: \.searchAlbum))
                        .textFieldStyle(.roundedBorder)
                    sourcePicker(for: folderPath)
                    Button("Search") {
                        searchGeneral(for: folderPath)
                    }
                    .disabled(state.searchArtist.isEmpty && state.searchAlbum.isEmpty)
                }
            case .catalogNumber:
                HStack {
                    TextField("e.g. WPCR-80001", text: searchFieldBinding(for: folderPath, keyPath: \.searchCatalog))
                        .textFieldStyle(.roundedBorder)
                    sourcePicker(for: folderPath)
                    Button("Search") {
                        searchByCatalog(for: folderPath)
                    }
                    .disabled(state.searchCatalog.isEmpty)
                }
            case .barcode:
                HStack {
                    TextField("e.g. 4943674251780", text: searchFieldBinding(for: folderPath, keyPath: \.searchBarcode))
                        .textFieldStyle(.roundedBorder)
                    sourcePicker(for: folderPath)
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

    private func sourcePicker(for folderPath: String) -> some View {
        Picker("", selection: activeSourceBinding(for: folderPath)) {
            ForEach(SearchSource.allCases, id: \.self) { source in
                Text(source.label).tag(source)
            }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
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

        Task {
            let results: [BridgeMetadataResult]
            switch capturedSource {
            case .musicbrainz:
                results = await appService.searchMusicbrainz(artist: artist, album: album, year: nil, label: nil)
            case .discogs:
                results = await appService.searchDiscogs(artist: artist, album: album, year: nil, label: nil)
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

                // Storage mode
                VStack(alignment: .leading, spacing: 4) {
                    Picker("Storage", selection: Binding(
                        get: { candidateStates[folderPath]?.managed ?? true },
                        set: { candidateStates[folderPath]?.managed = $0 }
                    )) {
                        Text("Copy to library").tag(true)
                        Text("Leave in place").tag(false)
                    }
                    .pickerStyle(.segmented)
                    if candidateStates[folderPath]?.managed == true && appService.appHandle.isSyncReady() {
                        Text("Files will sync to cloud")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

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
        let managed = candidateStates[folderPath]?.managed ?? true

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
            selectedCover: coverSelection,
            managed: managed
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
                                    .font(.callout)
                                Text("\(pair.flacName) (\(formatBytes(pair.totalSize)))")
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                                Text("\(pair.trackCount) tracks")
                                    .font(.caption)
                                    .foregroundStyle(.tertiary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    } label: {
                        Text("Audio (\(pairs.count) disc\(pairs.count == 1 ? "" : "s"))")
                            .font(.subheadline)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                case .trackFiles(let tracks):
                    DisclosureGroup(isExpanded: $audioExpanded) {
                        ForEach(Array(tracks.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Text(file.name)
                                    .font(.callout)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    } label: {
                        Text("Audio (\(tracks.count) tracks)")
                            .font(.subheadline)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                }
                if !files.artwork.isEmpty {
                    DisclosureGroup(isExpanded: $imagesExpanded) {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 100), spacing: 4)], spacing: 4) {
                            ForEach(Array(files.artwork.enumerated()), id: \.offset) { index, file in
                                imageThumb(file)
                                    .onTapGesture { galleryIndex = index }
                            }
                        }
                    } label: {
                        Text("Images (\(files.artwork.count))")
                            .font(.subheadline)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                }
                if !files.documents.isEmpty {
                    DisclosureGroup(isExpanded: $documentsExpanded) {
                        ForEach(Array(files.documents.enumerated()), id: \.offset) { _, file in
                            HStack {
                                Image(systemName: "doc.text")
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                                Text(file.name)
                                    .font(.callout)
                                    .lineLimit(1)
                                Spacer()
                                Text(formatBytes(file.size))
                                    .font(.callout)
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
                    } label: {
                        Text("Documents (\(files.documents.count))")
                            .font(.subheadline)
                            .fontWeight(.semibold)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
        }
    }

    private func imageThumb(_ file: BridgeFileInfo) -> some View {
        VStack(spacing: 2) {
            if let url = URL(string: "file://\(file.path)"),
               let nsImage = NSImage(contentsOf: url) {
                Image(nsImage: nsImage)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            } else {
                ZStack {
                    Theme.placeholder
                    Image(systemName: "photo")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .aspectRatio(1, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            Text(file.name)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
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
        let kb = Double(bytes) / 1024
        if kb < 1 {
            return "\(bytes) B"
        }
        let mb = kb / 1024
        if mb < 1 {
            return String(format: "%.0f KB", kb)
        }
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
        HStack(spacing: 10) {
            AsyncImage(url: result.coverUrl.flatMap { URL(string: $0) }) { image in
                image.resizable().aspectRatio(contentMode: .fill)
            } placeholder: {
                ZStack {
                    Color(nsColor: .controlBackgroundColor)
                    Image(systemName: "photo")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .frame(width: 44, height: 44)
            .clipShape(RoundedRectangle(cornerRadius: 4))
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
                    .font(.callout)
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

    private var canCycle: Bool {
        images.count > 1
    }

    var body: some View {
        ZStack {
            // Main image area
            VStack(spacing: 0) {
                // Image viewer
                GeometryReader { geometry in
                    let availableHeight = geometry.size.height - 120
                    ZStack {
                        if let nsImage = NSImage(contentsOf: URL(fileURLWithPath: currentFile.path)) {
                            Image(nsImage: nsImage)
                                .resizable()
                                .aspectRatio(contentMode: .fit)
                                .frame(maxWidth: geometry.size.width - 120, maxHeight: availableHeight)
                                .shadow(color: .black.opacity(0.5), radius: 20)
                        } else {
                            VStack(spacing: 8) {
                                Image(systemName: "photo")
                                    .font(.largeTitle)
                                    .foregroundStyle(.gray)
                                Text("Cannot load image")
                                    .font(.callout)
                                    .foregroundStyle(.gray)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                // Caption + thumbnail strip
                VStack(spacing: 10) {
                    // Filename and count
                    Text("\(currentFile.name) \u{2014} \(safeIndex + 1) of \(images.count)")
                        .font(.callout)
                        .foregroundStyle(.white.opacity(0.7))
                        .lineLimit(1)

                    // Thumbnail strip
                    if canCycle {
                        ScrollViewReader { scrollProxy in
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 6) {
                                    ForEach(Array(images.enumerated()), id: \.offset) { index, file in
                                        thumbnailView(for: file, at: index)
                                            .id(index)
                                    }
                                }
                                .padding(.horizontal, 8)
                            }
                            .frame(height: 64)
                            .onChange(of: safeIndex) { _, newIndex in
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    scrollProxy.scrollTo(newIndex, anchor: .center)
                                }
                            }
                        }
                    }
                }
                .padding(.bottom, 16)
            }

            // Previous button
            if canCycle {
                HStack {
                    Button(action: navigatePrevious) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 48, height: 48)
                            Image(systemName: "chevron.left")
                                .font(.title2.weight(.medium))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(.leading, 16)
                    Spacer()
                }
            }

            // Next button
            if canCycle {
                HStack {
                    Spacer()
                    Button(action: navigateNext) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 48, height: 48)
                            Image(systemName: "chevron.right")
                                .font(.title2.weight(.medium))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(.trailing, 16)
                }
            }

            // Close button (top-right)
            VStack {
                HStack {
                    Spacer()
                    Button(action: { currentIndex = nil }) {
                        ZStack {
                            Circle()
                                .fill(.black.opacity(0.4))
                                .frame(width: 36, height: 36)
                            Image(systemName: "xmark")
                                .font(.body.weight(.semibold))
                                .foregroundStyle(.white.opacity(0.8))
                        }
                    }
                    .buttonStyle(.plain)
                    .padding(12)
                }
                Spacer()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onKeyPress(.leftArrow) {
            navigatePrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            navigateNext()
            return .handled
        }
        .onKeyPress(.escape) {
            currentIndex = nil
            return .handled
        }
    }

    @ViewBuilder
    private func thumbnailView(for file: BridgeFileInfo, at index: Int) -> some View {
        let isActive = index == safeIndex
        Button(action: { currentIndex = index }) {
            Group {
                if let nsImage = NSImage(contentsOf: URL(fileURLWithPath: file.path)) {
                    Image(nsImage: nsImage)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: 56, height: 56)
                        .clipped()
                } else {
                    Color.gray.opacity(0.3)
                        .frame(width: 56, height: 56)
                        .overlay {
                            Image(systemName: "photo")
                                .foregroundStyle(.gray)
                        }
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(
                        isActive ? .white : .gray.opacity(0.4),
                        lineWidth: isActive ? 2 : 1
                    )
            )
        }
        .buttonStyle(.plain)
    }

    private func navigatePrevious() {
        if canCycle {
            currentIndex = safeIndex == 0 ? images.count - 1 : safeIndex - 1
        }
    }

    private func navigateNext() {
        if canCycle {
            currentIndex = safeIndex == images.count - 1 ? 0 : safeIndex + 1
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
            trackCount: 12,
            coverUrl: nil
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
