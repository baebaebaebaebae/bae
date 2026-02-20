import SwiftUI
import UniformTypeIdentifiers

struct QueueItemViewModel: Identifiable {
    let id: String
    let title: String
    let artistNames: String
    let albumTitle: String
    let durationMs: Int64?
    let coverArtURL: URL?
}

struct QueueView: View {
    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingArtURL: URL?
    let items: [QueueItemViewModel]
    let onClose: () -> Void
    let onClear: () -> Void
    let onSkipTo: (Int) -> Void
    let onRemove: (Int) -> Void
    let onReorder: (Int, Int) -> Void
    @State private var hoveredIndex: Int?
    @State private var draggedTrackId: String?
    @State private var dropInsertIndex: Int?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()

            if isActive {
                nowPlayingSection
                Divider()
            }

            if items.isEmpty {
                ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: Text("Play an album to fill the queue")
                )
                .frame(maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                            VStack(spacing: 0) {
                                // Insertion line above this row
                                if dropInsertIndex == index {
                                    insertionLine
                                }
                                queueItemRow(item, index: index)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 4)
                                    .opacity(draggedTrackId == item.id ? 0.3 : 1.0)
                                Divider().padding(.leading, 62)
                            }
                            .onDrop(of: [UTType.plainText], delegate: QueueDropDelegate(
                                targetIndex: index,
                                items: items,
                                draggedTrackId: $draggedTrackId,
                                dropInsertIndex: $dropInsertIndex,
                                onReorder: onReorder
                            ))
                        }
                        // Insertion line at the very end
                        if dropInsertIndex == items.count {
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
            Button("Clear") { onClear() }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(items.isEmpty)
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
                Text(nowPlayingTitle ?? "")
                    .font(.callout)
                    .fontWeight(.medium)
                    .lineLimit(1)
                Text(nowPlayingArtist ?? "")
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
        if let url = nowPlayingArtURL {
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

    private func queueItemRow(_ item: QueueItemViewModel, index: Int) -> some View {
        HStack(spacing: 10) {
            ZStack {
                queueItemArt(url: item.coverArtURL)
                    .frame(width: 40, height: 40)
                    .clipShape(RoundedRectangle(cornerRadius: 3))

                if hoveredIndex == index {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(.black.opacity(0.5))
                        .frame(width: 40, height: 40)
                    Button(action: { onSkipTo(index) }) {
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
                Button(action: { onRemove(index) }) {
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
            draggedTrackId = item.id
            return NSItemProvider(object: item.id as NSString)
        }
        .onTapGesture(count: 2) {
            onSkipTo(index)
        }
        .contextMenu {
            Button("Remove from Queue") {
                onRemove(index)
            }
        }
    }

    @ViewBuilder
    private func queueItemArt(url: URL?) -> some View {
        if let url {
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
    let items: [QueueItemViewModel]
    @Binding var draggedTrackId: String?
    @Binding var dropInsertIndex: Int?
    let onReorder: (Int, Int) -> Void

    func dropEntered(info: DropInfo) {
        guard let draggedId = draggedTrackId,
              let fromIndex = items.firstIndex(where: { $0.id == draggedId }) else { return }

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
              let fromIndex = items.firstIndex(where: { $0.id == draggedId }) else { return false }

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

// MARK: - Previews

#Preview("With items") {
    QueueView(
        isActive: true,
        nowPlayingTitle: PreviewData.nowPlayingTitle,
        nowPlayingArtist: PreviewData.nowPlayingArtist,
        nowPlayingArtURL: nil,
        items: PreviewData.queueItems,
        onClose: {},
        onClear: {},
        onSkipTo: { _ in },
        onRemove: { _ in },
        onReorder: { _, _ in }
    )
    .frame(width: 350, height: 500)
}

#Preview("Empty") {
    QueueView(
        isActive: false,
        nowPlayingTitle: nil,
        nowPlayingArtist: nil,
        nowPlayingArtURL: nil,
        items: [],
        onClose: {},
        onClear: {},
        onSkipTo: { _ in },
        onRemove: { _ in },
        onReorder: { _, _ in }
    )
    .frame(width: 350, height: 400)
}
