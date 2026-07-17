#if os(macOS)
import Foundation

@MainActor
final class AlanTerminalInputRouter {
    private enum PrimaryButtonSequence {
        case idle
        case suppressingFocusTransfer
    }

    private enum ShellActionLookup {
        case action(ShellKeyboardAction)
        case unmapped
        case failed(String)
    }

    private let pointerAdapter = AlanTerminalPointerAdapter()
    private let keyboardActionResolver: (ShellActionShortcut) throws -> ShellKeyboardAction?
    private var primaryButtonSequence = PrimaryButtonSequence.idle

    init(
        keyboardActionResolver: ((ShellActionShortcut) throws -> ShellKeyboardAction?)? = nil
    ) {
        self.keyboardActionResolver = keyboardActionResolver ?? { shortcut in
            try ShellActionCoordinator().keyboardAction(for: shortcut)
        }
    }

    func reset() {
        primaryButtonSequence = .idle
    }

    func routeLeftMouseDown(
        hitOwnsTerminal: Bool,
        commandSurfaceVisible: Bool,
        isFirstResponder: Bool,
        appIsActive: Bool,
        windowIsKey: Bool
    ) -> AlanTerminalLeftMouseDownRoutingDecision {
        primaryButtonSequence = .idle

        guard hitOwnsTerminal, !commandSurfaceVisible else {
            return .ignored
        }

        guard !isFirstResponder else {
            return .deliverToTerminal
        }

        if appIsActive && windowIsKey {
            primaryButtonSequence = .suppressingFocusTransfer
            return .focusOnly
        }

        return .focusAndDeliver
    }

    func routeKeyboard(
        _ input: AlanTerminalKeyInput,
        hasMarkedText: Bool
    ) -> AlanTerminalKeyboardRoutingDecision {
        if input.phase == .down,
           input.modifiers == .command,
           input.characters?.lowercased() == "q"
        {
            return .nativeCommand("quit")
        }

        switch routeShellActionLookup(input) {
        case .action(let action):
            return .shellAction(action.id, action.target)
        case .failed(let reason):
            return .shellActionLookupFailed(reason)
        case .unmapped:
            break
        }

        if input.phase == .down,
           input.modifiers == .command,
           input.characters?.lowercased() == "f"
        {
            return .nativeCommand("find")
        }

        guard input.phase == .down else { return .terminalKey }

        if hasMarkedText {
            return .interpretTextInput
        }

        if shouldInterpretTextInput(input) {
            return .interpretTextInput
        }

        return .terminalKey
    }

    private func shouldInterpretTextInput(_ input: AlanTerminalKeyInput) -> Bool {
        guard input.modifiers.subtracting([.shift]).isEmpty else { return false }
        guard let characters = input.characters, !characters.isEmpty else { return false }
        if characters.count == 1, let scalar = characters.unicodeScalars.first {
            if scalar.value < 0x20 || scalar.value == 0x7F {
                return false
            }
            if scalar.value >= 0xF700 && scalar.value <= 0xF8FF {
                return false
            }
        }
        return true
    }

    func routeShellAction(_ input: AlanTerminalKeyInput) -> ShellKeyboardAction? {
        guard case .action(let action) = routeShellActionLookup(input) else { return nil }
        return action
    }

    private func routeShellActionLookup(_ input: AlanTerminalKeyInput) -> ShellActionLookup {
        guard input.phase == .down, !input.isRepeat else { return .unmapped }
        guard input.modifiers.contains(.command) else { return .unmapped }

        guard let shortcut = shellActionShortcut(for: input) else { return .unmapped }
        guard isShellOwnedKeyboardShortcut(shortcut) else { return .unmapped }

        do {
            let action = try keyboardActionResolver(shortcut)
            return action.map(ShellActionLookup.action) ?? .unmapped
        } catch {
            return .failed("shell-core keyboard action lookup unavailable")
        }
    }

    private func shellActionShortcut(for input: AlanTerminalKeyInput) -> ShellActionShortcut? {
        guard let key = shellActionShortcutKey(for: input) else { return nil }
        return ShellActionShortcut(
            key: key,
            modifiers: input.modifiers.shellActionModifiers,
            context: .shell
        )
    }

    private func shellActionShortcutKey(for input: AlanTerminalKeyInput) -> String? {
        switch input.keyCode {
        case 0x7B:
            return "leftArrow"
        case 0x7C:
            return "rightArrow"
        case 0x7E:
            return "upArrow"
        case 0x7D:
            return "downArrow"
        case 0x31:
            return "space"
        case 0x24, 0x4C:
            return "return"
        default:
            break
        }

        guard let characters = input.characters?.lowercased(), !characters.isEmpty else {
            return nil
        }
        if input.keyCode == 0x18 {
            return "="
        }
        return characters
    }

    private func isShellOwnedKeyboardShortcut(_ shortcut: ShellActionShortcut) -> Bool {
        Self.shellOwnedKeyboardShortcuts.contains(shortcut)
    }

    // Bounds lookup-failure consumption; shell-core remains the action mapping source of truth.
    private static let shellOwnedKeyboardShortcuts: Set<ShellActionShortcut> = {
        var shortcuts: Set<ShellActionShortcut> = [
            shellShortcut("t", [.command]),
            shellShortcut("w", [.command]),
            shellShortcut("[", [.command, .shift]),
            shellShortcut("]", [.command, .shift]),
            shellShortcut("leftArrow", [.command, .option, .shift]),
            shellShortcut("rightArrow", [.command, .option, .shift]),
            shellShortcut("d", [.command, .option]),
            shellShortcut("d", [.command]),
            shellShortcut("d", [.command, .option, .shift]),
            shellShortcut("d", [.command, .shift]),
            shellShortcut("leftArrow", [.command, .control]),
            shellShortcut("rightArrow", [.command, .control]),
            shellShortcut("upArrow", [.command, .control]),
            shellShortcut("downArrow", [.command, .control]),
            shellShortcut("=", [.command, .option]),
            shellShortcut("return", [.command, .shift]),
            shellShortcut("leftArrow", [.command, .control, .shift]),
            shellShortcut("rightArrow", [.command, .control, .shift]),
            shellShortcut("upArrow", [.command, .control, .shift]),
            shellShortcut("downArrow", [.command, .control, .shift]),
            shellShortcut("w", [.command, .shift]),
            shellShortcut("k", [.command]),
            shellShortcut("f", [.command]),
            shellShortcut("leftArrow", [.command, .option]),
            shellShortcut("rightArrow", [.command, .option]),
        ]
        for index in 1...9 {
            shortcuts.insert(shellShortcut(String(index), [.command, .option]))
        }
        return shortcuts
    }()

    private static func shellShortcut(
        _ key: String,
        _ modifiers: Set<ShellActionModifier>
    ) -> ShellActionShortcut {
        ShellActionShortcut(key: key, modifiers: modifiers, context: .shell)
    }

    func routePointer(
        _ input: AlanTerminalPointerInput,
        terminalMode: AlanTerminalMode,
        surfaceReady: Bool
    ) -> AlanTerminalPointerRoutingDecision {
        if shouldConsumePrimaryFocusTransfer(input) {
            if input.phase == .buttonUp {
                primaryButtonSequence = .idle
            }
            return .consumed
        }

        return pointerAdapter.routePointer(
            input,
            terminalMode: terminalMode,
            surfaceReady: surfaceReady
        )
    }

    private func shouldConsumePrimaryFocusTransfer(_ input: AlanTerminalPointerInput) -> Bool {
        guard primaryButtonSequence == .suppressingFocusTransfer else { return false }
        guard input.normalizedButton == .primary else { return false }
        return input.phase == .buttonDown || input.phase == .drag || input.phase == .buttonUp
    }
}

#endif
