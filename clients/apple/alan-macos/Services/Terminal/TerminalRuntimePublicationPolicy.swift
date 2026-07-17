#if os(macOS)
enum TerminalRuntimePublicationPolicy {
    static func shouldProjectToShell(
        previous: TerminalHostRuntimeSnapshot?,
        next: TerminalHostRuntimeSnapshot
    ) -> Bool {
        guard let previous else {
            return true
        }

        return shellProjectionChanged(previous: previous, next: next)
    }

    private static func shellProjectionChanged(
        previous: TerminalHostRuntimeSnapshot,
        next: TerminalHostRuntimeSnapshot
    ) -> Bool {
        previous.contentID != next.contentID
            || previous.paneID != next.paneID
            || previous.tabID != next.tabID
            || previous.stage != next.stage
            || previous.renderPriority != next.renderPriority
            || previous.displayName != next.displayName
            || previous.displayID != next.displayID
            || previous.attachedWindowTitle != next.attachedWindowTitle
            || previous.renderer.failureReason != next.renderer.failureReason
            || (previous.renderer.phase != .failed && next.renderer.phase == .failed)
            || previous.paneMetadata.title != next.paneMetadata.title
            || previous.paneMetadata.workingDirectory != next.paneMetadata.workingDirectory
            || previous.paneMetadata.summary != next.paneMetadata.summary
            || previous.paneMetadata.attention != next.paneMetadata.attention
            || previous.paneMetadata.processExited != next.paneMetadata.processExited
            || previous.paneMetadata.lastCommandExitCode != next.paneMetadata.lastCommandExitCode
            || previous.paneMetadata.activeTaskState != next.paneMetadata.activeTaskState
            || previous.paneMetadata.activity != next.paneMetadata.activity
            || previous.paneMetadata.clearsActivity != next.paneMetadata.clearsActivity
            || previous.surfaceState.readiness != next.surfaceState.readiness
            || previous.surfaceState.rendererHealth != next.surfaceState.rendererHealth
            || previous.surfaceState.readonly != next.surfaceState.readonly
            || previous.surfaceState.terminalMode != next.surfaceState.terminalMode
            || previous.surfaceState.inputReady != next.surfaceState.inputReady
            || previous.surfaceState.childExited != next.surfaceState.childExited
    }
}

#endif
