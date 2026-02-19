import SwiftUI
import AppKit

// MARK: - Smart wiring view (keeps appService)

struct AlbumDetailView: View {
    let albumId: String
    let appService: AppService
    var onClose: (() -> Void)?

    @State private var detail: BridgeAlbumDetail?
    @State private var error: String?
    @State private var selectedReleaseIndex: Int = 0
    @State private var showingCoverSheet: Bool = false
    @State private var remoteCovers: [BridgeRemoteCover] = []
    @State private var loadingRemoteCovers: Bool = false
    @State private var coverChangeError: String?
    @State private var shareError: String?
    @State private var showShareCopied: Bool = false
    @State private var transferring: Bool = false
    @State private var transferError: String?

    var body: some View {
        Group {
            if let error {
                ContentUnavailableView(
                    "Failed to load album",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else if let detail {
                AlbumDetailContent(
                    detail: detail,
                    coverArtURL: appService.imageURL(for: detail.album.coverReleaseId),
                    selectedReleaseIndex: $selectedReleaseIndex,
                    showShareCopied: showShareCopied,
                    transferring: transferring,
                    onClose: onClose,
                    onPlay: { appService.playAlbum(albumId: albumId) },
                    onPlayFromTrack: { index in
                        appService.playAlbum(albumId: albumId, startTrackIndex: UInt32(index))
                    },
                    onShare: {
                        guard !detail.releases.isEmpty else { return }
                        createShareLink(releaseId: detail.releases[selectedReleaseIndex].id)
                    },
                    onChangeCover: {
                        showingCoverSheet = true
                        fetchRemoteCovers()
                    },
                    onTransferToManaged: { transferToManaged(releaseId: $0) },
                    onEject: { ejectRelease(releaseId: $0) }
                )
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: albumId) {
            await loadDetail()
        }
        .overlay {
            if showingCoverSheet, let detail {
                Color.black.opacity(0.5)
                    .ignoresSafeArea()
                    .onTapGesture { showingCoverSheet = false }
                CoverSheetView(
                    remoteCovers: remoteCovers,
                    releaseImages: collectImageFiles(detail).map { item in
                        (id: item.file.id, name: item.file.originalFilename, url: nil as URL?)
                    },
                    loading: loadingRemoteCovers,
                    onSelectRemote: { cover in
                        let releaseId = currentReleaseId(detail)
                        changeCover(
                            albumId: albumId,
                            releaseId: releaseId,
                            selection: .remoteCover(url: cover.url, source: cover.source)
                        )
                    },
                    onSelectReleaseImage: { fileId in
                        let releaseId = currentReleaseId(detail)
                        changeCover(
                            albumId: albumId,
                            releaseId: releaseId,
                            selection: .releaseImage(fileId: fileId)
                        )
                    },
                    onRefresh: { fetchRemoteCovers() },
                    onDone: { showingCoverSheet = false }
                )
                .frame(width: 500, height: 450)
                .background(Theme.surface)
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .shadow(radius: 20)
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
        .alert("Transfer Failed", isPresented: .init(
            get: { transferError != nil },
            set: { if !$0 { transferError = nil } }
        )) {
            Button("OK") { transferError = nil }
        } message: {
            if let err = transferError {
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

    private func currentReleaseId(_ detail: BridgeAlbumDetail) -> String {
        guard !detail.releases.isEmpty else { return "" }
        return detail.releases[selectedReleaseIndex].id
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

    private func transferToManaged(releaseId: String) {
        transferring = true
        transferError = nil
        Task.detached { [appHandle = appService.appHandle] in
            do {
                try appHandle.transferReleaseToManaged(releaseId: releaseId)
                await MainActor.run {
                    transferring = false
                    Task { await loadDetail() }
                }
            } catch {
                await MainActor.run {
                    transferring = false
                    transferError = error.localizedDescription
                }
            }
        }
    }

    private func ejectRelease(releaseId: String) {
        let panel = NSOpenPanel()
        panel.title = "Select Eject Directory"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false

        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }

        transferring = true
        transferError = nil
        let targetDir = url.path
        Task.detached { [appHandle = appService.appHandle] in
            do {
                try appHandle.ejectReleaseStorage(releaseId: releaseId, targetDir: targetDir)
                await MainActor.run {
                    transferring = false
                    Task { await loadDetail() }
                }
            } catch {
                await MainActor.run {
                    transferring = false
                    transferError = error.localizedDescription
                }
            }
        }
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

// MARK: - AlbumDetailContent (pure leaf view)

struct AlbumDetailContent: View {
    let detail: BridgeAlbumDetail
    let coverArtURL: URL?
    @Binding var selectedReleaseIndex: Int
    let showShareCopied: Bool
    let transferring: Bool
    let onClose: (() -> Void)?
    let onPlay: () -> Void
    let onPlayFromTrack: (Int) -> Void
    let onShare: () -> Void
    let onChangeCover: () -> Void
    let onTransferToManaged: (String) -> Void
    let onEject: (String) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                albumHeader

                if detail.releases.count > 1 {
                    releasePicker
                }

                if !detail.releases.isEmpty {
                    let release = detail.releases[selectedReleaseIndex]
                    trackList(release, isCompilation: detail.album.isCompilation)

                    if !release.files.isEmpty {
                        fileSection(release)
                    }

                    storageSection(release)
                }
            }
            .padding()
        }
        .background(Theme.background)
    }

    private var albumHeader: some View {
        HStack(alignment: .top, spacing: 16) {
            albumArt
                .frame(width: 100, height: 100)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .contextMenu {
                    Button("Change Cover...") {
                        onChangeCover()
                    }
                }

            VStack(alignment: .leading, spacing: 4) {
                Text(detail.album.title)
                    .font(.headline)
                    .lineLimit(1)

                HStack(spacing: 8) {
                    Text(detail.album.artistNames)
                        .foregroundStyle(.secondary)
                    if let year = detail.album.year {
                        Text("·")
                            .foregroundStyle(.tertiary)
                        Text(String(year))
                            .foregroundStyle(.tertiary)
                    }
                }
                .font(.callout)
                .lineLimit(1)

                if let release = detail.releases.first {
                    releaseMetadataCompact(release)
                }

                HStack(spacing: 8) {
                    Button(action: onPlay) {
                        Label("Play", systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)

                    if !detail.releases.isEmpty {
                        Button(action: onShare) {
                            Label("Share", systemImage: "square.and.arrow.up")
                        }
                        .controlSize(.small)
                    }
                }
                .padding(.top, 2)
            }

            Spacer()

            if let onClose {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private var albumArt: some View {
        if let url = coverArtURL {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                case .failure:
                    albumArtPlaceholder
                default:
                    Theme.placeholder
                }
            }
        } else {
            albumArtPlaceholder
        }
    }

    private var albumArtPlaceholder: some View {
        ZStack {
            Theme.placeholder
            Image(systemName: "photo")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
        }
    }

    private func releaseMetadataCompact(_ release: BridgeRelease) -> some View {
        HStack(spacing: 12) {
            if let format = release.format {
                Text(format)
            }
            if let label = release.label {
                Text(label)
            }
            if let catalog = release.catalogNumber {
                Text(catalog)
            }
            if let country = release.country {
                Text(country)
            }
        }
        .font(.caption)
        .foregroundStyle(.tertiary)
        .lineLimit(1)
    }

    private var releasePicker: some View {
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
                    onPlay: { onPlayFromTrack(index) }
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

    // MARK: - Storage

    private func storageSection(_ release: BridgeRelease) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Storage")
                .font(.headline)

            HStack(spacing: 8) {
                storageStatusLabel(release)

                Spacer()

                if transferring {
                    ProgressView()
                        .controlSize(.small)
                    Text("Transferring...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                } else {
                    storageActions(release)
                }
            }
        }
    }

    private func storageStatusLabel(_ release: BridgeRelease) -> some View {
        let isUnmanaged = !release.managedLocally && !release.managedInCloud && release.unmanagedPath != nil

        return HStack(spacing: 6) {
            if release.managedLocally && release.managedInCloud {
                Image(systemName: "internaldrive")
                Text("Local + Cloud")
            } else if release.managedLocally {
                Image(systemName: "internaldrive")
                Text("Managed locally")
            } else if release.managedInCloud {
                Image(systemName: "cloud")
                Text("Cloud storage")
            } else if isUnmanaged {
                Image(systemName: "folder")
                Text("Unmanaged")
            } else {
                Image(systemName: "folder")
                Text("No storage")
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func storageActions(_ release: BridgeRelease) -> some View {
        let isUnmanaged = !release.managedLocally && !release.managedInCloud && release.unmanagedPath != nil

        if isUnmanaged {
            Button(action: { onTransferToManaged(release.id) }) {
                Label("Copy to library", systemImage: "square.and.arrow.down")
            }
            .help("Copy files into managed local storage")
        }

        if release.managedLocally {
            Button(action: { onEject(release.id) }) {
                Label("Eject to folder", systemImage: "square.and.arrow.up.on.square")
            }
            .help("Export files to a local folder and remove from managed storage")
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
}

// MARK: - CoverSheetView (pure leaf view)

struct CoverSheetView: View {
    let remoteCovers: [BridgeRemoteCover]
    let releaseImages: [(id: String, name: String, url: URL?)]
    let loading: Bool
    let onSelectRemote: (BridgeRemoteCover) -> Void
    let onSelectReleaseImage: (String) -> Void
    let onRefresh: () -> Void
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Change Cover")
                    .font(.headline)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Remote Sources")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)

                    if loading {
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
                                remoteCoverOption(cover)
                            }
                        }
                    }

                    Button(action: onRefresh) {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .disabled(loading)

                    if !releaseImages.isEmpty {
                        Divider()

                        Text("Release Files")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120))], spacing: 12) {
                            ForEach(Array(releaseImages.enumerated()), id: \.offset) { _, item in
                                Button(action: { onSelectReleaseImage(item.id) }) {
                                    VStack(spacing: 4) {
                                        coverOptionPlaceholder
                                            .frame(width: 120, height: 120)
                                            .clipShape(RoundedRectangle(cornerRadius: 6))

                                        Text(item.name)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding()
            }
        }
    }

    private func remoteCoverOption(_ cover: BridgeRemoteCover) -> some View {
        Button(action: { onSelectRemote(cover) }) {
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
                        Theme.placeholder
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

    private var coverOptionPlaceholder: some View {
        ZStack {
            Theme.placeholder
            Image(systemName: "photo")
                .font(.title2)
                .foregroundStyle(.tertiary)
        }
    }
}

// MARK: - Previews

#Preview("Album Detail Content") {
    AlbumDetailContent(
        detail: BridgeAlbumDetail(
            album: BridgeAlbum(
                id: "album-1",
                title: "Album Title",
                year: 2024,
                isCompilation: false,
                coverReleaseId: nil,
                artistNames: "Artist Name"
            ),
            artists: [BridgeArtist(id: "ar-1", name: "Artist Name")],
            releases: [
                BridgeRelease(
                    id: "rel-1",
                    albumId: "album-1",
                    releaseName: nil,
                    year: 2024,
                    format: "CD",
                    label: "Label Name",
                    catalogNumber: "CAT-001",
                    country: "US",
                    managedLocally: true,
                    managedInCloud: false,
                    unmanagedPath: nil,
                    tracks: [
                        BridgeTrack(id: "t-1", title: "First Track", discNumber: 1, trackNumber: 1, durationMs: 234000, artistNames: "Artist Name"),
                        BridgeTrack(id: "t-2", title: "Second Track", discNumber: 1, trackNumber: 2, durationMs: 180000, artistNames: "Artist Name"),
                        BridgeTrack(id: "t-3", title: "Third Track", discNumber: 1, trackNumber: 3, durationMs: 312000, artistNames: "Artist Name"),
                    ],
                    files: []
                )
            ]
        ),
        coverArtURL: nil,
        selectedReleaseIndex: .constant(0),
        showShareCopied: false,
        transferring: false,
        onClose: {},
        onPlay: {},
        onPlayFromTrack: { _ in },
        onShare: {},
        onChangeCover: {},
        onTransferToManaged: { _ in },
        onEject: { _ in }
    )
    .frame(width: 450, height: 600)
}

#Preview("Cover Sheet") {
    CoverSheetView(
        remoteCovers: [
            BridgeRemoteCover(url: "https://example.com/cover1.jpg", thumbnailUrl: "https://example.com/thumb1.jpg", label: "Front", source: "musicbrainz"),
            BridgeRemoteCover(url: "https://example.com/cover2.jpg", thumbnailUrl: "https://example.com/thumb2.jpg", label: "Back", source: "musicbrainz"),
        ],
        releaseImages: [
            (id: "file-1", name: "cover.jpg", url: nil),
            (id: "file-2", name: "back.jpg", url: nil),
        ],
        loading: false,
        onSelectRemote: { _ in },
        onSelectReleaseImage: { _ in },
        onRefresh: {},
        onDone: {}
    )
    .frame(width: 500, height: 450)
    .background(Theme.surface)
}

#Preview("Cover Sheet Loading") {
    CoverSheetView(
        remoteCovers: [],
        releaseImages: [],
        loading: true,
        onSelectRemote: { _ in },
        onSelectReleaseImage: { _ in },
        onRefresh: {},
        onDone: {}
    )
    .frame(width: 500, height: 450)
    .background(Theme.surface)
}
