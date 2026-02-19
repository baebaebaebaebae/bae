import SwiftUI
import AppKit

struct AlbumDetailView: View {
    let albumId: String
    let appService: AppService

    @State private var detail: BridgeAlbumDetail?
    @State private var error: String?
    @State private var selectedReleaseIndex: Int = 0
    @State private var showingCoverSheet: Bool = false
    @State private var remoteCovers: [BridgeRemoteCover] = []
    @State private var loadingRemoteCovers: Bool = false
    @State private var coverChangeError: String?
    @State private var shareError: String?
    @State private var showShareCopied: Bool = false

    var body: some View {
        Group {
            if let error {
                ContentUnavailableView(
                    "Failed to load album",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else if let detail {
                albumContent(detail)
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: albumId) {
            await loadDetail()
        }
        .sheet(isPresented: $showingCoverSheet) {
            if let detail {
                coverSheet(detail)
            }
        }
        .alert("Cover Change Failed", isPresented: .init(
            get: { coverChangeError != nil },
            set: { if !$0 { coverChangeError = nil } }
        )) {
            Button("OK") { coverChangeError = nil }
        } message: {
            if let err = coverChangeError {
                Text(err)
            }
        }
        .alert("Share Failed", isPresented: .init(
            get: { shareError != nil },
            set: { if !$0 { shareError = nil } }
        )) {
            Button("OK") { shareError = nil }
        } message: {
            if let err = shareError {
                Text(err)
            }
        }
        .overlay(alignment: .bottom) {
            if showShareCopied {
                Text("Share link copied to clipboard")
                    .font(.callout)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .padding(.bottom, 16)
            }
        }
    }

    private func albumContent(_ detail: BridgeAlbumDetail) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                albumHeader(detail)

                if detail.releases.count > 1 {
                    releasePicker(detail)
                }

                if !detail.releases.isEmpty {
                    let release = detail.releases[selectedReleaseIndex]
                    trackList(release, isCompilation: detail.album.isCompilation)

                    if !release.files.isEmpty {
                        fileSection(release)
                    }
                }
            }
            .padding()
        }
        .toolbar {
            ToolbarItemGroup {
                if !detail.releases.isEmpty {
                    let release = detail.releases[selectedReleaseIndex]

                    Button(action: { createShareLink(releaseId: release.id) }) {
                        Label("Share", systemImage: "square.and.arrow.up")
                    }
                    .help("Create share link")
                }
            }
        }
    }

    private func albumHeader(_ detail: BridgeAlbumDetail) -> some View {
        HStack(alignment: .top, spacing: 20) {
            albumArt(detail.album)
                .frame(width: 300, height: 300)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .contextMenu {
                    Button("Change Cover...") {
                        showingCoverSheet = true
                        fetchRemoteCovers()
                    }
                }

            VStack(alignment: .leading, spacing: 8) {
                Text(detail.album.title)
                    .font(.largeTitle)
                    .fontWeight(.bold)

                Text(detail.album.artistNames)
                    .font(.title2)
                    .foregroundStyle(.secondary)

                if let year = detail.album.year {
                    Text(String(year))
                        .font(.title3)
                        .foregroundStyle(.tertiary)
                }

                if let release = detail.releases.first {
                    releaseMetadata(release)
                }

                Button(action: {
                    appService.playAlbum(albumId: albumId)
                }) {
                    Label("Play", systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .padding(.top, 8)
            }

            Spacer()
        }
    }

    private func releaseMetadata(_ release: BridgeRelease) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            if let format = release.format {
                metadataRow("Format", value: format)
            }
            if let label = release.label {
                metadataRow("Label", value: label)
            }
            if let catalog = release.catalogNumber {
                metadataRow("Catalog", value: catalog)
            }
            if let country = release.country {
                metadataRow("Country", value: country)
            }
        }
        .font(.callout)
        .padding(.top, 4)
    }

    private func metadataRow(_ label: String, value: String) -> some View {
        HStack(spacing: 6) {
            Text(label + ":")
                .foregroundStyle(.secondary)
            Text(value)
        }
    }

    @ViewBuilder
    private func albumArt(_ album: BridgeAlbum) -> some View {
        if let coverReleaseId = album.coverReleaseId,
           let urlString = appService.appHandle.getImageUrl(imageId: coverReleaseId),
           let url = URL(string: urlString) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                case .failure:
                    albumArtPlaceholder
                default:
                    Color(.separatorColor)
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Color(.separatorColor)
            Image(systemName: "photo")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
        }
    }

    private func releasePicker(_ detail: BridgeAlbumDetail) -> some View {
        Picker("Release", selection: $selectedReleaseIndex) {
            ForEach(Array(detail.releases.enumerated()), id: \.offset) { index, release in
                Text(releaseDisplayName(release))
                    .tag(index)
            }
        }
        .pickerStyle(.segmented)
        .onChange(of: selectedReleaseIndex) { _, newValue in
            if newValue >= detail.releases.count {
                selectedReleaseIndex = 0
            }
        }
    }

    private func releaseDisplayName(_ release: BridgeRelease) -> String {
        if let name = release.releaseName {
            return name
        }
        if let format = release.format {
            return format
        }
        return "Release"
    }

    private func trackList(_ release: BridgeRelease, isCompilation: Bool) -> some View {
        let sortedTracks = release.tracks.sorted { a, b in
            let discA = a.discNumber ?? 1
            let discB = b.discNumber ?? 1
            if discA != discB { return discA < discB }
            let trackA = a.trackNumber ?? 0
            let trackB = b.trackNumber ?? 0
            return trackA < trackB
        }

        let hasMultipleDiscs = Set(release.tracks.compactMap(\.discNumber)).count > 1

        return VStack(alignment: .leading, spacing: 0) {
            Text("Tracks")
                .font(.headline)
                .padding(.bottom, 8)

            ForEach(Array(sortedTracks.enumerated()), id: \.element.id) { index, track in
                trackRow(
                    track,
                    showArtist: isCompilation,
                    showDisc: hasMultipleDiscs,
                    onPlay: {
                        // Play album starting from this track
                        appService.playAlbum(albumId: albumId, startTrackIndex: UInt32(index))
                    }
                )
                Divider()
            }
        }
    }

    private func trackRow(
        _ track: BridgeTrack,
        showArtist: Bool,
        showDisc: Bool,
        onPlay: @escaping () -> Void
    ) -> some View {
        HStack(spacing: 12) {
            trackNumberLabel(track, showDisc: showDisc)
                .frame(width: 40, alignment: .trailing)
                .foregroundStyle(.secondary)
                .font(.callout.monospacedDigit())

            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                    .font(.body)
                    .lineLimit(1)

                if showArtist {
                    Text(track.artistNames)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()

            if let durationMs = track.durationMs {
                Text(formatDuration(durationMs))
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            onPlay()
        }
    }

    private func trackNumberLabel(_ track: BridgeTrack, showDisc: Bool) -> some View {
        if showDisc, let disc = track.discNumber, let num = track.trackNumber {
            Text("\(disc).\(num)")
        } else if let num = track.trackNumber {
            Text("\(num)")
        } else {
            Text("-")
        }
    }

    private func fileSection(_ release: BridgeRelease) -> some View {
        DisclosureGroup("Files") {
            ForEach(release.files, id: \.id) { file in
                HStack {
                    Text(file.originalFilename)
                        .font(.callout)
                        .lineLimit(1)
                    Spacer()
                    Text(formatFileSize(file.fileSize))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Text(file.contentType)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .padding(.vertical, 2)
            }
        }
        .font(.headline)
    }

    // MARK: - Cover Sheet

    private func coverSheet(_ detail: BridgeAlbumDetail) -> some View {
        VStack(spacing: 0) {
            HStack {
                Text("Change Cover")
                    .font(.headline)
                Spacer()
                Button("Done") { showingCoverSheet = false }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Remote covers section
                    Text("Remote Sources")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)

                    if loadingRemoteCovers {
                        HStack {
                            ProgressView()
                                .controlSize(.small)
                            Text("Fetching covers...")
                                .font(.callout)
                                .foregroundStyle(.secondary)
                        }
                    } else if remoteCovers.isEmpty {
                        Text("No remote covers found")
                            .font(.callout)
                            .foregroundStyle(.tertiary)
                    } else {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120))], spacing: 12) {
                            ForEach(remoteCovers, id: \.url) { cover in
                                remoteCoverOption(cover, detail: detail)
                            }
                        }
                    }

                    Button(action: { fetchRemoteCovers() }) {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .disabled(loadingRemoteCovers)

                    // Image files from releases
                    let imageFiles = collectImageFiles(detail)
                    if !imageFiles.isEmpty {
                        Divider()

                        Text("Release Files")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120))], spacing: 12) {
                            ForEach(imageFiles, id: \.file.id) { item in
                                releaseFileOption(item, detail: detail)
                            }
                        }
                    }
                }
                .padding()
            }
        }
        .frame(minWidth: 400, minHeight: 350)
    }

    private func remoteCoverOption(_ cover: BridgeRemoteCover, detail: BridgeAlbumDetail) -> some View {
        Button(action: {
            let releaseId = currentReleaseId(detail)
            changeCover(
                albumId: albumId,
                releaseId: releaseId,
                selection: .remoteCover(url: cover.url, source: cover.source)
            )
        }) {
            VStack(spacing: 4) {
                AsyncImage(url: URL(string: cover.thumbnailUrl)) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .aspectRatio(contentMode: .fill)
                    case .failure:
                        coverOptionPlaceholder
                    default:
                        Color(.separatorColor)
                    }
                }
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 6))

                Text(cover.label)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
    }

    private func releaseFileOption(_ item: ImageFileItem, detail: BridgeAlbumDetail) -> some View {
        Button(action: {
            let releaseId = currentReleaseId(detail)
            changeCover(
                albumId: albumId,
                releaseId: releaseId,
                selection: .releaseImage(fileId: item.file.id)
            )
        }) {
            VStack(spacing: 4) {
                coverOptionPlaceholder
                    .frame(width: 120, height: 120)
                    .clipShape(RoundedRectangle(cornerRadius: 6))

                Text(item.file.originalFilename)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
    }

    private var coverOptionPlaceholder: some View {
        ZStack {
            Color(.separatorColor)
            Image(systemName: "photo")
                .font(.title2)
                .foregroundStyle(.tertiary)
        }
    }

    private func currentReleaseId(_ detail: BridgeAlbumDetail) -> String {
        guard !detail.releases.isEmpty else { return "" }
        return detail.releases[selectedReleaseIndex].id
    }

    // MARK: - Data helpers

    private struct ImageFileItem {
        let file: BridgeFile
    }

    private func collectImageFiles(_ detail: BridgeAlbumDetail) -> [ImageFileItem] {
        var items: [ImageFileItem] = []
        for release in detail.releases {
            for file in release.files {
                if file.contentType.hasPrefix("image/") {
                    items.append(ImageFileItem(file: file))
                }
            }
        }
        return items
    }

    // MARK: - Actions

    private func fetchRemoteCovers() {
        guard let detail else { return }
        let releaseId = currentReleaseId(detail)
        guard !releaseId.isEmpty else { return }

        loadingRemoteCovers = true
        remoteCovers = []

        Task.detached { [appHandle = appService.appHandle] in
            let covers = (try? appHandle.fetchRemoteCovers(releaseId: releaseId)) ?? []
            await MainActor.run {
                remoteCovers = covers
                loadingRemoteCovers = false
            }
        }
    }

    private func changeCover(albumId: String, releaseId: String, selection: BridgeCoverSelection) {
        Task.detached { [appHandle = appService.appHandle] in
            do {
                try appHandle.changeCover(albumId: albumId, releaseId: releaseId, selection: selection)
                await MainActor.run {
                    showingCoverSheet = false
                    // Reload detail to refresh cover art
                    Task { await loadDetail() }
                }
            } catch {
                await MainActor.run {
                    coverChangeError = error.localizedDescription
                }
            }
        }
    }

    private func createShareLink(releaseId: String) {
        Task.detached { [appHandle = appService.appHandle] in
            do {
                let url = try appHandle.createShareLink(releaseId: releaseId)
                await MainActor.run {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(url, forType: .string)
                    withAnimation {
                        showShareCopied = true
                    }
                    // Hide after 2 seconds
                    Task {
                        try? await Task.sleep(for: .seconds(2))
                        withAnimation {
                            showShareCopied = false
                        }
                    }
                }
            } catch {
                await MainActor.run {
                    shareError = error.localizedDescription
                }
            }
        }
    }

    // MARK: - Formatting

    private func formatDuration(_ ms: Int64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return "\(minutes):\(String(format: "%02d", seconds))"
    }

    private func formatFileSize(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: bytes)
    }

    private func loadDetail() async {
        detail = nil
        error = nil
        selectedReleaseIndex = 0

        do {
            let result = try await Task.detached {
                try appService.appHandle.getAlbumDetail(albumId: albumId)
            }.value
            detail = result
        } catch {
            self.error = error.localizedDescription
        }
    }
}
