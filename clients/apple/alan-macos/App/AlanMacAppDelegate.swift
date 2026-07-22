#if os(macOS)
import AppKit

final class AlanMacAppDelegate: NSObject, NSApplicationDelegate {
    weak var shellHost: ShellHostController?

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        AlanMacPrimaryWindowPresenter.focusExistingWindow()
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let shellHost else {
            return .terminateNow
        }
        return shellHost.requestTerminateApp() ? .terminateNow : .terminateCancel
    }

    func applicationDidResignActive(_ notification: Notification) {
        // Force pending debounced restore content to disk when the app loses
        // focus, so backgrounding is a durable persistence point.
        shellHost?.persistenceCoordinator.flushWorkspacePersistence()
    }
}
#endif
