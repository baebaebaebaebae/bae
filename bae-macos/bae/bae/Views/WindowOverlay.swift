import SwiftUI
import AppKit

/// Renders SwiftUI content as a full-window overlay, regardless of where in the view
/// hierarchy this modifier is used. Works by adding an NSHostingView directly to the
/// window's content view, bypassing SwiftUI's `.overlay` frame constraints.
///
/// The `isPresented` flag controls when the overlay hosting view is installed. When false,
/// no hosting view exists and all events pass through normally.
struct WindowOverlay<OverlayContent: View>: ViewModifier {
    let isPresented: Bool
    @ViewBuilder var overlayContent: () -> OverlayContent

    func body(content: Content) -> some View {
        content
            .background(
                WindowOverlayBridge(isPresented: isPresented, overlayContent: overlayContent)
            )
    }
}

private struct WindowOverlayBridge<OverlayContent: View>: NSViewRepresentable {
    let isPresented: Bool
    @ViewBuilder var overlayContent: () -> OverlayContent

    func makeNSView(context: Context) -> NSView {
        NSView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        let coordinator = context.coordinator

        if isPresented {
            // Defer so the anchor view is in the window
            DispatchQueue.main.async {
                guard let window = nsView.window,
                      let windowContentView = window.contentView else { return }

                let hostingView: NSHostingView<AnyView>
                if let existing = coordinator.hostingView {
                    hostingView = existing
                } else {
                    hostingView = NSHostingView(rootView: AnyView(EmptyView()))
                    hostingView.translatesAutoresizingMaskIntoConstraints = false
                    windowContentView.addSubview(hostingView)

                    NSLayoutConstraint.activate([
                        hostingView.leadingAnchor.constraint(equalTo: windowContentView.leadingAnchor),
                        hostingView.trailingAnchor.constraint(equalTo: windowContentView.trailingAnchor),
                        hostingView.topAnchor.constraint(equalTo: windowContentView.topAnchor),
                        hostingView.bottomAnchor.constraint(equalTo: windowContentView.bottomAnchor),
                    ])

                    coordinator.hostingView = hostingView
                }

                hostingView.rootView = AnyView(overlayContent())
            }
        } else {
            coordinator.hostingView?.removeFromSuperview()
            coordinator.hostingView = nil
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.hostingView?.removeFromSuperview()
        coordinator.hostingView = nil
    }

    class Coordinator {
        var hostingView: NSHostingView<AnyView>?
    }
}

extension View {
    /// Present content as a full-window overlay, escaping the bounds of the current view.
    /// The overlay is only installed when `isPresented` is true.
    func windowOverlay<Content: View>(
        isPresented: Bool,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        modifier(WindowOverlay(isPresented: isPresented, overlayContent: content))
    }
}
