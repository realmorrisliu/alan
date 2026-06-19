#if os(macOS)
import AppKit

enum AlanMacPrimaryWindowPresenter {
    private static func primaryWindow() -> NSWindow? {
        NSApp.windows.first(where: { $0.title == "alan" }) ?? NSApp.windows.first
    }

    static func focusExistingWindow() {
        guard let window = primaryWindow() else { return }
        present(window)
    }

    static func summonExistingWindow(refocusTerminal: (() -> Void)? = nil) {
        guard let window = primaryWindow() else {
            refocusTerminal?()
            return
        }
        window.collectionBehavior.insert(.moveToActiveSpace)
        present(window)
        DispatchQueue.main.async {
            refocusTerminal?()
        }
    }

    private static func present(_ window: NSWindow) {
        NSApp.unhide(nil)
        window.deminiaturize(nil)
        window.orderFrontRegardless()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    static func focusExistingWindowSoon() {
        DispatchQueue.main.async {
            focusExistingWindow()
        }
    }

    static func summonExistingWindowSoon(refocusTerminal: (() -> Void)? = nil) {
        DispatchQueue.main.async {
            summonExistingWindow(refocusTerminal: refocusTerminal)
        }
    }
}
#endif
