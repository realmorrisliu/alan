#if os(macOS)
import AppKit
#if canImport(GhosttyKit)
import GhosttyKit
#endif

extension AlanTerminalHostNSView {
    func focusTerminalSoon() {
        guard isSelected, pane != nil else { return }
        guard window != nil else {
            needsWindowAttachmentFocus = true
            return
        }
        guard !pendingFocusRequest else { return }
        pendingFocusRequest = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            pendingFocusRequest = false
            guard isSelected, pane != nil, window != nil else { return }
            requestTerminalFocus()
        }
    }

    func requestTerminalFocus() {
        guard !terminalInputIsActive else { return }
        if window?.makeFirstResponder(self) == true {
            return
        }
        synchronizeLiveHost()
        publishRuntimeSnapshot()
    }

    func focusTerminal() {
        focusTerminalSoon()
    }

    func activateTerminalHostForMouseEvent() {
        if let paneID = pane?.paneID {
            activationDelegate?.terminalHostDidRequestActivation(paneID: paneID)
        }
        requestTerminalFocus()
    }

    func localPoint(for event: NSEvent) -> CGPoint {
        convert(event.locationInWindow, from: nil)
    }

    private func ghosttyPoint(for event: NSEvent) -> CGPoint {
        let point = localPoint(for: event)
        return CGPoint(x: point.x, y: bounds.height - point.y)
    }

    func terminalPointerInput(
        for event: NSEvent,
        phase: AlanTerminalPointerPhase,
        button: AlanTerminalPointerButton? = nil
    ) -> AlanTerminalPointerInput {
        let point = ghosttyPoint(for: event)
        return AlanTerminalPointerInput(
            phase: phase,
            button: button,
            buttonNumber: event.buttonNumber,
            x: point.x,
            y: point.y,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            pressureStage: nil,
            pressure: nil
        )
    }

    private func terminalPointerPressureInput(for event: NSEvent) -> AlanTerminalPointerInput {
        AlanTerminalPointerInput(
            phase: .pressure,
            button: nil,
            buttonNumber: nil,
            x: 0,
            y: 0,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            pressureStage: event.stage,
            pressure: Double(event.pressure)
        )
    }

    @discardableResult
    func routePointer(_ input: AlanTerminalPointerInput) -> Bool {
        let decision = pointerDecision(for: input)
        let handled = executePointerDecision(decision)
        tracePointerInput("pointer-route", input: input, decision: decision, handled: handled)
        return handled
    }

    private func pointerDecision(
        for input: AlanTerminalPointerInput
    ) -> AlanTerminalPointerRoutingDecision {
#if canImport(GhosttyKit)
        return surfaceController.routePointer(input)
#else
        return .ignored
#endif
    }

    @discardableResult
    private func executePointerDecision(_ decision: AlanTerminalPointerRoutingDecision) -> Bool {
#if canImport(GhosttyKit)
        return deliverPointerDecision(decision)
#else
        return false
#endif
    }

    override func mouseDown(with event: NSEvent) {
        traceTerminalInput("raw-leftMouseDown", event: event)
        activateTerminalHostForMouseEvent()
        routePointer(terminalPointerInput(for: event, phase: .buttonDown, button: .primary))
    }

    override func mouseUp(with event: NSEvent) {
        let buttonUpDecision = pointerDecision(
            for: terminalPointerInput(for: event, phase: .buttonUp, button: .primary)
        )
        traceTerminalInput(
            "raw-leftMouseUp",
            event: event,
            details: "decision=\(buttonUpDecision)"
        )
        if buttonUpDecision == .consumed {
            return
        }
        previousPressureStage = 0
        executePointerDecision(buttonUpDecision)
        routePointer(
            AlanTerminalPointerInput(
                phase: .pressure,
                button: nil,
                buttonNumber: nil,
                x: 0,
                y: 0,
                modifiers: terminalKeyModifiers(from: event.modifierFlags),
                pressureStage: 0,
                pressure: 0
            )
        )
    }

    override func rightMouseDown(with event: NSEvent) {
        activateTerminalHostForMouseEvent()
        let consumed = routePointer(
            terminalPointerInput(for: event, phase: .buttonDown, button: .secondary)
        )
        if !consumed {
            super.rightMouseDown(with: event)
        }
    }

    override func rightMouseUp(with event: NSEvent) {
        let consumed = routePointer(
            terminalPointerInput(for: event, phase: .buttonUp, button: .secondary)
        )
        if !consumed {
            super.rightMouseUp(with: event)
        }
    }

    override func otherMouseDown(with event: NSEvent) {
        activateTerminalHostForMouseEvent()
        let consumed = routePointer(
            terminalPointerInput(
                for: event,
                phase: .buttonDown,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
        if !consumed {
            super.otherMouseDown(with: event)
        }
    }

    override func otherMouseUp(with event: NSEvent) {
        let consumed = routePointer(
            terminalPointerInput(
                for: event,
                phase: .buttonUp,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
        if !consumed {
            super.otherMouseUp(with: event)
        }
    }

    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        routePointer(terminalPointerInput(for: event, phase: .entered))
    }

    override func mouseMoved(with event: NSEvent) {
        routePointer(terminalPointerInput(for: event, phase: .moved))
    }

    override func mouseDragged(with event: NSEvent) {
        traceTerminalInput("raw-leftMouseDragged", event: event)
        routePointer(terminalPointerInput(for: event, phase: .drag, button: .primary))
    }

    override func rightMouseDragged(with event: NSEvent) {
        routePointer(terminalPointerInput(for: event, phase: .drag, button: .secondary))
    }

    override func otherMouseDragged(with event: NSEvent) {
        routePointer(
            terminalPointerInput(
                for: event,
                phase: .drag,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        routePointer(terminalPointerInput(for: event, phase: .exited))
    }

    @discardableResult
    func routeWrappedMouseEvent(_ routedEvent: AlanTerminalRoutedMouseEvent, _ event: NSEvent) -> Bool {
        traceTerminalInput(
            "wrapped-\(routedEvent)",
            event: event,
            details: "source=nativeScrollView"
        )
        switch routedEvent {
        case .mouseDown:
            mouseDown(with: event)
        case .mouseUp:
            mouseUp(with: event)
        case .rightMouseDown:
            rightMouseDown(with: event)
        case .rightMouseUp:
            rightMouseUp(with: event)
        case .otherMouseDown:
            otherMouseDown(with: event)
        case .otherMouseUp:
            otherMouseUp(with: event)
        case .mouseEntered:
            mouseEntered(with: event)
        case .mouseMoved:
            mouseMoved(with: event)
        case .mouseDragged:
            mouseDragged(with: event)
        case .rightMouseDragged:
            rightMouseDragged(with: event)
        case .otherMouseDragged:
            otherMouseDragged(with: event)
        case .mouseExited:
            mouseExited(with: event)
        case .pressureChange:
            pressureChange(with: event)
        }
        return true
    }

    override func scrollWheel(with event: NSEvent) {
        if routeScrollWheel(event) {
            return
        }
        super.scrollWheel(with: event)
    }

    func routeScrollWheel(_ event: NSEvent) -> Bool {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return false }

        let scrollRoute = surfaceController.routeScroll(
            AlanTerminalScrollInput(
                deltaX: event.scrollingDeltaX,
                deltaY: event.scrollingDeltaY,
                precise: event.hasPreciseScrollingDeltas
            )
        )
        switch scrollRoute {
        case .nativeScroll:
            syncNativeScrollback()
            publishRuntimeSnapshot()
            return true
        case .ignored:
            return true
        case .terminalScroll:
            break
        }

        var x = event.scrollingDeltaX
        var y = event.scrollingDeltaY
        let precision = event.hasPreciseScrollingDeltas
        if precision {
            x *= 2
            y *= 2
        }

        var scrollMods: Int32 = 0
        if precision {
            scrollMods |= 0b0000_0001
        }

        let momentum: Int32
        switch event.momentumPhase {
        case .began:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_BEGAN.rawValue)
        case .stationary:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_STATIONARY.rawValue)
        case .changed:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_CHANGED.rawValue)
        case .ended:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_ENDED.rawValue)
        case .cancelled:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_CANCELLED.rawValue)
        case .mayBegin:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_MAY_BEGIN.rawValue)
        default:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_NONE.rawValue)
        }
        scrollMods |= momentum << 1

        surfaceController.sendMouseScroll(x: x, y: y, mods: ghostty_input_scroll_mods_t(scrollMods))
        return true
#else
        return false
#endif
    }

    override func pressureChange(with event: NSEvent) {
        super.pressureChange(with: event)
        guard routePointer(terminalPointerPressureInput(for: event)) else { return }
        previousPressureStage = event.stage
    }

#if canImport(GhosttyKit)
    @discardableResult
    private func deliverPointerDecision(_ decision: AlanTerminalPointerRoutingDecision) -> Bool {
        switch decision {
        case .terminalMouse(let operation),
             .terminalSelection(let operation),
             .terminalHover(let operation):
            return deliverPointerOperation(operation)
        case .consumed:
            return true
        case .ignored:
            return false
        }
    }

    @discardableResult
    private func deliverPointerOperation(_ operation: AlanTerminalPointerOperation) -> Bool {
        switch operation {
        case .position(let x, let y, let modifiers):
            surfaceController.sendMousePosition(
                x: x,
                y: y,
                mods: ghosttyMods(from: modifiers)
            )
            return true
        case .button(let state, let button, let x, let y, let modifiers):
            let mods = ghosttyMods(from: modifiers)
            surfaceController.sendMousePosition(x: x, y: y, mods: mods)
            return surfaceController.sendMouseButton(
                state: ghosttyMouseState(from: state),
                button: ghosttyMouseButton(from: button),
                mods: mods
            )
        case .pressure(let stage, let pressure):
            surfaceController.sendMousePressure(stage: UInt32(max(stage, 0)), pressure: pressure)
            return true
        }
    }

    private func ghosttyMouseState(
        from state: AlanTerminalPointerButtonState
    ) -> ghostty_input_mouse_state_e {
        switch state {
        case .press:
            GHOSTTY_MOUSE_PRESS
        case .release:
            GHOSTTY_MOUSE_RELEASE
        }
    }

    private func ghosttyMouseButton(
        from button: AlanTerminalPointerButton
    ) -> ghostty_input_mouse_button_e {
        switch button {
        case .unknown:
            GHOSTTY_MOUSE_UNKNOWN
        case .primary:
            GHOSTTY_MOUSE_LEFT
        case .secondary:
            GHOSTTY_MOUSE_RIGHT
        case .middle:
            GHOSTTY_MOUSE_MIDDLE
        case .four:
            GHOSTTY_MOUSE_FOUR
        case .five:
            GHOSTTY_MOUSE_FIVE
        case .six:
            GHOSTTY_MOUSE_SIX
        case .seven:
            GHOSTTY_MOUSE_SEVEN
        case .eight:
            GHOSTTY_MOUSE_EIGHT
        case .nine:
            GHOSTTY_MOUSE_NINE
        case .ten:
            GHOSTTY_MOUSE_TEN
        case .eleven:
            GHOSTTY_MOUSE_ELEVEN
        }
    }

    private func ghosttyMods(from modifiers: AlanTerminalKeyModifiers) -> ghostty_input_mods_e {
        var mods = GHOSTTY_MODS_NONE.rawValue
        if modifiers.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if modifiers.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if modifiers.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
        if modifiers.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
        return ghostty_input_mods_e(rawValue: mods)
    }
#endif
}
#endif
