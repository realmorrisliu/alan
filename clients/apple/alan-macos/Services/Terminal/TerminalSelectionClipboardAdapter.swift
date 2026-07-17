#if os(macOS)
import AppKit
import Foundation

@MainActor
protocol AlanTerminalSelectionEngine: AnyObject {
    func readSelectionText() -> String?
    func hasSelection() -> Bool
}

@MainActor
protocol AlanTerminalPasteboardWriting: AnyObject {
    func writeString(_ text: String) -> Bool
}

@MainActor
final class AlanTerminalSystemPasteboardWriter: AlanTerminalPasteboardWriting {
    private let pasteboard: NSPasteboard

    init(pasteboard: NSPasteboard = .general) {
        self.pasteboard = pasteboard
    }

    func writeString(_ text: String) -> Bool {
        pasteboard.clearContents()
        pasteboard.declareTypes([.string], owner: nil)
        return pasteboard.setString(text, forType: .string)
    }
}

@MainActor
final class AlanTerminalSelectionClipboardAdapter {
    private weak var surfaceHandle: AlanTerminalSurfaceHandle?

    init(surfaceHandle: AlanTerminalSurfaceHandle?) {
        self.surfaceHandle = surfaceHandle
    }

    func updateSurfaceHandle(_ surfaceHandle: AlanTerminalSurfaceHandle?) {
        self.surfaceHandle = surfaceHandle
    }

    func paste(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return .accepted(byteCount: 0)
        }
        guard let surfaceHandle,
              surfaceHandle.isSurfaceReady,
              surfaceHandle.snapshot.teardownStatus != .completed
        else {
            return .rejected(
                errorCode: "terminal_clipboard_unavailable",
                errorMessage: "Paste cannot be delivered because the terminal is not ready.",
                runtimePhase: surfaceHandle?.snapshot.runtimePhase
            )
        }
        return surfaceHandle.sendControlText(text)
    }

    func writeSelection(_ text: String?, to writer: AlanTerminalPasteboardWriting) -> Bool {
        guard let text, !text.isEmpty else { return false }
        return writer.writeString(text)
    }

    func writeSelectionToPasteboard(_ text: String?, pasteboard: NSPasteboard = .general) -> Bool {
        writeSelection(text, to: AlanTerminalSystemPasteboardWriter(pasteboard: pasteboard))
    }
}

#endif
