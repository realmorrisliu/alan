#if os(macOS)
import Foundation

enum AlanTerminalClearIntent {
    static func isControlL(_ input: AlanTerminalKeyInput) -> Bool {
        guard input.phase == .down, !input.isRepeat else { return false }
        guard input.modifiers == [.control] else { return false }
        guard let characters = input.characters, !characters.isEmpty else { return false }
        return characters == "\u{0c}" || characters.lowercased() == "l"
    }
}

struct AlanTerminalClearCommandTracker {
    private var currentLine = ""

    mutating func observeCommittedText(_ text: String?) -> Bool {
        guard let text, !text.isEmpty else { return false }

        var detected = false
        for scalar in text.unicodeScalars {
            switch scalar.value {
            case 0x0A, 0x0D:
                detected = detected || currentLine.trimmingCharacters(in: .whitespaces) == "clear"
                currentLine = ""
            case 0x08, 0x7F:
                if !currentLine.isEmpty {
                    currentLine.removeLast()
                }
            case 0x00..<0x20:
                currentLine = ""
            default:
                currentLine.append(String(scalar))
            }
        }
        return detected
    }

    mutating func reset() {
        currentLine = ""
    }
}

struct AlanTerminalKeyModifiers: OptionSet, Equatable {
    let rawValue: Int

    static let shift = AlanTerminalKeyModifiers(rawValue: 1 << 0)
    static let control = AlanTerminalKeyModifiers(rawValue: 1 << 1)
    static let option = AlanTerminalKeyModifiers(rawValue: 1 << 2)
    static let command = AlanTerminalKeyModifiers(rawValue: 1 << 3)

    var shellActionModifiers: Set<ShellActionModifier> {
        var modifiers = Set<ShellActionModifier>()
        if contains(.shift) { modifiers.insert(.shift) }
        if contains(.control) { modifiers.insert(.control) }
        if contains(.option) { modifiers.insert(.option) }
        if contains(.command) { modifiers.insert(.command) }
        return modifiers
    }
}

enum AlanTerminalKeyPhase: Equatable {
    case down
    case up
    case flagsChanged
}

struct AlanTerminalKeyInput: Equatable {
    let characters: String?
    let keyCode: UInt16
    let modifiers: AlanTerminalKeyModifiers
    let phase: AlanTerminalKeyPhase
    let isRepeat: Bool
}

enum AlanTerminalKeyboardRoutingDecision: Equatable {
    case nativeCommand(String)
    case shellAction(ShellActionID, ShellActionTarget)
    case shellActionLookupFailed(String)
    case terminalKey
    case interpretTextInput
    case drop
}

enum AlanTerminalTextCompositionPolicy {
    static func shouldSuppressComposingControlInput(
        _ text: String?,
        composing: Bool
    ) -> Bool {
        guard composing, let text else { return false }
        let scalars = text.unicodeScalars
        guard let scalar = scalars.first,
              scalars.index(after: scalars.startIndex) == scalars.endIndex else {
            return false
        }
        return scalar.value < 0x20 || scalar.value == 0x7F
    }
}

struct AlanTerminalKeyEquivalentInput: Equatable {
    let characters: String?
    let charactersIgnoringModifiers: String?
    let modifiers: AlanTerminalKeyModifiers
    let keyCode: UInt16
    let timestamp: TimeInterval
    let isRepeat: Bool
}

enum AlanTerminalKeyEquivalentRoutingDecision: Equatable {
    case sendOriginal
    case sendEquivalent(String)
    case deferToResponder
}

@MainActor
final class AlanTerminalKeyEquivalentAdapter {
    private var lastPerformKeyEvent: TimeInterval?

    func routeKeyEquivalent(
        _ input: AlanTerminalKeyEquivalentInput,
        isFocused: Bool,
        isTerminalBinding: Bool
    ) -> AlanTerminalKeyEquivalentRoutingDecision {
        guard isFocused else {
            clearPendingRedispatch()
            return .deferToResponder
        }

        if isTerminalBinding {
            clearPendingRedispatch()
            return .sendOriginal
        }

        let equivalent: String
        switch input.charactersIgnoringModifiers {
        case "\r":
            guard input.modifiers.contains(.control) else { return .deferToResponder }
            equivalent = "\r"
        case "/":
            guard input.modifiers.contains(.control),
                  input.modifiers.isDisjoint(with: [.shift, .option, .command])
            else {
                return .deferToResponder
            }
            equivalent = "_"
        default:
            guard input.timestamp != 0 else { return .deferToResponder }

            guard input.modifiers.contains(.command) || input.modifiers.contains(.control) else {
                clearPendingRedispatch()
                return .deferToResponder
            }

            if let lastPerformKeyEvent {
                self.lastPerformKeyEvent = nil
                if lastPerformKeyEvent == input.timestamp {
                    return .sendEquivalent(input.characters ?? "")
                }
            }

            lastPerformKeyEvent = input.timestamp
            return .deferToResponder
        }

        clearPendingRedispatch()
        return .sendEquivalent(equivalent)
    }

    func shouldRedispatchDoCommand(currentEventTimestamp: TimeInterval) -> Bool {
        guard currentEventTimestamp != 0 else { return false }
        return lastPerformKeyEvent == currentEventTimestamp
    }

    func clearPendingRedispatch() {
        lastPerformKeyEvent = nil
    }
}

#endif
