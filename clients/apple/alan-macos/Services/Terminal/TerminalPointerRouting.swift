#if os(macOS)
import Foundation

enum AlanTerminalLeftMouseDownRoutingDecision: Equatable {
    case ignored
    case deliverToTerminal
    case focusOnly
    case focusAndDeliver
}

@MainActor
enum AlanTerminalPointerPhase: Equatable {
    case entered
    case moved
    case exited
    case buttonDown
    case buttonUp
    case drag
    case pressure
}

enum AlanTerminalPointerButton: Equatable {
    case unknown
    case primary
    case secondary
    case middle
    case four
    case five
    case six
    case seven
    case eight
    case nine
    case ten
    case eleven

    static func fromAppKitButtonNumber(_ buttonNumber: Int) -> AlanTerminalPointerButton {
        switch buttonNumber {
        case 0:
            .primary
        case 1:
            .secondary
        case 2:
            .middle
        case 3:
            .eight
        case 4:
            .nine
        case 5:
            .six
        case 6:
            .seven
        case 7:
            .four
        case 8:
            .five
        case 9:
            .ten
        case 10:
            .eleven
        default:
            .unknown
        }
    }
}

enum AlanTerminalPointerButtonState: Equatable {
    case press
    case release
}

struct AlanTerminalPointerInput: Equatable {
    let phase: AlanTerminalPointerPhase
    let button: AlanTerminalPointerButton?
    let buttonNumber: Int?
    let x: Double
    let y: Double
    let modifiers: AlanTerminalKeyModifiers
    let pressureStage: Int?
    let pressure: Double?

    var normalizedButton: AlanTerminalPointerButton? {
        if let button {
            return button
        }
        guard let buttonNumber else { return nil }
        return AlanTerminalPointerButton.fromAppKitButtonNumber(buttonNumber)
    }
}

enum AlanTerminalPointerOperation: Equatable {
    case position(x: Double, y: Double, modifiers: AlanTerminalKeyModifiers)
    case button(
        state: AlanTerminalPointerButtonState,
        button: AlanTerminalPointerButton,
        x: Double,
        y: Double,
        modifiers: AlanTerminalKeyModifiers
    )
    case pressure(stage: Int, pressure: Double)
}

enum AlanTerminalPointerRoutingDecision: Equatable {
    case terminalMouse(AlanTerminalPointerOperation)
    case terminalSelection(AlanTerminalPointerOperation)
    case terminalHover(AlanTerminalPointerOperation)
    case consumed
    case ignored
}

@MainActor
final class AlanTerminalPointerAdapter {
    func routePointer(
        _ input: AlanTerminalPointerInput,
        terminalMode: AlanTerminalMode,
        surfaceReady: Bool
    ) -> AlanTerminalPointerRoutingDecision {
        guard surfaceReady else { return .ignored }

        switch input.phase {
        case .entered, .moved:
            return routePosition(input, terminalMode: terminalMode)
        case .exited:
            return .terminalHover(.position(x: -1, y: -1, modifiers: input.modifiers))
        case .drag:
            return routeDrag(input, terminalMode: terminalMode)
        case .buttonDown, .buttonUp:
            return routeButton(input, terminalMode: terminalMode)
        case .pressure:
            return .terminalMouse(
                .pressure(stage: input.pressureStage ?? 0, pressure: input.pressure ?? 0)
            )
        }
    }

    private func routePosition(
        _ input: AlanTerminalPointerInput,
        terminalMode: AlanTerminalMode
    ) -> AlanTerminalPointerRoutingDecision {
        let operation = AlanTerminalPointerOperation.position(
            x: input.x,
            y: input.y,
            modifiers: input.modifiers
        )
        switch terminalMode {
        case .normalBuffer:
            return .terminalHover(operation)
        case .alternateScreen, .mouseReporting:
            return .terminalMouse(operation)
        }
    }

    private func routeDrag(
        _ input: AlanTerminalPointerInput,
        terminalMode: AlanTerminalMode
    ) -> AlanTerminalPointerRoutingDecision {
        let operation = AlanTerminalPointerOperation.position(
            x: input.x,
            y: input.y,
            modifiers: input.modifiers
        )
        switch terminalMode {
        case .normalBuffer:
            return .terminalSelection(operation)
        case .alternateScreen, .mouseReporting:
            return .terminalMouse(operation)
        }
    }

    private func routeButton(
        _ input: AlanTerminalPointerInput,
        terminalMode: AlanTerminalMode
    ) -> AlanTerminalPointerRoutingDecision {
        guard let button = input.normalizedButton else { return .ignored }
        let state: AlanTerminalPointerButtonState = input.phase == .buttonDown ? .press : .release
        let operation = AlanTerminalPointerOperation.button(
            state: state,
            button: button,
            x: input.x,
            y: input.y,
            modifiers: input.modifiers
        )
        switch terminalMode {
        case .normalBuffer:
            return .terminalSelection(operation)
        case .alternateScreen, .mouseReporting:
            return .terminalMouse(operation)
        }
    }
}

#endif
