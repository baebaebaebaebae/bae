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
    @State private var showingManageSheet: Bool = false
    @State private var showingDeleteConfirmation: Bool = false

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
                    lightboxItems: buildLightboxItems(detail),
                    selectedReleaseIndex: $selectedReleaseIndex,
                    showShareCopied: showShareCopied,
                    onClose: onClose,
                    onPlay: { appService.playAlbum(albumId: albumId) },
                    onShuffle: { appService.playAlbum(albumId: albumId, shuffle: true) },
                    onPlayFromTrack: { index in
                        appService.playAlbum(albumId: albumId, startTrackIndex: UInt32(index))
                    },
                    onShare: {
                        guard !detail.releases.isEmpty else { return }
                        createShareLink(releaseId: detail.releases[selectedReleaseIndex].id)
                    },
                    onAddNext: { trackId in appService.addNext(trackIds: [trackId]) },
                    onAddToQueue: { trackId in appService.addToQueue(trackIds: [trackId]) },
                    onChangeCover: {
                        showingCoverSheet = true
                        fetchRemoteCovers()
                    },
                    onManage: { showingManageSheet = true },
                    onDeleteAlbum: { showingDeleteConfirmation = true }
                )
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: albumId) {
            await loadDetail()
        }
        .windowOverlay(isPresented: showingCoverSheet && detail != nil) {
            if let detail {
                ZStack {
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
        .alert("Delete Album", isPresented: $showingDeleteConfirmation) {
            Button("Delete", role: .destructive) {
                // TODO: Wire to appService.appHandle.deleteAlbum(albumId:) once bridge method exists
                onClose?()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Are you sure you want to delete this album? This cannot be undone.")
        }
        .windowOverlay(isPresented: showingManageSheet && detail != nil) {
            if let detail {
                let release = detail.releases.isEmpty ? nil : detail.releases[selectedReleaseIndex]
                ZStack {
                    Color.black.opacity(0.5)
                        .ignoresSafeArea()
                        .onTapGesture { showingManageSheet = false }
                    if let release {
                        ManageReleaseSheet(
                            release: release,
                            transferring: transferring,
                            onTransferToManaged: { transferToManaged(releaseId: release.id) },
                            onEject: { ejectRelease(releaseId: release.id) },
                            onDone: { showingManageSheet = false }
                        )
                        .frame(width: 450, height: 400)
                        .background(Theme.surface)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        .shadow(radius: 20)
                    }
                }
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

    private func buildLightboxItems(_ detail: BridgeAlbumDetail) -> [LightboxItem] {
        var items: [LightboxItem] = []

        if let coverURL = appService.imageURL(for: detail.album.coverReleaseId) {
            items.append(LightboxItem(id: "cover", label: "Cover", url: coverURL))
        }

        if !detail.releases.isEmpty {
            let release = detail.releases[selectedReleaseIndex]
            for file in release.files where file.contentType.hasPrefix("image/") {
                items.append(LightboxItem(
                    id: file.id,
                    label: file.originalFilename,
                    url: appService.fileURL(for: file.id)
                ))
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
    let lightboxItems: [LightboxItem]
    @Binding var selectedReleaseIndex: Int
    let showShareCopied: Bool
    let onClose: (() -> Void)?
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (Int) -> Void
    let onShare: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onChangeCover: () -> Void
    let onManage: () -> Void
    let onDeleteAlbum: () -> Void

    @State private var hoverTrackId: String?
    @State private var lightboxIndex: Int?

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
                }
            }
            .padding()
        }
        .background(Theme.background)
        .windowOverlay(isPresented: lightboxIndex != nil && !lightboxItems.isEmpty) {
            ImageLightbox(items: lightboxItems, currentIndex: $lightboxIndex)
        }
    }

    private var albumHeader: some View {
        HStack(alignment: .top, spacing: 16) {
            albumArt
                .frame(width: 160, height: 160)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .contentShape(Rectangle())
                .onTapGesture {
                    if !lightboxItems.isEmpty {
                        lightboxIndex = 0
                    }
                }
                .overlay(alignment: .bottomTrailing) {
                    Menu {
                        Button("Change Cover...") { onChangeCover() }
                        Button("Storage...") { onManage() }
                        Divider()
                        Button("Delete Album", role: .destructive) { onDeleteAlbum() }
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.caption)
                            .foregroundStyle(.white)
                            .frame(width: 24, height: 24)
                            .background(.black.opacity(0.6))
                            .clipShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .padding(6)
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

                    Button(action: onShuffle) {
                        Label("Shuffle", systemImage: "shuffle")
                    }
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

        let totalDurationMs = sortedTracks.compactMap(\.durationMs).reduce(0, +)

        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(sortedTracks.enumerated()), id: \.element.id) { index, track in
                trackRow(
                    track,
                    showArtist: isCompilation,
                    showDisc: hasMultipleDiscs,
                    isHovered: hoverTrackId == track.id,
                    onPlay: { onPlayFromTrack(index) }
                )
                Divider()
            }

            if totalDurationMs > 0 {
                Text(formatTotalDuration(totalDurationMs))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .padding(.top, 8)
            }
        }
    }

    private func trackRow(
        _ track: BridgeTrack,
        showArtist: Bool,
        showDisc: Bool,
        isHovered: Bool,
        onPlay: @escaping () -> Void
    ) -> some View {
        HStack(spacing: 12) {
            ZStack {
                if isHovered {
                    Button(action: onPlay) {
                        Image(systemName: "play.fill")
                            .font(.callout)
                    }
                    .buttonStyle(.plain)
                } else {
                    trackNumberLabel(track, showDisc: showDisc)
                }
            }
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
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            onPlay()
        }
        .onHover { hovering in
            hoverTrackId = hovering ? track.id : nil
        }
        .contextMenu {
            Button("Play") { onPlay() }
            Button("Play Next") { onAddNext(track.id) }
            Button("Add to Queue") { onAddToQueue(track.id) }
        }
        .focusable()
        .draggable(track.id)
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

    // MARK: - Formatting

    private func formatDuration(_ ms: Int64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return "\(minutes):\(String(format: "%02d", seconds))"
    }

    private func formatTotalDuration(_ ms: Int64) -> String {
        let totalMinutes = ms / 1000 / 60
        if totalMinutes >= 60 {
            let hours = totalMinutes / 60
            let mins = totalMinutes % 60
            return mins > 0 ? "\(hours) hr \(mins) min" : "\(hours) hr"
        }
        return "\(totalMinutes) min"
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

// MARK: - ManageReleaseSheet

struct ManageReleaseSheet: View {
    let release: BridgeRelease
    let transferring: Bool
    let onTransferToManaged: () -> Void
    let onEject: () -> Void
    let onDone: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Storage")
                    .font(.headline)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Storage status
                    storageStatus

                    // Transfer actions
                    if transferring {
                        HStack(spacing: 8) {
                            ProgressView()
                                .controlSize(.small)
                            Text("Transferring...")
                                .font(.callout)
                                .foregroundStyle(.secondary)
                        }
                    } else {
                        transferActions
                    }

                    if !release.files.isEmpty {
                        Divider()

                        Text("Files")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

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
                }
                .padding()
            }
        }
    }

    private var storageStatus: some View {
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
    private var transferActions: some View {
        let isUnmanaged = !release.managedLocally && !release.managedInCloud && release.unmanagedPath != nil

        if isUnmanaged {
            Button(action: onTransferToManaged) {
                Label("Copy to library", systemImage: "square.and.arrow.down")
            }
            .help("Copy files into managed local storage")
        }

        if release.managedLocally {
            Button(action: onEject) {
                Label("Eject to folder", systemImage: "square.and.arrow.up.on.square")
            }
            .help("Export files to a local folder and remove from managed storage")
        }
    }

    private func formatFileSize(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: bytes)
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
        lightboxItems: [LightboxItem(id: "cover", label: "Cover", url: nil)],
        selectedReleaseIndex: .constant(0),
        showShareCopied: false,
        onClose: {},
        onPlay: {},
        onShuffle: {},
        onPlayFromTrack: { _ in },
        onShare: {},
        onAddNext: { _ in },
        onAddToQueue: { _ in },
        onChangeCover: {},
        onManage: {},
        onDeleteAlbum: {}
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
