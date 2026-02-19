import SwiftUI

struct QueueView: View {
    let appService: AppService
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()

            if appService.isActive {
                nowPlayingSection
                Divider()
            }

            if appService.queueItems.isEmpty {
                ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: Text("Play an album to fill the queue")
                )
                .frame(maxHeight: .infinity)
            } else {
                List {
                    ForEach(Array(appService.queueItems.enumerated()), id: \.element.trackId) { index, item in
                        queueItemRow(item, index: index)
                    }
                    .onMove { from, to in
                        guard let fromIndex = from.first else { return }
                        appService.reorderQueue(
                            fromIndex: UInt32(fromIndex),
                            toIndex: UInt32(to > fromIndex ? to - 1 : to)
                        )
                    }
                }
                .scrollContentBackground(.hidden)
                .background(Theme.background)
            }
        }
        .background(Theme.surface)
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("Queue")
                .font(.headline)
            Spacer()
            Button("Clear") { appService.clearQueue() }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(appService.queueItems.isEmpty)
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding()
    }

    // MARK: - Now Playing

    private var nowPlayingSection: some View {
        HStack(spacing: 12) {
            nowPlayingArt
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))

            VStack(alignment: .leading, spacing: 2) {
                Text("Now Playing")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Text(appService.trackTitle ?? "")
                    .font(.callout)
                    .fontWeight(.medium)
                    .lineLimit(1)
                Text(appService.artistNames ?? "")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var nowPlayingArt: some View {
        if let coverImageId = appService.coverImageId,
           let urlString = appService.appHandle.getImageUrl(imageId: coverImageId),
           let url = URL(string: urlString) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().aspectRatio(contentMode: .fill)
                default:
                    artPlaceholder
                }
            }
        } else {
            artPlaceholder
        }
    }

    // MARK: - Queue Items

    private func queueItemRow(_ item: BridgeQueueItem, index: Int) -> some View {
        HStack(spacing: 10) {
            queueItemArt(coverImageId: item.coverImageId)
                .frame(width: 40, height: 40)
                .clipShape(RoundedRectangle(cornerRadius: 3))

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.callout)
                    .lineLimit(1)
                Text(item.albumTitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            if let durationMs = item.durationMs {
                Text(formatDuration(durationMs))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            appService.skipToQueueIndex(index: UInt32(index))
        }
        .contextMenu {
            Button("Remove from Queue") {
                appService.removeFromQueue(index: UInt32(index))
            }
        }
    }

    @ViewBuilder
    private func queueItemArt(coverImageId: String?) -> some View {
        if let coverImageId,
           let urlString = appService.appHandle.getImageUrl(imageId: coverImageId),
           let url = URL(string: urlString) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().aspectRatio(contentMode: .fill)
                default:
                    artPlaceholder
                }
            }
        } else {
            artPlaceholder
        }
    }

    private var artPlaceholder: some View {
        ZStack {
            Theme.placeholder
            Image(systemName: "photo")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
    }

    // MARK: - Formatting

    private func formatDuration(_ ms: Int64) -> String {
        let totalSeconds = ms / 1000
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        return "\(minutes):\(String(format: "%02d", seconds))"
    }
}
