#if os(macOS)
import AppKit
import Foundation

@MainActor
protocol ShellCloseConfirmationPresenting: AnyObject {
    func confirmClose(impact: ShellCloseGuardImpact) -> Bool
}

@MainActor
final class ShellCloseWorkflow {
    private let confirmationPresenter: ShellCloseConfirmationPresenting
    private let terminalRuntimeRegistry: TerminalRuntimeRegistry
    private let gracefulShutdownTimeout: TimeInterval
    private let gracefulShutdownPollInterval: TimeInterval = 0.05
    private var autoCloseSuppressedContentIDs: Set<String> = []

    init(
        confirmationPresenter: ShellCloseConfirmationPresenting,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        gracefulShutdownTimeout: TimeInterval
    ) {
        self.confirmationPresenter = confirmationPresenter
        self.terminalRuntimeRegistry = terminalRuntimeRegistry
        self.gracefulShutdownTimeout = gracefulShutdownTimeout
    }

    func confirmAndPerformClose(
        impact: ShellCloseGuardImpact,
        recordDiagnostic: (String) -> Void,
        applyClose: ([String: TerminalTranscriptSnapshot]) -> Bool
    ) -> Bool {
        guard confirmationPresenter.confirmClose(impact: impact) else {
            return false
        }
        return withAutoCloseSuppressed(for: impact.affectedTerminalContentIDs) {
            let gracefullyRequestedContentIDs = requestGracefulShutdown(
                for: impact,
                recordDiagnostic: recordDiagnostic
            )
            waitForGracefulShutdownDrain(
                contentIDs: gracefullyRequestedContentIDs,
                recordDiagnostic: recordDiagnostic
            )
            return applyClose(
                captureTerminalTranscriptSnapshots(
                    for: impact,
                    recordDiagnostic: recordDiagnostic
                )
            )
        }
    }

    func suppressesAutoClose(forTerminalContentID contentID: String) -> Bool {
        autoCloseSuppressedContentIDs.contains(contentID)
    }

    private func captureTerminalTranscriptSnapshots(
        for impact: ShellCloseGuardImpact,
        recordDiagnostic: (String) -> Void
    ) -> [String: TerminalTranscriptSnapshot] {
        impact.affectedTerminalContentIDs.reduce(into: [:]) { capturedByContentID, contentID in
            switch terminalRuntimeRegistry.captureTranscriptSnapshot(
                forTerminalContentID: contentID
            ) {
            case .captured(let transcript):
                capturedByContentID[contentID] = transcript
            case .failed(let failure):
                recordDiagnostic(
                    "terminal transcript capture failed for \(contentID): \(failure.code.rawValue)"
                )
            }
        }
    }

    private func withAutoCloseSuppressed<T>(
        for contentIDs: [String],
        operation: () -> T
    ) -> T {
        guard !contentIDs.isEmpty else { return operation() }
        let previous = autoCloseSuppressedContentIDs
        autoCloseSuppressedContentIDs.formUnion(contentIDs)
        defer {
            autoCloseSuppressedContentIDs = previous
        }
        return operation()
    }

    private func requestGracefulShutdown(
        for impact: ShellCloseGuardImpact,
        recordDiagnostic: (String) -> Void
    ) -> [String] {
        let reason = gracefulShutdownReason(for: impact.scope)
        var requestedContentIDs: [String] = []
        var seenContentIDs: Set<String> = []
        for contentID in impact.activeTerminalContentIDs
            where seenContentIDs.insert(contentID).inserted
        {
            let result = terminalRuntimeRegistry.requestGracefulShutdown(
                forTerminalContentID: contentID,
                reason: reason
            )
            if result.wasRequested {
                requestedContentIDs.append(contentID)
            } else if result.code != .alreadyExited {
                recordDiagnostic(
                    "terminal graceful shutdown request \(result.code.rawValue) for \(contentID)"
                )
            }
        }
        return requestedContentIDs
    }

    private func waitForGracefulShutdownDrain(
        contentIDs: [String],
        recordDiagnostic: (String) -> Void
    ) {
        guard gracefulShutdownTimeout > 0, !contentIDs.isEmpty else { return }
        let deadline = Date().addingTimeInterval(gracefulShutdownTimeout)
        while Date() < deadline {
            if contentIDs.allSatisfy({ terminalGracefulShutdownSettled(contentID: $0) }) {
                return
            }
            let remaining = max(0, deadline.timeIntervalSinceNow)
            _ = RunLoop.current.run(
                mode: .default,
                before: Date().addingTimeInterval(
                    min(gracefulShutdownPollInterval, remaining)
                )
            )
        }

        let timedOutContentIDs = contentIDs.filter {
            !terminalGracefulShutdownSettled(contentID: $0)
        }
        guard !timedOutContentIDs.isEmpty else { return }
        recordDiagnostic(
            "terminal graceful shutdown timed out for \(timedOutContentIDs.joined(separator: ","))"
        )
    }

    private func terminalGracefulShutdownSettled(contentID: String) -> Bool {
        let runtime = terminalRuntimeRegistry.snapshot(forTerminalContentID: contentID)
        let metadata = runtime.paneMetadata
        if metadata.processExited {
            return true
        }
        if let activeTaskState = metadata.activeTaskState {
            return !activeTaskState.protectsFromPruning
        }
        return !terminalRuntimeRegistry.registeredContentIDs.contains(contentID)
    }

    private func gracefulShutdownReason(
        for scope: ShellCloseGuardScope
    ) -> TerminalRuntimeGracefulShutdownReason {
        switch scope {
        case .paneSlot:
            return .paneClose
        case .tab:
            return .tabClose
        case .window:
            return .windowClose
        case .app:
            return .appQuit
        }
    }
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
