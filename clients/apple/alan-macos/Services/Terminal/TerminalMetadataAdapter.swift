#if os(macOS)
import Foundation

@MainActor
final class AlanTerminalMetadataAdapter {
    func overlayState(
        renderer: TerminalRendererSnapshot,
        metadata: TerminalPaneMetadataSnapshot,
        surface: AlanTerminalSurfaceReadiness
    ) -> AlanTerminalOverlayState? {
        if metadata.processExited {
            let status = metadata.lastCommandExitCode.map { "Exit status \($0)." }
                ?? "The shell process ended."
            return AlanTerminalOverlayState(
                title: "Process exited",
                message: status,
                badge: "Exited",
                action: "Open a new pane or tab to continue.",
                debugDetail: metadata.summary
            )
        }

        if renderer.phase == .failed || surface == .unready(reason: .rendererFailed) {
            return AlanTerminalOverlayState(
                title: "Terminal cannot draw",
                message: "The terminal renderer is not available for this pane.",
                badge: "Renderer failed",
                action: "Close and reopen the pane if it does not recover.",
                debugDetail: renderer.failureReason ?? renderer.detail ?? renderer.summary
            )
        }

        switch surface {
        case .ready:
            return nil
        case .unready(reason: .missingSurface):
            return AlanTerminalOverlayState(
                title: "Terminal surface missing",
                message: "This pane does not currently have a terminal surface.",
                badge: "Missing",
                action: "Select the pane again or open a new terminal.",
                debugDetail: nil
            )
        case .unready(reason: .inputNotReady):
            return AlanTerminalOverlayState(
                title: "Terminal is starting",
                message: "Input will be available after the terminal finishes attaching.",
                badge: "Starting",
                action: nil,
                debugDetail: renderer.detail
            )
        case .unready(reason: .rendererFailed):
            return AlanTerminalOverlayState(
                title: "Terminal cannot draw",
                message: "The terminal renderer is not available for this pane.",
                badge: "Renderer failed",
                action: "Close and reopen the pane if it does not recover.",
                debugDetail: renderer.failureReason ?? renderer.detail ?? renderer.summary
            )
        case .unready(reason: .childExited):
            return AlanTerminalOverlayState(
                title: "Process exited",
                message: "The shell process ended.",
                badge: "Exited",
                action: "Open a new pane or tab to continue.",
                debugDetail: metadata.summary
            )
        case .unready(reason: .readonly):
            return AlanTerminalOverlayState(
                title: "Terminal is read-only",
                message: "This pane is not accepting input right now.",
                badge: "Read-only",
                action: nil,
                debugDetail: nil
            )
        }
    }
}

#endif
