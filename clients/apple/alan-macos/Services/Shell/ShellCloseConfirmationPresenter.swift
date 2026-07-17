#if os(macOS)
import AppKit

@MainActor
protocol ShellCloseConfirmationPresenting: AnyObject {
    func confirmClose(impact: ShellCloseGuardImpact) -> Bool
}

@MainActor
final class ShellNSAlertCloseConfirmationPresenter: ShellCloseConfirmationPresenting {
    func confirmClose(impact: ShellCloseGuardImpact) -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = closeTitle(for: impact.scope)
        alert.informativeText = closeMessage(for: impact)
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func closeTitle(for scope: ShellCloseGuardScope) -> String {
        switch scope {
        case .paneSlot:
            return "Close pane?"
        case .tab:
            return "Close tab?"
        case .window:
            return "Close window?"
        case .app:
            return "Quit alan?"
        }
    }

    private func closeMessage(for impact: ShellCloseGuardImpact) -> String {
        let count = impact.activeTerminalContentIDs.count
        let noun = count == 1 ? "terminal has" : "terminals have"
        return "\(count) \(noun) active work. Closing will stop the running process "
            + "and save only restorable terminal history."
    }
}
#endif
