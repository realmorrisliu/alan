import Combine
import Foundation
import SwiftUI

#if os(macOS)
import AppKit

@MainActor
final class AlanMacPrimaryShellOwner: ObservableObject {
    let host: ShellHostController
    private let quickTerminalPeakWindow: ShellQuickTerminalPeakPanelWindow
    private let quickTerminalPeakPresenter: ShellQuickTerminalPeakPresenter
    private var quickTerminalPeakStateSubscription: AnyCancellable?

    init(fileManager: FileManager = .default) {
        let windowContext = ShellWindowContext.make(
            fileManager: fileManager,
            windowID: "window_main"
        )
        let resolvedHost = ShellHostController.live(
            fileManager: fileManager,
            windowContext: windowContext,
            startupMode: .workspaceManifest
        )
        let peakWindow = ShellQuickTerminalPeakPanelWindow()
        let peakPresenter = ShellQuickTerminalPeakPresenter(
            host: resolvedHost,
            window: peakWindow
        )
        host = resolvedHost
        quickTerminalPeakWindow = peakWindow
        quickTerminalPeakPresenter = peakPresenter
        quickTerminalPeakStateSubscription = resolvedHost.$shellState.sink { [weak peakPresenter] _ in
            Task { @MainActor in
                peakPresenter?.synchronize()
            }
        }
        quickTerminalPeakPresenter.synchronize()
    }

    deinit {
        let host = host
        Task { @MainActor in
            host.shutdownTerminalRuntimes()
        }
    }
}

@MainActor
private final class ShellQuickTerminalPeakPanelWindow: NSObject, ShellQuickTerminalPeakWindowing, NSWindowDelegate {
    var onDismissRequest: (() -> Void)?
    private var panel: NSPanel?
    private var hostingController: NSHostingController<ShellQuickTerminalPeakView>?

    var isVisible: Bool {
        panel?.isVisible == true
    }

    func presentQuickTerminal(
        host: ShellHostController,
        pane _: ShellPane,
        tab _: ShellTab,
        placement: ShellQuickTerminalPeakPlacement
    ) {
        let panel = ensurePanel()
        if let hostingController {
            hostingController.rootView = ShellQuickTerminalPeakView(host: host)
        } else {
            let hostingController = NSHostingController(rootView: ShellQuickTerminalPeakView(host: host))
            self.hostingController = hostingController
            panel.contentViewController = hostingController
        }

        panel.setFrame(placement.frame, display: true)
        panel.collectionBehavior = collectionBehavior(for: placement)
        orderFrontQuickTerminal(panel)
    }

    func dismissQuickTerminalPeak(reason: ShellQuickTerminalPeakDismissalReason) {
        panel?.orderOut(nil)
        if reason == .removed {
            panel?.contentViewController = nil
            hostingController = nil
        }
    }

    func focusTerminal(paneID: String) {
        guard let panel else {
            return
        }
        orderFrontQuickTerminal(panel)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        onDismissRequest?()
        return false
    }

    func windowDidResignKey(_ notification: Notification) {
        // Intentionally no-op. Focus loss must not hide the quick terminal Peak.
    }

    private func ensurePanel() -> NSPanel {
        if let panel {
            return panel
        }

        let panel = NSPanel(
            contentRect: CGRect(x: 0, y: 0, width: 840, height: 360),
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.title = "Quick Terminal"
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isMovableByWindowBackground = true
        panel.hidesOnDeactivate = false
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.collectionBehavior = collectionBehavior(
            for: ShellQuickTerminalPeakPlacement.defaultPlacement(
                in: ShellQuickTerminalPeakPlacement.activeVisibleFrame()
            )
        )
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.animationBehavior = .utilityWindow
        panel.isReleasedWhenClosed = false
        panel.minSize = CGSize(width: 520, height: 280)
        panel.delegate = self
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        self.panel = panel
        return panel
    }

    private func orderFrontQuickTerminal(_ panel: NSPanel) {
        panel.orderFrontRegardless()
        panel.makeKey()
    }

    private func collectionBehavior(
        for placement: ShellQuickTerminalPeakPlacement
    ) -> NSWindow.CollectionBehavior {
        var behavior: NSWindow.CollectionBehavior = [.fullScreenAuxiliary, .moveToActiveSpace]
        if placement.joinsAllSpaces {
            behavior.insert(.canJoinAllSpaces)
        }
        return behavior
    }
}
#endif
