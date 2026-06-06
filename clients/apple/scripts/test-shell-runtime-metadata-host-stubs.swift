import AppKit
import Foundation

#if os(macOS)
@MainActor
protocol TerminalHostActivationDelegate: AnyObject {
    func terminalHostDidRequestActivation(paneID: String)
}

@MainActor
final class AlanTerminalHostNSView: NSView {
    private(set) var teardownCount = 0
    private(set) var focusCount = 0
    private(set) var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    private weak var surfaceHandle: AlanTerminalSurfaceHandle?

    func configure(
        pane: ShellPane?,
        terminalContentID: String?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        surfaceHandle: AlanTerminalSurfaceHandle?,
        activationDelegate: TerminalHostActivationDelegate?,
        attachmentPolicy: TerminalHostAttachmentPolicy,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onClearRestoredTranscript: (() -> Void)?,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) {
        self.renderPriority = renderPriority
        surfaceHandle?.updateRenderPriority(renderPriority, forceCatchUp: false)
        self.surfaceHandle = surfaceHandle
    }

    func teardownTerminalRuntime() {
        teardownCount += 1
    }

    var terminalCommandRuntimeState: ShellTerminalCommandRuntimeState {
        let selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine
        let searchEngine = surfaceHandle as? AlanTerminalSearchEngine
        return ShellTerminalCommandRuntimeState(
            paneID: surfaceHandle?.paneID ?? "",
            hasSelection: selectionEngine?.hasSelection() ?? false,
            inputReady: surfaceHandle?.isSurfaceReady ?? false,
            searchAvailable: searchEngine != nil,
            hasReliableSemanticCommands: false
        )
    }

    func copySelection() -> Bool {
        copySelection(to: AlanTerminalSystemPasteboardWriter(pasteboard: .general))
    }

    func copySelection(to writer: AlanTerminalPasteboardWriting) -> Bool {
        guard let selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine,
              let text = selectionEngine.readSelectionText(),
              !text.isEmpty
        else {
            return false
        }
        return writer.writeString(text)
    }

    func pasteText(_ text: String) -> TerminalRuntimeDeliveryResult {
        surfaceHandle?.sendControlText(text)
            ?? .missingTarget(errorMessage: "The stub terminal host has no surface handle.")
    }

    func beginFindInteraction() -> Bool {
        (surfaceHandle as? AlanTerminalSearchEngine)?.startSearch() ?? false
    }

    func beginLastCommandOutputSearch() -> Bool {
        false
    }

    func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        false
    }

    func copyLastCommandOutput() -> Bool {
        false
    }

    func updateFindQuery(_ query: String) -> Bool {
        false
    }

    func selectNextFindMatch() {}

    func selectPreviousFindMatch() {}

    func dismissFindInteraction(refocusTerminal: Bool = true) {}

    func focusTerminal() {
        focusCount += 1
    }
}
#endif
