import SwiftUI

#if os(macOS)
import AppKit

struct ShellSidebarSpaceSliderWheelMonitor: NSViewRepresentable {
    let onScroll: (CGFloat, CGFloat) -> Bool
    let onReset: () -> Void
    let onContextMenuIntent: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(
            onScroll: onScroll,
            onReset: onReset,
            onContextMenuIntent: onContextMenuIntent
        )
    }

    func makeNSView(context: Context) -> MonitorView {
        let view = MonitorView()
        view.coordinator = context.coordinator
        context.coordinator.install(for: view)
        return view
    }

    func updateNSView(_ nsView: MonitorView, context: Context) {
        context.coordinator.onScroll = onScroll
        context.coordinator.onReset = onReset
        context.coordinator.onContextMenuIntent = onContextMenuIntent
        nsView.coordinator = context.coordinator
        context.coordinator.install(for: nsView)
    }

    final class MonitorView: NSView {
        var coordinator: Coordinator?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            coordinator?.install(for: self)
        }

        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }

        deinit {
            coordinator?.uninstall()
        }
    }

    final class Coordinator {
        var onScroll: (CGFloat, CGFloat) -> Bool
        var onReset: () -> Void
        var onContextMenuIntent: () -> Void
        private weak var view: NSView?
        private var monitor: Any?

        init(
            onScroll: @escaping (CGFloat, CGFloat) -> Bool,
            onReset: @escaping () -> Void,
            onContextMenuIntent: @escaping () -> Void
        ) {
            self.onScroll = onScroll
            self.onReset = onReset
            self.onContextMenuIntent = onContextMenuIntent
        }

        func install(for view: NSView) {
            self.view = view
            guard monitor == nil else { return }

            monitor = NSEvent.addLocalMonitorForEvents(
                matching: [.scrollWheel, .rightMouseDown, .leftMouseDown]
            ) { [weak self] event in
                self?.handle(event) ?? event
            }
        }

        func uninstall() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
        }

        private func handle(_ event: NSEvent) -> NSEvent? {
            guard let view,
                  view.window === event.window
            else {
                return event
            }

            let point = view.convert(event.locationInWindow, from: nil)
            guard view.bounds.contains(point) else {
                return event
            }

            if event.type == .rightMouseDown
                || (event.type == .leftMouseDown && event.modifierFlags.contains(.control))
            {
                onContextMenuIntent()
                return event
            }

            guard event.type == .scrollWheel else {
                return event
            }

            if event.phase.contains(.began) || event.momentumPhase.contains(.began) {
                onReset()
            }

            let consumed = onScroll(scrollDeltaX(from: event), scrollDeltaY(from: event))

            if event.phase.contains(.ended)
                || event.phase.contains(.cancelled)
                || event.momentumPhase.contains(.ended)
                || event.momentumPhase.contains(.cancelled)
            {
                onReset()
            }

            return consumed ? nil : event
        }

        private func scrollDeltaX(from event: NSEvent) -> CGFloat {
            if event.hasPreciseScrollingDeltas {
                return event.scrollingDeltaX
            }
            return event.deltaX * 10
        }

        private func scrollDeltaY(from event: NSEvent) -> CGFloat {
            if event.hasPreciseScrollingDeltas {
                return event.scrollingDeltaY
            }
            return event.deltaY * 10
        }
    }
}
#endif
