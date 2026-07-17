#if os(macOS)
import AppKit
#if canImport(GhosttyKit)
import GhosttyKit
#endif

extension AlanTerminalHostNSView: NSTextInputClient {
    func insertText(_ string: Any, replacementRange: NSRange) {
        let characters: String
        switch string {
        case let value as NSAttributedString:
            characters = value.string
        case let value as String:
            characters = value
        default:
            return
        }

        let wasComposing = markedText.length > 0
        unmarkText()
        guard !AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
            characters,
            composing: wasComposing
        ) else { return }

        if var keyTextAccumulator {
            keyTextAccumulator.append(characters)
            self.keyTextAccumulator = keyTextAccumulator
        } else {
#if canImport(GhosttyKit)
            surfaceController.sendProgrammaticText(characters)
#endif
        }
    }

    override func insertText(_ insertString: Any) {
        insertText(insertString, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    override func doCommand(by selector: Selector) {
#if canImport(GhosttyKit)
        if let current = NSApp.currentEvent,
           keyEquivalentAdapter.shouldRedispatchDoCommand(currentEventTimestamp: current.timestamp)
        {
            NSApp.sendEvent(current)
        }
#endif
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        switch string {
        case let value as NSAttributedString:
            markedText = NSMutableAttributedString(attributedString: value)
        case let value as String:
            markedText = NSMutableAttributedString(string: value)
        default:
            return
        }

        if keyTextAccumulator == nil {
            syncPreedit()
        }
    }

    func unmarkText() {
        guard markedText.length > 0 else { return }
        markedText.mutableString.setString("")
        syncPreedit()
    }

    func selectedRange() -> NSRange {
#if canImport(GhosttyKit)
        if let selection = surfaceController.readSelectionText() {
            return NSRange(location: 0, length: selection.utf16.count)
        }
#endif
        return NSRange(location: NSNotFound, length: 0)
    }

    func markedRange() -> NSRange {
        markedText.length > 0
            ? NSRange(location: 0, length: markedText.length)
            : NSRange(location: NSNotFound, length: 0)
    }

    func hasMarkedText() -> Bool {
        markedText.length > 0
    }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
#if canImport(GhosttyKit)
        guard let selection = surfaceController.readSelectionText(), !selection.isEmpty else { return nil }
        actualRange?.pointee = NSRange(location: 0, length: selection.utf16.count)
        return NSAttributedString(string: selection)
#else
        return nil
#endif
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
#if canImport(GhosttyKit)
        if let imeRect = surfaceController.imeRect(in: self) {
            return imeRect
        }
#endif
        guard let window else { return frame }
        return window.convertToScreen(convert(bounds, to: nil))
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }

    @discardableResult
    func beginFindInteraction() -> Bool {
        guard surfaceController.beginSearch() else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func beginLastCommandOutputSearch() -> Bool {
        guard surfaceController.beginLastCommandOutputSearch() else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard surfaceController.navigateSemanticPrompt(direction) else { return false }
        syncNativeScrollback()
        publishRuntimeSnapshot()
        return true
    }

    func copyLastCommandOutput() -> Bool {
        surfaceController.copyLastCommandOutput(to: .general)
    }

    @discardableResult
    func updateFindQuery(_ query: String) -> Bool {
        guard surfaceController.updateSearchQuery(query) else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func selectNextFindMatch() {
        surfaceController.nextSearchMatch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
    }

    func selectPreviousFindMatch() {
        surfaceController.previousSearchMatch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
    }

    func dismissFindInteraction(refocusTerminal: Bool) {
        surfaceController.dismissSearch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        if refocusTerminal {
            requestTerminalFocus()
        }
    }
}
#endif
