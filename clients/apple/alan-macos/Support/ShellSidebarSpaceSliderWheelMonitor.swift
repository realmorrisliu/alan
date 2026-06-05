import SwiftUI

#if os(macOS)
import AppKit

final class ShellSidebarSpaceSliderWheelPhaseLessResetScheduler {
    static let resetDelay: TimeInterval = 0.14

    private let onReset: () -> Void
    private let scheduleWorkItem: (DispatchWorkItem, TimeInterval) -> Void
    private var workItem: DispatchWorkItem?

    init(
        onReset: @escaping () -> Void,
        scheduleWorkItem: @escaping (DispatchWorkItem, TimeInterval) -> Void = { workItem, delay in
            DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: workItem)
        }
    ) {
        self.onReset = onReset
        self.scheduleWorkItem = scheduleWorkItem
    }

    func scheduleResetAfterIdle() {
        cancel()
        var nextWorkItem: DispatchWorkItem!
        nextWorkItem = DispatchWorkItem { [weak self] in
            guard let self, !nextWorkItem.isCancelled else { return }
            self.workItem = nil
            self.onReset()
        }
        workItem = nextWorkItem
        scheduleWorkItem(nextWorkItem, Self.resetDelay)
    }

    func resetNow() {
        cancel()
        onReset()
    }

    func cancel() {
        workItem?.cancel()
        workItem = nil
    }
}

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
        private lazy var phaseLessResetScheduler =
            ShellSidebarSpaceSliderWheelPhaseLessResetScheduler { [weak self] in
                self?.onReset()
            }

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
            phaseLessResetScheduler.cancel()
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
                phaseLessResetScheduler.resetNow()
            }

            let consumed = onScroll(scrollDeltaX(from: event), scrollDeltaY(from: event))

            if event.phase.contains(.ended)
                || event.phase.contains(.cancelled)
                || event.momentumPhase.contains(.ended)
                || event.momentumPhase.contains(.cancelled)
            {
                phaseLessResetScheduler.resetNow()
            } else if event.phase.isEmpty, event.momentumPhase.isEmpty {
                phaseLessResetScheduler.scheduleResetAfterIdle()
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
