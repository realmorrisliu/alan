import Combine
import Foundation
import SwiftUI

#if os(macOS)
import AppKit

@MainActor
final class AlanMacPrimaryShellOwner: ObservableObject {
    let host: ShellHostController
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
        let peakPresenter = ShellQuickTerminalPeakPresenter(
            host: resolvedHost,
            window: QuickTerminalPeakWindowPresenter()
        )
        #if canImport(AppIntents)
        ShellAutomationEntityStore.install(snapshotProvider: { [weak resolvedHost] in
            resolvedHost?.shellState
        })
        ShellAutomationIntentStore.install(
            commandHandler: resolvedHost
        )
        #endif
        host = resolvedHost
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
private final class QuickTerminalPeakWindowPresenter: NSObject, ShellQuickTerminalPeakWindowing, NSWindowDelegate {
    var onDismissRequest: (() -> Void)?
    private var panel: QuickTerminalPeakPanel?
    private var hostingController: NSHostingController<ShellQuickTerminalPeakView>?
    private var pendingContentTask: Task<Void, Never>?
    private weak var presentedHost: ShellHostController?
    private var representedPaneID: String?
    private var keyAttemptCount = 0

    var isVisible: Bool {
        panel?.isVisible == true
    }

    func presentQuickTerminal(
        host: ShellHostController,
        pane: ShellPane,
        tab _: ShellTab,
        placement: ShellQuickTerminalPeakPlacement
    ) {
        let panel = ensurePanel()
        pendingContentTask?.cancel()
        presentedHost = host
        representedPaneID = pane.paneID
        keyAttemptCount = 0

        let loadingView = ShellQuickTerminalPeakView(
            host: host,
            paneID: pane.paneID,
            terminalContentEnabled: false
        )
        if let hostingController {
            hostingController.rootView = loadingView
        } else {
            let hostingController = NSHostingController(rootView: loadingView)
            self.hostingController = hostingController
            panel.contentViewController = hostingController
        }

        panel.setFrame(placement.frame, display: true)
        panel.collectionBehavior = placement.windowCollectionBehavior
        panel.orderFrontRegardless()
        requestPanelKeyIfPossible(panel)
        installTerminalContentAfterPanelPresentation(host: host, paneID: pane.paneID)
    }

    func dismissQuickTerminalPeak(reason: ShellQuickTerminalPeakDismissalReason) {
        pendingContentTask?.cancel()
        pendingContentTask = nil
        panel?.orderOut(nil)
        if reason == .removed {
            panel?.contentViewController = nil
            hostingController = nil
            presentedHost = nil
            representedPaneID = nil
        }
    }

    func focusTerminal(paneID: String) {
        guard representedPaneID == paneID else { return }
        if let panel, panel.isVisible {
            requestPanelKeyIfPossible(panel)
        }
        presentedHost?.terminalRuntimeRegistry.requestFocus(for: paneID, retryBudget: 3)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        onDismissRequest?()
        return false
    }

    func windowDidResignKey(_ notification: Notification) {
        // Intentionally no-op. Focus loss must not hide the quick terminal Peak.
    }

    private func ensurePanel() -> QuickTerminalPeakPanel {
        if let panel {
            return panel
        }

        let panel = QuickTerminalPeakPanel(
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
        panel.collectionBehavior = ShellQuickTerminalPeakPlacement.defaultPlacement(
            in: ShellQuickTerminalPeakPlacement.activeVisibleFrame()
        ).windowCollectionBehavior
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

    private func installTerminalContentAfterPanelPresentation(
        host: ShellHostController,
        paneID: String
    ) {
        pendingContentTask = Task { @MainActor [weak self, weak host] in
            await Task.yield()
            guard let self,
                  let host,
                  !Task.isCancelled,
                  representedPaneID == paneID,
                  panel?.isVisible == true
            else {
                return
            }
            hostingController?.rootView = ShellQuickTerminalPeakView(
                host: host,
                paneID: paneID,
                terminalContentEnabled: true
            )
            await Task.yield()
            guard !Task.isCancelled,
                  representedPaneID == paneID,
                  panel?.isVisible == true
            else {
                return
            }
            if let panel {
                requestPanelKeyIfPossible(panel)
            }
            host.terminalRuntimeRegistry.requestFocus(for: paneID, retryBudget: 3)
        }
    }

    private func requestPanelKeyIfPossible(_ panel: NSPanel) {
        guard keyAttemptCount < 2 else { return }
        keyAttemptCount += 1
        panel.makeKey()
    }
}

private final class QuickTerminalPeakPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}
#endif
