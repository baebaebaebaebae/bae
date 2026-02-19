import SwiftUI
import UniformTypeIdentifiers

struct QueueView: View {
    let appService: AppService
    let onClose: () -> Void
    @State private var hoveredIndex: Int?
    @State private var draggedTrackId: String?
    @State private var dropInsertIndex: Int?

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
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(Array(appService.queueItems.enumerated()), id: \.element.trackId) { index, item in
                            VStack(spacing: 0) {
                                // Insertion line above this row
                                if dropInsertIndex == index {
                                    insertionLine
                                }
                                queueItemRow(item, index: index)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 4)
                                    .opacity(draggedTrackId == item.trackId ? 0.3 : 1.0)
                                Divider().padding(.leading, 62)
                            }
                            .onDrop(of: [UTType.plainText], delegate: QueueDropDelegate(
                                targetIndex: index,
                                items: appService.queueItems,
                                draggedTrackId: $draggedTrackId,
                                dropInsertIndex: $dropInsertIndex,
                                onReorder: { from, to in
                                    appService.reorderQueue(
                                        fromIndex: UInt32(from),
                                        toIndex: UInt32(to)
                                    )
                                }
                            ))
                        }
                        // Insertion line at the very end
                        if dropInsertIndex == appService.queueItems.count {
                            insertionLine
                        }
                    }
                }
                .background(Theme.background)
            }
        }
        .background(Theme.surface)
    }

    private var insertionLine: some View {
        Rectangle()
            .fill(Color.accentColor)
            .frame(height: 2)
            .padding(.horizontal, 8)
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
            ZStack {
                queueItemArt(coverImageId: item.coverImageId)
                    .frame(width: 40, height: 40)
                    .clipShape(RoundedRectangle(cornerRadius: 3))

                if hoveredIndex == index {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(.black.opacity(0.5))
                        .frame(width: 40, height: 40)
                    Button(action: { appService.skipToQueueIndex(index: UInt32(index)) }) {
                        Image(systemName: "play.fill")
                            .font(.caption)
                            .foregroundColor(.white)
                    }
                    .buttonStyle(.plain)
                }
            }

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

            if hoveredIndex == index {
                Button(action: { appService.removeFromQueue(index: UInt32(index)) }) {
                    Image(systemName: "xmark")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help("Remove from queue")
            } else if let durationMs = item.durationMs {
                Text(formatDuration(durationMs))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .onHover { isHovered in
            hoveredIndex = isHovered ? index : nil
        }
        .onDrag {
            draggedTrackId = item.trackId
            return NSItemProvider(object: item.trackId as NSString)
        }
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

// MARK: - Drop Delegate

private struct QueueDropDelegate: DropDelegate {
    let targetIndex: Int
    let items: [BridgeQueueItem]
    @Binding var draggedTrackId: String?
    @Binding var dropInsertIndex: Int?
    let onReorder: (Int, Int) -> Void

    func dropEntered(info: DropInfo) {
        guard let draggedId = draggedTrackId,
              let fromIndex = items.firstIndex(where: { $0.trackId == draggedId }) else { return }

        // Show insertion line: above targetIndex when dragging up, below when dragging down
        if targetIndex > fromIndex {
            dropInsertIndex = targetIndex + 1
        } else if targetIndex < fromIndex {
            dropInsertIndex = targetIndex
        } else {
            dropInsertIndex = nil
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        guard let draggedId = draggedTrackId,
              let fromIndex = items.firstIndex(where: { $0.trackId == draggedId }) else { return false }

        // When dragging down, pass targetIndex + 1 because PlaybackQueue.reorder()
        // does insert(to - 1) after remove(from) to compensate for the index shift.
        let toIndex = targetIndex > fromIndex ? targetIndex + 1 : targetIndex
        if toIndex != fromIndex {
            onReorder(fromIndex, toIndex)
        }

        draggedTrackId = nil
        dropInsertIndex = nil
        return true
    }

    func dropExited(info: DropInfo) {
        dropInsertIndex = nil
    }

    func validateDrop(info: DropInfo) -> Bool {
        draggedTrackId != nil
    }
}
