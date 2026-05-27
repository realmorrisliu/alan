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
}
#endif
