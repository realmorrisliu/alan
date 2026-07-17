#if os(macOS)
import AppKit
import Carbon
#if canImport(GhosttyKit)
import GhosttyKit
#endif

extension AlanTerminalHostNSView {
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if routeShellActionKeyIfNeeded(event) {
            return true
        }
        if routeNativeKeyCommandIfNeeded(event) {
            return true
        }

#if canImport(GhosttyKit)
        guard !isApplicationReservedKeyEquivalent(event) else { return false }
        guard event.type == .keyDown,
              terminalInputIsActive,
              surfaceController.isSurfaceReady == true else { return false }

        var keyEvent = ghosttyKeyEvent(for: event, action: GHOSTTY_ACTION_PRESS)
        var flags = ghostty_binding_flags_e(0)
        let text = event.characters ?? ""
        let isBinding = text.withCString { cString in
            keyEvent.text = cString
            return surfaceController.keyIsBinding(keyEvent, flags: &flags)
        }

        switch keyEquivalentAdapter.routeKeyEquivalent(
            terminalKeyEquivalentInput(for: event),
            isFocused: terminalInputIsActive,
            isTerminalBinding: isBinding
        ) {
        case .sendOriginal:
            keyDown(with: event)
            return true
        case .sendEquivalent(let equivalent):
            keyDown(with: terminalKeyEquivalentEvent(equivalent: equivalent, basedOn: event) ?? event)
            return true
        case .deferToResponder:
            return false
        }
#else
        return false
#endif
    }

    override func keyDown(with event: NSEvent) {
        let traceStartedAt = terminalInputTraceStart()
        traceTerminalInput(
            "raw-keyDown-begin",
            event: event,
            details: "keyCode=\(event.keyCode) chars_len=\((event.characters ?? "").count) surfaceReady=\(surfaceController.isSurfaceReady == true)"
        )
        defer {
            traceTerminalInputDuration(
                "raw-keyDown-end",
                event: event,
                startedAt: traceStartedAt,
                details: "keyCode=\(event.keyCode) surfaceReady=\(surfaceController.isSurfaceReady == true)"
            )
        }

        if routeShellActionKeyIfNeeded(event) {
            return
        }
        if routeNativeKeyCommandIfNeeded(event) {
            return
        }

#if canImport(GhosttyKit)
        if isApplicationReservedKeyEquivalent(event) {
            NSApp.terminate(nil)
            return
        }

        guard surfaceController.isSurfaceReady == true else {
            interpretKeyEvents([event])
            return
        }

        requestTerminalFocus()
        keyEquivalentAdapter.clearPendingRedispatch()
        let keyInput = terminalKeyInput(for: event)
        let keyboardDecision = surfaceController.routeKeyboard(
            keyInput,
            hasMarkedText: markedText.length > 0
        )
        if AlanTerminalClearIntent.isControlL(keyInput) {
            observeTerminalClearIntent()
        }

        switch keyboardDecision {
        case .shellAction(let actionID, let target):
            if actionID == .findOpen {
                _ = routeFindCommandToShellHost(target: target)
            } else {
                shellActionHandler?(actionID, target)
            }
            return
        case .nativeCommand("find"):
            _ = routeFindCommandToShellHost()
            return
        case .nativeCommand("quit"):
            NSApp.terminate(nil)
            return
        case .shellActionLookupFailed:
            return
        case .nativeCommand, .drop:
            return
        case .interpretTextInput, .terminalKey:
            break
        }

        let translationModsGhostty =
            surfaceController.keyTranslationMods(for: modsFromEvent(event))
        var translationMods = event.modifierFlags
        for flag in [NSEvent.ModifierFlags.shift, .control, .option, .command] {
            let shouldInclude: Bool
            switch flag {
            case .shift:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_SHIFT.rawValue) != 0
            case .control:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_CTRL.rawValue) != 0
            case .option:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_ALT.rawValue) != 0
            case .command:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_SUPER.rawValue) != 0
            default:
                shouldInclude = translationMods.contains(flag)
            }

            if shouldInclude {
                translationMods.insert(flag)
            } else {
                translationMods.remove(flag)
            }
        }

        let translationEvent: NSEvent
        if translationMods == event.modifierFlags {
            translationEvent = event
        } else {
            translationEvent = NSEvent.keyEvent(
                with: event.type,
                location: event.locationInWindow,
                modifierFlags: translationMods,
                timestamp: event.timestamp,
                windowNumber: event.windowNumber,
                context: nil,
                characters: event.characters(byApplyingModifiers: translationMods) ?? "",
                charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                isARepeat: event.isARepeat,
                keyCode: event.keyCode
            ) ?? event
        }

        keyTextAccumulator = []
        defer { keyTextAccumulator = nil }

        let markedTextBefore = markedText.length > 0
        let shouldInterpretText = keyboardDecision == .interpretTextInput
        let keyboardIDBefore = !markedTextBefore && shouldInterpretText
            ? AlanKeyboardLayout.currentID
            : nil
        if shouldInterpretText {
            interpretKeyEvents([translationEvent])
            if let keyboardIDBefore, keyboardIDBefore != AlanKeyboardLayout.currentID {
                return
            }
            syncPreedit(clearIfNeeded: markedTextBefore)
        }
        let composing = markedText.length > 0 || markedTextBefore
        let action = event.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS

        if shouldInterpretText,
           let keyTextAccumulator,
           !keyTextAccumulator.isEmpty
        {
            for text in keyTextAccumulator {
                guard !AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
                    text,
                    composing: composing
                ) else {
                    continue
                }

                observeCommittedTerminalText(text)
                if markedTextBefore {
                    sendCommittedPreeditText(text, action: action)
                } else {
                    sendGhosttyKeyEvent(
                        for: event,
                        action: action,
                        translationMods: translationMods,
                        textOverride: text,
                        composing: false
                    )
                }
            }

            if markedTextBefore, shouldReplayCommittedPreeditKey(translationEvent) {
                sendGhosttyKeyEvent(
                    for: event,
                    action: action,
                    translationMods: translationMods,
                    composing: false
                )
            }
            return
        }

        if AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
            event.characters,
            composing: composing
        ) {
            return
        }

        if !composing {
            observeCommittedTerminalText(translationEvent.characters ?? event.characters)
        }
        sendGhosttyKeyEvent(
            for: event,
            action: action,
            translationMods: translationMods,
            textEvent: translationEvent,
            composing: composing
        )
#else
        super.keyDown(with: event)
#endif
    }

    private func observeTerminalClearIntent() {
        clearCommandTracker.reset()
        clearRestoredTranscriptHandler?()
    }

    private func observeCommittedTerminalText(_ text: String?) {
        if clearCommandTracker.observeCommittedText(text) {
            clearRestoredTranscriptHandler?()
        }
    }

    override func keyUp(with event: NSEvent) {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return super.keyUp(with: event) }
        let traceStartedAt = terminalInputTraceStart()
        let keyEvent = ghosttyKeyEvent(for: event, action: GHOSTTY_ACTION_RELEASE)
        let handled = surfaceController.sendKey(keyEvent)
        traceTerminalInputDuration(
            "keyUp-sendKey-end",
            event: event,
            startedAt: traceStartedAt,
            details: "keyCode=\(event.keyCode) handled=\(handled)"
        )
#else
        super.keyUp(with: event)
#endif
    }

    override func flagsChanged(with event: NSEvent) {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return super.flagsChanged(with: event) }
        guard !hasMarkedText() else { return }

        let modifier: UInt32
        switch event.keyCode {
        case 0x39: modifier = GHOSTTY_MODS_CAPS.rawValue
        case 0x38, 0x3C: modifier = GHOSTTY_MODS_SHIFT.rawValue
        case 0x3B, 0x3E: modifier = GHOSTTY_MODS_CTRL.rawValue
        case 0x3A, 0x3D: modifier = GHOSTTY_MODS_ALT.rawValue
        case 0x37, 0x36: modifier = GHOSTTY_MODS_SUPER.rawValue
        default: return
        }

        let mods = modsFromEvent(event)
        var action = GHOSTTY_ACTION_RELEASE
        if mods.rawValue & modifier != 0 {
            let sidePressed: Bool
            switch event.keyCode {
            case 0x3C:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERSHIFTKEYMASK) != 0
            case 0x3E:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCTLKEYMASK) != 0
            case 0x3D:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERALTKEYMASK) != 0
            case 0x36:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCMDKEYMASK) != 0
            default:
                sidePressed = true
            }
            if sidePressed {
                action = GHOSTTY_ACTION_PRESS
            }
        }

        let keyEvent = ghosttyKeyEvent(for: event, action: action)
        _ = surfaceController.sendKey(keyEvent)
        routePointer(terminalPointerInput(for: event, phase: .moved))
#else
        super.flagsChanged(with: event)
#endif
    }

    @objc func copy(_ sender: Any?) {
        _ = copySelection()
    }

    @objc func cut(_ sender: Any?) {
        copy(sender)
    }

    @objc func paste(_ sender: Any?) {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        _ = pasteText(text)
    }

    var terminalCommandRuntimeState: ShellTerminalCommandRuntimeState {
        ShellTerminalCommandRuntimeState(
            paneID: pane?.paneID ?? "",
            hasSelection: surfaceController.hasSelection(),
            inputReady: surfaceController.surfaceStateSnapshot.inputReady,
            searchAvailable: pane?.paneID != nil,
            hasReliableSemanticCommands: surfaceController.hasReliableSemanticCommandActions
        )
    }

    @discardableResult
    func copySelection() -> Bool {
        surfaceController.copySelection(to: .general)
    }

    @discardableResult
    func copySelection(to writer: AlanTerminalPasteboardWriting) -> Bool {
        surfaceController.copySelection(to: writer)
    }

    @discardableResult
    func pasteText(_ text: String) -> TerminalRuntimeDeliveryResult {
        let result = surfaceController.paste(text)
        publishRuntimeSnapshot()
        return result
    }

    private func modsFromEvent(_ event: NSEvent) -> ghostty_input_mods_e {
        ghosttyMods(from: event.modifierFlags)
    }

    private func consumedModsFromFlags(_ flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        ghosttyMods(from: flags)
    }

    private func ghosttyMods(from flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        var mods = GHOSTTY_MODS_NONE.rawValue
        if flags.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
        if flags.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
        if flags.contains(.capsLock) { mods |= GHOSTTY_MODS_CAPS.rawValue }

        let rawFlags = flags.rawValue
        if rawFlags & UInt(NX_DEVICERSHIFTKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_SHIFT_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERCTLKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_CTRL_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERALTKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_ALT_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERCMDKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_SUPER_RIGHT.rawValue
        }

        return ghostty_input_mods_e(rawValue: mods)
    }

    private func ghosttyKeyEvent(
        for event: NSEvent,
        action: ghostty_input_action_e,
        translationMods: NSEvent.ModifierFlags? = nil
    ) -> ghostty_input_key_s {
        var keyEvent = ghostty_input_key_s()
        keyEvent.action = action
        keyEvent.keycode = UInt32(event.keyCode)
        keyEvent.mods = modsFromEvent(event)
        keyEvent.consumed_mods = consumedModsFromFlags(
            (translationMods ?? event.modifierFlags).subtracting([.control, .command])
        )
        keyEvent.text = nil
        keyEvent.composing = false
        keyEvent.unshifted_codepoint = unshiftedCodepointFromEvent(event)
        return keyEvent
    }

    @discardableResult
    private func sendGhosttyKeyEvent(
        for event: NSEvent,
        action: ghostty_input_action_e,
        translationMods: NSEvent.ModifierFlags,
        textEvent: NSEvent? = nil,
        textOverride: String? = nil,
        composing: Bool
    ) -> Bool {
        let traceStartedAt = terminalInputTraceStart()
        var keyEvent = ghosttyKeyEvent(
            for: event,
            action: action,
            translationMods: translationMods
        )
        keyEvent.composing = composing

        let text = textOverride ?? textEvent.flatMap { textForKeyEvent($0) }
        let handled: Bool
        if let text, shouldSendText(text) {
            handled = text.withCString { cString in
                keyEvent.text = cString
                return surfaceController.sendKey(keyEvent)
            }
        } else {
            handled = surfaceController.sendKey(keyEvent)
        }

        traceTerminalInputDuration(
            "sendGhosttyKeyEvent-end",
            event: event,
            startedAt: traceStartedAt,
            details: "action=\(action.rawValue) text_len=\((text ?? "").count) composing=\(composing) handled=\(handled)"
        )
        return handled
    }

    private func sendCommittedPreeditText(
        _ text: String,
        action: ghostty_input_action_e
    ) {
        var keyEvent = ghostty_input_key_s()
        keyEvent.action = action
        keyEvent.keycode = 0
        keyEvent.mods = GHOSTTY_MODS_NONE
        keyEvent.consumed_mods = GHOSTTY_MODS_NONE
        keyEvent.text = nil
        keyEvent.composing = false
        keyEvent.unshifted_codepoint = 0

        text.withCString { cString in
            keyEvent.text = cString
            _ = surfaceController.sendKey(keyEvent)
        }
    }

    private func shouldReplayCommittedPreeditKey(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 0x7C, 0x7D, 0x7E:
            return true
        case 0x7B:
            return !event.modifierFlags.isDisjoint(with: [.shift, .control, .option, .command])
        default:
            return false
        }
    }

    private func unshiftedCodepointFromEvent(_ event: NSEvent) -> UInt32 {
        guard event.type != .flagsChanged else {
            return 0
        }
        guard let chars = event.characters(byApplyingModifiers: []) ?? event.charactersIgnoringModifiers ?? event.characters,
              let scalar = chars.unicodeScalars.first else {
            return 0
        }
        return scalar.value
    }

    private func textForKeyEvent(_ event: NSEvent) -> String? {
        guard let chars = event.characters, !chars.isEmpty else { return nil }

        if chars.count == 1, let scalar = chars.unicodeScalars.first {
            if isControlCharacter(scalar) {
                return event.characters(byApplyingModifiers: event.modifierFlags.subtracting(.control))
            }

            if scalar.value >= 0xF700 && scalar.value <= 0xF8FF {
                return nil
            }
        }

        return chars
    }

    private func isControlCharacter(_ scalar: UnicodeScalar) -> Bool {
        scalar.value < 0x20 || scalar.value == 0x7F
    }

    private func shouldSendText(_ text: String) -> Bool {
        guard !text.isEmpty else { return false }
        if text.count == 1, let scalar = text.unicodeScalars.first {
            return !isControlCharacter(scalar)
        }
        return true
    }

    private func routeNativeKeyCommandIfNeeded(_ event: NSEvent) -> Bool {
        switch surfaceController.routeKeyboard(
            terminalKeyInput(for: event),
            hasMarkedText: markedText.length > 0
        ) {
        case .nativeCommand("find"):
            return routeFindCommandToShellHost()
        case .nativeCommand("quit"):
            return false
        case .nativeCommand,
             .shellAction,
             .shellActionLookupFailed,
             .terminalKey,
             .interpretTextInput,
             .drop:
            return false
        }
    }

    private func routeShellActionKeyIfNeeded(_ event: NSEvent) -> Bool {
        let decision = surfaceController.routeKeyboard(
            terminalKeyInput(for: event),
            hasMarkedText: markedText.length > 0
        )
        guard case .shellAction(let actionID, let target) = decision else {
            return false
        }
        if actionID == .findOpen {
            return routeFindCommandToShellHost(target: target)
        }
        shellActionHandler?(actionID, target)
        return true
    }

    private func routeFindCommandToShellHost(target: ShellActionTarget = .currentSelection) -> Bool {
        guard let shellActionHandler else {
            return beginFindInteraction()
        }
        let resolvedTarget: ShellActionTarget
        if case .currentSelection = target,
           let paneID = pane?.paneID
        {
            resolvedTarget = .contextPane(paneID)
        } else {
            resolvedTarget = target
        }
        shellActionHandler(.findOpen, resolvedTarget)
        return true
    }

    private func terminalKeyInput(for event: NSEvent) -> AlanTerminalKeyInput {
        let phase: AlanTerminalKeyPhase
        switch event.type {
        case .keyDown:
            phase = .down
        case .keyUp:
            phase = .up
        case .flagsChanged:
            phase = .flagsChanged
        default:
            phase = .down
        }
        return AlanTerminalKeyInput(
            characters: event.charactersIgnoringModifiers ?? event.characters,
            keyCode: event.keyCode,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            phase: phase,
            isRepeat: event.isARepeat
        )
    }

    func terminalKeyModifiers(from flags: NSEvent.ModifierFlags) -> AlanTerminalKeyModifiers {
        var modifiers: AlanTerminalKeyModifiers = []
        if flags.contains(.shift) { modifiers.insert(.shift) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        return modifiers
    }

    private func isApplicationReservedKeyEquivalent(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return false }

        let flags = event.modifierFlags
            .intersection(.deviceIndependentFlagsMask)
            .subtracting([.capsLock, .numericPad, .function])
        guard flags == .command else { return false }

        return event.charactersIgnoringModifiers?.lowercased() == "q"
    }

    private func terminalKeyEquivalentInput(for event: NSEvent) -> AlanTerminalKeyEquivalentInput {
        AlanTerminalKeyEquivalentInput(
            characters: event.characters,
            charactersIgnoringModifiers: event.charactersIgnoringModifiers,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            keyCode: event.keyCode,
            timestamp: event.timestamp,
            isRepeat: event.isARepeat
        )
    }

    private func terminalKeyEquivalentEvent(
        equivalent: String,
        basedOn event: NSEvent
    ) -> NSEvent? {
        NSEvent.keyEvent(
            with: .keyDown,
            location: event.locationInWindow,
            modifierFlags: event.modifierFlags,
            timestamp: event.timestamp,
            windowNumber: event.windowNumber,
            context: nil,
            characters: equivalent,
            charactersIgnoringModifiers: equivalent,
            isARepeat: event.isARepeat,
            keyCode: event.keyCode
        )
    }

    func syncPreedit(clearIfNeeded: Bool = true) {
#if canImport(GhosttyKit)
        if markedText.length > 0 {
            surfaceController.sendPreedit(markedText.string)
        } else if clearIfNeeded {
            surfaceController.sendPreedit(nil)
        }
#endif
    }

}
#endif
