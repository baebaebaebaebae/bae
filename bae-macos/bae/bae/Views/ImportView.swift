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
                selectedCandidate = candidate
                searchArtist = candidate.artistName
                searchAlbum = candidate.albumTitle
                searchResults = []
                candidateFiles = appService.getCandidateFiles(folderPath: candidate.folderPath)
            }
        )
    }

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
                }
                Button("Clear Completed") {
                    let wasSelected = selectedCandidate
                    appService.clearCompletedCandidates()
                    if let wasSelected,
                       !appService.scanResults.contains(where: { $0.folderPath == wasSelected.folderPath }) {
                        selectedCandidate = nil
                    }
                }
                Button("Clear Incomplete") {
                    let wasSelected = selectedCandidate
                    appService.clearIncompleteCandidates()
                    if let wasSelected,
                       !appService.scanResults.contains(where: { $0.folderPath == wasSelected.folderPath }) {
                        selectedCandidate = nil
                    }
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

    // MARK: - Metadata search

    private func metadataSearchView(for candidate: BridgeImportCandidate) -> some View {
        VStack(spacing: 0) {
            candidateHeader(candidate)
            Divider()
            if let files = candidateFiles {
                HSplitView {
                    filePane(files)
                        .frame(minWidth: 200)
                    VStack(spacing: 0) {
                        ImportSearchForm(
                            searchTab: $searchTab,
                            searchArtist: $searchArtist,
                            searchAlbum: $searchAlbum,
                            searchCatalog: $searchCatalog,
                            searchBarcode: $searchBarcode,
                            isSearching: isSearching,
                            onSearchMusicbrainz: { searchMusicbrainz() },
                            onSearchDiscogs: { searchDiscogs() },
                            onSearchCatalog: { searchByCatalogNumber() },
                            onSearchBarcode: { searchByBarcode() }
                        )
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
                    .frame(minWidth: 300)
                }
            } else {
                ImportSearchForm(
                    searchTab: $searchTab,
                    searchArtist: $searchArtist,
                    searchAlbum: $searchAlbum,
                    searchCatalog: $searchCatalog,
                    searchBarcode: $searchBarcode,
                    isSearching: isSearching,
                    onSearchMusicbrainz: { searchMusicbrainz() },
                    onSearchDiscogs: { searchDiscogs() },
                    onSearchCatalog: { searchByCatalogNumber() },
                    onSearchBarcode: { searchByBarcode() }
                )
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
        .animation(nil, value: selectedCandidate?.folderPath)
    }

    private func candidateHeader(_ candidate: BridgeImportCandidate) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(candidate.albumTitle)
                    .font(.headline)
                    .textSelection(.enabled)
                    .help(candidate.folderPath)
                Text("\(candidate.trackCount) tracks · \(candidate.format) · \(formatBytes(candidate.totalSizeBytes))")
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

    // MARK: - Results list

    private func resultsList(for candidate: BridgeImportCandidate) -> some View {
        List(searchResults, id: \.releaseId) { result in
            MetadataResultRow(
                result: result,
                isImporting: isImporting(candidate.folderPath),
                onImport: { commitImport(candidate: candidate, result: result) }
            )
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

// MARK: - ImportSearchForm (pure leaf)

struct ImportSearchForm: View {
    @Binding var searchTab: ImportView.SearchTab
    @Binding var searchArtist: String
    @Binding var searchAlbum: String
    @Binding var searchCatalog: String
    @Binding var searchBarcode: String
    let isSearching: Bool
    let onSearchMusicbrainz: () -> Void
    let onSearchDiscogs: () -> Void
    let onSearchCatalog: () -> Void
    let onSearchBarcode: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            Picker("", selection: $searchTab) {
                Text("General").tag(ImportView.SearchTab.general)
                Text("Catalog #").tag(ImportView.SearchTab.catalogNumber)
                Text("Barcode").tag(ImportView.SearchTab.barcode)
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
                        onSearchMusicbrainz()
                    }
                    .disabled(searchArtist.isEmpty && searchAlbum.isEmpty)
                    Button("Discogs") {
                        onSearchDiscogs()
                    }
                    .disabled(searchArtist.isEmpty && searchAlbum.isEmpty)
                }
            case .catalogNumber:
                HStack {
                    TextField("e.g. WPCR-80001", text: $searchCatalog)
                        .textFieldStyle(.roundedBorder)
                    Button("Search") {
                        onSearchCatalog()
                    }
                    .disabled(searchCatalog.isEmpty)
                }
            case .barcode:
                HStack {
                    TextField("e.g. 4943674251780", text: $searchBarcode)
                        .textFieldStyle(.roundedBorder)
                    Button("Search") {
                        onSearchBarcode()
                    }
                    .disabled(searchBarcode.isEmpty)
                }
            }
        }
        .padding()
        .animation(nil, value: searchTab)
    }
}

// MARK: - MetadataResultRow (pure leaf)

struct MetadataResultRow: View {
    let result: BridgeMetadataResult
    let isImporting: Bool
    let onImport: () -> Void

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
                }
                .font(.caption)
                Text(result.source == "musicbrainz" ? "MusicBrainz" : "Discogs")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Button("Import") {
                onImport()
            }
            .buttonStyle(.borderedProminent)
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
            badImageCount: 0
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
            badImageCount: 0
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
            badImageCount: 1
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
            badImageCount: 0
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
        isImporting: false,
        onImport: {}
    )
    .padding()
}

#Preview("Import Search Form") {
    ImportSearchForm(
        searchTab: .constant(.general),
        searchArtist: .constant("Artist Name"),
        searchAlbum: .constant("Album Title"),
        searchCatalog: .constant(""),
        searchBarcode: .constant(""),
        isSearching: false,
        onSearchMusicbrainz: {},
        onSearchDiscogs: {},
        onSearchCatalog: {},
        onSearchBarcode: {}
    )
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
