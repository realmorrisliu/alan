import Foundation

#if os(macOS)
final class TerminalHostRuntimeReporter {
    private var lastReportedRuntime: TerminalHostRuntimeSnapshot?

    func publish(
        _ snapshot: TerminalHostRuntimeSnapshot,
        observer: @escaping (TerminalHostRuntimeSnapshot) -> Void
    ) {
        if let lastReportedRuntime,
           Self.snapshotsEqualIgnoringTimestamp(lastReportedRuntime, snapshot)
        {
            return
        }

        lastReportedRuntime = snapshot
        DispatchQueue.main.async { [weak self] in
            guard let self,
                  let lastReportedRuntime = self.lastReportedRuntime,
                  Self.snapshotsEqualIgnoringTimestamp(lastReportedRuntime, snapshot)
            else {
                return
            }
            observer(snapshot)
        }
    }

    private static func snapshotsEqualIgnoringTimestamp(
        _ lhs: TerminalHostRuntimeSnapshot,
        _ rhs: TerminalHostRuntimeSnapshot
    ) -> Bool {
        lhs.stage == rhs.stage
            && lhs.paneID == rhs.paneID
            && lhs.tabID == rhs.tabID
            && lhs.renderPriority == rhs.renderPriority
            && lhs.logicalSize == rhs.logicalSize
            && lhs.backingSize == rhs.backingSize
            && lhs.displayName == rhs.displayName
            && lhs.displayID == rhs.displayID
            && lhs.attachedWindowTitle == rhs.attachedWindowTitle
            && lhs.isFocused == rhs.isFocused
            && lhs.renderer == rhs.renderer
            && lhs.paneMetadata == rhs.paneMetadata
            && lhs.surfaceState.equalsIgnoringTimestamp(rhs.surfaceState)
    }
}
#endif
