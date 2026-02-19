import SwiftUI

struct AlbumDetailView: View {
    let albumId: String
    let appService: AppService

    @State private var detail: BridgeAlbumDetail?
    @State private var error: String?
    @State private var selectedReleaseIndex: Int = 0

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
    }

    private func albumHeader(_ detail: BridgeAlbumDetail) -> some View {
        HStack(alignment: .top, spacing: 20) {
            albumArt(detail.album)
                .frame(width: 300, height: 300)
                .clipShape(RoundedRectangle(cornerRadius: 8))

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
