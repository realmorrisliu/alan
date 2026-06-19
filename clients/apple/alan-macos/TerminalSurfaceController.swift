import Foundation

#if os(macOS)
import AppKit
#if canImport(GhosttyKit)
import GhosttyKit
#endif

enum AlanTerminalMode: String, Equatable {
    case normalBuffer = "normal_buffer"
    case alternateScreen = "alternate_screen"
    case mouseReporting = "mouse_reporting"
}

final class AlanTerminalModeTracker {
    private var hasSeenScrollableNormalBuffer = false

    func reset() {
        hasSeenScrollableNormalBuffer = false
    }

    func resolveMode(totalRows: Int, visibleRows: Int, mouseCaptured: Bool) -> AlanTerminalMode {
        if mouseCaptured {
            return .mouseReporting
        }

        let hasScrollableRows = max(0, totalRows) > max(0, visibleRows)
        if hasScrollableRows {
            hasSeenScrollableNormalBuffer = true
            return .normalBuffer
        }

        if hasSeenScrollableNormalBuffer {
            return .alternateScreen
        }

        return .normalBuffer
    }
}

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

struct AlanTerminalScrollbackMetrics: Equatable {
    let totalRows: Int
    let visibleRows: Int
    let firstVisibleRow: Int
    let mode: AlanTerminalMode

    static let empty = AlanTerminalScrollbackMetrics(
        totalRows: 0,
        visibleRows: 0,
        firstVisibleRow: 0,
        mode: .normalBuffer
    )
}

struct AlanTerminalScrollbackState: Equatable {
    let metrics: AlanTerminalScrollbackMetrics
    let nativeScrollbarVisible: Bool
    let thumbRange: Range<Int>

    static let empty = AlanTerminalScrollbackState(
        metrics: .empty,
        nativeScrollbarVisible: false,
        thumbRange: 0..<0
    )
}

struct AlanTerminalScrollInput: Equatable {
    let deltaX: Double
    let deltaY: Double
    let precise: Bool
}

enum AlanTerminalScrollRoutingDecision: Equatable {
    case nativeScroll(row: Int)
    case terminalScroll
    case ignored
}

@MainActor
protocol AlanTerminalScrollbackEngine: AnyObject {
    func setScrollbackUpdateHandler(_ handler: ((AlanTerminalScrollbackMetrics) -> Void)?)
    func scrollTo(row: Int) -> Bool
}

struct AlanTerminalBufferRange: Equatable, Hashable {
    let lowerBound: Int
    let upperBound: Int

    init(_ range: Range<Int>) {
        lowerBound = max(0, range.lowerBound)
        upperBound = max(lowerBound, range.upperBound)
    }

    init(lowerBound: Int, upperBound: Int) {
        self.lowerBound = max(0, lowerBound)
        self.upperBound = max(self.lowerBound, upperBound)
    }

    var isEmpty: Bool {
        lowerBound >= upperBound
    }
}

enum AlanTerminalCommandBoundaryState: Equatable {
    case reliable
    case unavailable(reason: String)
    case stale(reason: String)

    var isReliable: Bool {
        self == .reliable
    }
}

struct AlanTerminalCommandSegment: Equatable, Identifiable {
    let id: String
    let promptRange: AlanTerminalBufferRange?
    let commandRange: AlanTerminalBufferRange?
    let outputRange: AlanTerminalBufferRange?
    let commandText: String?
    let workingDirectory: String?
    let exitStatus: Int?
    let startedAt: Date?
    let endedAt: Date?
    let boundaryState: AlanTerminalCommandBoundaryState

    var hasReliablePrompt: Bool {
        boundaryState.isReliable && promptRange != nil
    }

    var hasReliableOutput: Bool {
        boundaryState.isReliable && outputRange != nil
    }
}

struct AlanTerminalSemanticCommandState: Equatable {
    let paneID: String?
    let boundaryState: AlanTerminalCommandBoundaryState
    let segments: [AlanTerminalCommandSegment]
    let lastUpdatedAt: Date?

    static func unavailable(paneID: String?, reason: String) -> AlanTerminalSemanticCommandState {
        AlanTerminalSemanticCommandState(
            paneID: paneID,
            boundaryState: .unavailable(reason: reason),
            segments: [],
            lastUpdatedAt: .now
        )
    }

    static let placeholder = AlanTerminalSemanticCommandState.unavailable(
        paneID: nil,
        reason: "No terminal pane is attached."
    )

    var reliableSegments: [AlanTerminalCommandSegment] {
        guard boundaryState.isReliable else { return [] }
        return segments.filter { $0.boundaryState.isReliable }
    }

    var hasReliableCommandBoundaries: Bool {
        reliableSegments.contains { $0.promptRange != nil || $0.outputRange != nil }
    }

    var hasReliablePromptMarks: Bool {
        reliableSegments.contains { $0.hasReliablePrompt }
    }

    var lastReliableOutputRange: AlanTerminalBufferRange? {
        reliableSegments.last(where: { $0.hasReliableOutput })?.outputRange
    }
}

@MainActor
protocol AlanTerminalCommandBufferEngine: AnyObject {
    func readText(in range: AlanTerminalBufferRange) -> String?
}

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
final class AlanTerminalScrollbackAdapter {
    private var preciseScrollRemainder = 0.0
    private(set) var state = AlanTerminalScrollbackState.empty

    @discardableResult
    func reset() -> AlanTerminalScrollbackState {
        state = .empty
        preciseScrollRemainder = 0
        return state
    }

    @discardableResult
    func updateMetrics(_ metrics: AlanTerminalScrollbackMetrics) -> AlanTerminalScrollbackState {
        let totalRows = max(0, metrics.totalRows)
        let visibleRows = max(0, min(metrics.visibleRows, totalRows))
        let firstVisibleRow = max(0, min(metrics.firstVisibleRow, max(totalRows - visibleRows, 0)))
        let hasScrollableNormalBuffer = metrics.mode == .normalBuffer && totalRows > visibleRows
        let nextMetrics = AlanTerminalScrollbackMetrics(
            totalRows: totalRows,
            visibleRows: visibleRows,
            firstVisibleRow: firstVisibleRow,
            mode: metrics.mode
        )
        state = AlanTerminalScrollbackState(
            metrics: nextMetrics,
            nativeScrollbarVisible: hasScrollableNormalBuffer,
            thumbRange: firstVisibleRow..<(firstVisibleRow + visibleRows)
        )
        if !hasScrollableNormalBuffer {
            preciseScrollRemainder = 0
        }
        return state
    }

    @discardableResult
    func scrollTo(firstVisibleRow: Int) -> AlanTerminalScrollbackState {
        updateMetrics(
            AlanTerminalScrollbackMetrics(
                totalRows: state.metrics.totalRows,
                visibleRows: state.metrics.visibleRows,
                firstVisibleRow: firstVisibleRow,
                mode: state.metrics.mode
            )
        )
    }

    func shouldConsumeNativeScrollInput(_ input: AlanTerminalScrollInput) -> Bool {
        guard state.nativeScrollbarVisible else { return false }
        guard abs(input.deltaY) >= abs(input.deltaX) else { return false }
        return input.deltaY != 0 || preciseScrollRemainder != 0
    }

    func resetPreciseScrollAccumulator() {
        preciseScrollRemainder = 0
    }

    func targetFirstVisibleRow(for input: AlanTerminalScrollInput, rowHeight: CGFloat = 1) -> Int? {
        guard shouldConsumeNativeScrollInput(input) else { return nil }
        let rowDelta: Int
        if input.precise {
            let rows = (-input.deltaY / max(Double(rowHeight), 1)) + preciseScrollRemainder
            rowDelta = Int(rows.rounded(.towardZero))
            preciseScrollRemainder = rows - Double(rowDelta)
        } else {
            preciseScrollRemainder = 0
            rowDelta = Int((-input.deltaY).rounded(.toNearestOrAwayFromZero))
        }
        guard rowDelta != 0 else { return nil }
        let maxFirstVisibleRow = max(state.metrics.totalRows - state.metrics.visibleRows, 0)
        let targetRow = max(0, min(state.metrics.firstVisibleRow + rowDelta, maxFirstVisibleRow))
        guard targetRow != state.metrics.firstVisibleRow else {
            preciseScrollRemainder = 0
            return nil
        }
        return targetRow
    }

    func shouldForwardScrollToTerminal() -> Bool {
        state.metrics.mode == .alternateScreen || state.metrics.mode == .mouseReporting
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

@MainActor
final class AlanTerminalInputRouter {
    private enum PrimaryButtonSequence {
        case idle
        case suppressingFocusTransfer
    }

    private let pointerAdapter = AlanTerminalPointerAdapter()
    private var primaryButtonSequence = PrimaryButtonSequence.idle

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
        if let action = routeShellAction(input) {
            return .shellAction(action.id, action.target)
        }

        if input.phase == .down,
           input.modifiers == .command,
           input.characters?.lowercased() == "q"
        {
            return .nativeCommand("quit")
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
        guard input.phase == .down, !input.isRepeat else { return nil }

        guard let shortcut = shellActionShortcut(for: input) else { return nil }
        do {
            return try ShellCoreFFIAdapter.shared.keyboardAction(for: shortcut)
        } catch {
            return nil
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

struct AlanTerminalSearchState: Equatable {
    let paneID: String
    let query: String
    let isActive: Bool
    let scope: AlanTerminalSearchScope
    let totalMatches: Int?
    let selectedIndex: Int?
    let focusRequestID: Int

    static func inactive(paneID: String) -> AlanTerminalSearchState {
        AlanTerminalSearchState(
            paneID: paneID,
            query: "",
            isActive: false,
            scope: .scrollback,
            totalMatches: nil,
            selectedIndex: nil,
            focusRequestID: 0
        )
    }
}

enum AlanTerminalSearchScope: Equatable {
    case scrollback
    case commandOutput(AlanTerminalBufferRange)
}

enum AlanTerminalSearchNavigationDirection: Equatable {
    case next
    case previous
}

enum AlanTerminalPromptNavigationDirection: Equatable {
    case previous
    case next
}

enum AlanTerminalSearchEngineUpdate: Equatable {
    case started(query: String)
    case ended
    case matches(total: Int?)
    case selected(index: Int?)
}

@MainActor
protocol AlanTerminalSearchEngine: AnyObject {
    func setSearchUpdateHandler(_ handler: ((AlanTerminalSearchEngineUpdate) -> Void)?)
    func startSearch() -> Bool
    func updateSearchQuery(_ query: String) -> Bool
    func navigateSearch(_ direction: AlanTerminalSearchNavigationDirection) -> Bool
    func endSearch() -> Bool
}

@MainActor
final class AlanTerminalSearchAdapter {
    private(set) var state: AlanTerminalSearchState

    init(paneID: String) {
        self.state = .inactive(paneID: paneID)
    }

    func requestFocus(scope: AlanTerminalSearchScope? = nil) {
        state = AlanTerminalSearchState(
            paneID: state.paneID,
            query: state.query,
            isActive: true,
            scope: scope ?? state.scope,
            totalMatches: state.totalMatches,
            selectedIndex: state.selectedIndex,
            focusRequestID: state.focusRequestID + 1
        )
    }

    func updateQuery(_ query: String) {
        state = AlanTerminalSearchState(
            paneID: state.paneID,
            query: query,
            isActive: true,
            scope: state.scope,
            totalMatches: state.totalMatches,
            selectedIndex: state.selectedIndex,
            focusRequestID: state.focusRequestID
        )
    }

    func updateMatches(total: Int?, selectedIndex: Int?) {
        let boundedIndex: Int?
        if let total, total > 0, let selectedIndex {
            boundedIndex = max(0, min(selectedIndex, total - 1))
        } else {
            boundedIndex = nil
        }
        state = AlanTerminalSearchState(
            paneID: state.paneID,
            query: state.query,
            isActive: state.isActive,
            scope: state.scope,
            totalMatches: total,
            selectedIndex: boundedIndex,
            focusRequestID: state.focusRequestID
        )
    }

    func next() {
        guard let total = state.totalMatches, total > 0 else { return }
        let current = state.selectedIndex ?? -1
        updateMatches(total: total, selectedIndex: (current + 1) % total)
    }

    func previous() {
        guard let total = state.totalMatches, total > 0 else { return }
        let current = state.selectedIndex ?? 0
        updateMatches(total: total, selectedIndex: (current - 1 + total) % total)
    }

    func dismiss() {
        state = .inactive(paneID: state.paneID)
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

enum AlanTerminalSurfaceUnreadyReason: String, Equatable {
    case missingSurface = "missing_surface"
    case inputNotReady = "input_not_ready"
    case rendererFailed = "renderer_failed"
    case childExited = "child_exited"
    case readonly
}

enum AlanTerminalSurfaceReadiness: Equatable {
    case ready
    case unready(reason: AlanTerminalSurfaceUnreadyReason)
}

struct AlanTerminalOverlayState: Equatable {
    let title: String
    let message: String
    let badge: String
    let action: String?
    let debugDetail: String?
}

struct AlanTerminalSurfaceStateSnapshot: Equatable {
    let readiness: AlanTerminalSurfaceReadiness
    let terminalMode: AlanTerminalMode
    let scrollback: AlanTerminalScrollbackState
    let search: AlanTerminalSearchState?
    let semanticCommands: AlanTerminalSemanticCommandState
    let readonly: Bool
    let secureInput: Bool
    let inputReady: Bool
    let rendererHealth: String
    let childExited: Bool
    let lastUpdatedAt: Date

    static let placeholder = AlanTerminalSurfaceStateSnapshot(
        readiness: .unready(reason: .missingSurface),
        terminalMode: .normalBuffer,
        scrollback: .empty,
        search: nil,
        semanticCommands: .placeholder,
        readonly: false,
        secureInput: false,
        inputReady: false,
        rendererHealth: "pending",
        childExited: false,
        lastUpdatedAt: .now
    )

    func equalsIgnoringTimestamp(_ other: AlanTerminalSurfaceStateSnapshot) -> Bool {
        readiness == other.readiness
            && terminalMode == other.terminalMode
            && scrollback == other.scrollback
            && search == other.search
            && semanticCommands == other.semanticCommands
            && readonly == other.readonly
            && secureInput == other.secureInput
            && inputReady == other.inputReady
            && rendererHealth == other.rendererHealth
            && childExited == other.childExited
    }
}

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

@MainActor
final class AlanTerminalSurfaceController {
    let inputRouter = AlanTerminalInputRouter()
    let scrollbackAdapter = AlanTerminalScrollbackAdapter()
    let nativeScrollViewAdapter = AlanTerminalNativeScrollViewAdapter()
    let metadataAdapter = AlanTerminalMetadataAdapter()
    var onSearchStateChange: (() -> Void)?
    var onSurfaceStateChange: (() -> Void)?

    private(set) var searchAdapter: AlanTerminalSearchAdapter?
    private(set) var clipboardAdapter = AlanTerminalSelectionClipboardAdapter(surfaceHandle: nil)
    private weak var surfaceHandle: AlanTerminalSurfaceHandle?
    private weak var searchEngine: AlanTerminalSearchEngine?
    private weak var scrollbackEngine: AlanTerminalScrollbackEngine?
    private weak var commandBufferEngine: AlanTerminalCommandBufferEngine?
    private weak var selectionEngine: AlanTerminalSelectionEngine?
    private let performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?
    private var semanticCommandState = AlanTerminalSemanticCommandState.placeholder
    private var latestRenderer = TerminalRendererSnapshot.placeholder
    private var latestMetadata = TerminalPaneMetadataSnapshot.placeholder
    private var readonly = false
    private var secureInput = false
    private var nativeScrollRowHeight: CGFloat = 1

    init(diagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil) {
        self.performanceDiagnosticsRecorder = diagnosticsRecorder
        nativeScrollViewAdapter.onVisibleRowChange = { [weak self] row in
            self?.scrollToNativeRow(row)
        }
    }

    var isSurfaceReady: Bool {
        surfaceReadiness == .ready
    }

    var surfaceStateSnapshot: AlanTerminalSurfaceStateSnapshot {
        AlanTerminalSurfaceStateSnapshot(
            readiness: surfaceReadiness,
            terminalMode: scrollbackAdapter.state.metrics.mode,
            scrollback: scrollbackAdapter.state,
            search: searchAdapter?.state,
            semanticCommands: semanticCommandState,
            readonly: readonly,
            secureInput: secureInput,
            inputReady: isSurfaceReady,
            rendererHealth: latestRenderer.phase == .failed ? "failed" : latestRenderer.phase.rawValue,
            childExited: latestMetadata.processExited,
            lastUpdatedAt: .now
        )
    }

    func bind(surfaceHandle: AlanTerminalSurfaceHandle?, paneID: String?) {
        let surfaceChanged = self.surfaceHandle !== surfaceHandle
        if surfaceChanged {
            self.surfaceHandle?.detach()
            self.surfaceHandle = surfaceHandle
        }
        let nextSearchEngine = surfaceHandle as? AlanTerminalSearchEngine
        if searchEngine !== nextSearchEngine {
            searchEngine?.setSearchUpdateHandler(nil)
            searchEngine = nextSearchEngine
            searchEngine?.setSearchUpdateHandler { [weak self] update in
                self?.applySearchEngineUpdate(update)
            }
        }
        let nextScrollbackEngine = surfaceHandle as? AlanTerminalScrollbackEngine
        if scrollbackEngine !== nextScrollbackEngine {
            scrollbackEngine?.setScrollbackUpdateHandler(nil)
            scrollbackEngine = nextScrollbackEngine
            resetPerSurfaceStateForSurfaceChange()
            scrollbackEngine?.setScrollbackUpdateHandler { [weak self] metrics in
                self?.applyScrollbackMetrics(metrics)
            }
        } else if surfaceChanged {
            resetPerSurfaceStateForSurfaceChange()
        }
        commandBufferEngine = surfaceHandle as? AlanTerminalCommandBufferEngine
        selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine
        clipboardAdapter.updateSurfaceHandle(surfaceHandle)
        if let paneID, searchAdapter?.state.paneID != paneID {
            searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
            semanticCommandState = .unavailable(
                paneID: paneID,
                reason: "Semantic command boundary signals are not available for this pane."
            )
        } else if paneID == nil {
            searchAdapter = nil
            semanticCommandState = .placeholder
        }
    }

    func attach(
        to canvasView: NSView,
        bootProfile: AlanShellBootProfile?,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        guard let surfaceHandle else { return }
        let attachStartedAt = performanceDiagnosticsStartTime()
        surfaceHandle.configure(mountedAtPaneID: surfaceHandle.paneID, bootProfile: bootProfile)
        surfaceHandle.updateRenderPriority(renderPriority, forceCatchUp: false)
        surfaceHandle.attach(
            to: canvasView,
            focused: focused,
            renderPriority: renderPriority,
            onDiagnosticsChange: { [weak self] snapshot in
                guard let self else { return }
                latestRenderer = snapshot
                if performanceDiagnosticsStartTime() != nil {
                    recordPerformanceDiagnostic(
                        .terminalRendererUpdate,
                        durationMs: 0,
                        counts: AlanPerformanceDiagnosticCounts(events: snapshot.recentEvents.count)
                    )
                }
                onDiagnosticsChange(snapshot)
            },
            onMetadataChange: { [weak self] metadata in
                guard let self else { return }
                latestMetadata = metadata
                onMetadataChange(metadata)
            },
            onCloseRequest: onCloseRequest
        )
        if let attachStartedAt {
            recordPerformanceDiagnostic(
                .terminalSurfaceAttach,
                durationMs: performanceDurationMs(since: attachStartedAt),
                priority: renderPriority
            )
        }
    }

    func detach() {
        surfaceHandle?.detach()
        surfaceHandle = nil
        searchEngine?.setSearchUpdateHandler(nil)
        searchEngine = nil
        scrollbackEngine?.setScrollbackUpdateHandler(nil)
        scrollbackEngine = nil
        commandBufferEngine = nil
        selectionEngine = nil
        semanticCommandState = .placeholder
        resetPerSurfaceStateForSurfaceChange()
        clipboardAdapter.updateSurfaceHandle(nil)
    }

    func updateRenderer(_ renderer: TerminalRendererSnapshot) {
        let updateStartedAt = performanceDiagnosticsStartTime()
        latestRenderer = renderer
        if let updateStartedAt {
            recordPerformanceDiagnostic(
                .terminalRendererUpdate,
                durationMs: performanceDurationMs(since: updateStartedAt),
                counts: AlanPerformanceDiagnosticCounts(events: renderer.recentEvents.count)
            )
        }
    }

    func updateMetadata(_ metadata: TerminalPaneMetadataSnapshot) {
        latestMetadata = metadata
    }

    func overlayState(
        renderer: TerminalRendererSnapshot,
        metadata: TerminalPaneMetadataSnapshot,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalOverlayState? {
        let readiness: AlanTerminalSurfaceReadiness
        if bootProfile == nil || surfaceHandle == nil {
            readiness = .unready(reason: .missingSurface)
        } else {
            readiness = surfaceReadiness
        }
        return metadataAdapter.overlayState(renderer: renderer, metadata: metadata, surface: readiness)
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return .accepted(byteCount: 0, runtimePhase: surfaceHandle?.snapshot.runtimePhase)
        }
        guard let surfaceHandle else {
            return .rejected(
                errorCode: "terminal_runtime_unavailable",
                errorMessage: "No service-owned terminal surface is attached to this host."
            )
        }
        guard !surfaceHandle.snapshot.metadata.processExited else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: surfaceHandle.snapshot.runtimePhase
            )
        }
        guard surfaceHandle.isSurfaceReady else {
            return .rejected(
                errorCode: "terminal_runtime_unavailable",
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: surfaceHandle.snapshot.runtimePhase
            )
        }
        return surfaceHandle.sendControlText(text)
    }

    func syncNativeScrollView(viewportSize: CGSize) {
        let visibleRows = max(scrollbackAdapter.state.metrics.visibleRows, 1)
        let rowHeight = viewportSize.height / CGFloat(visibleRows)
        nativeScrollRowHeight = max(rowHeight, 1)
        nativeScrollViewAdapter.sync(
            state: scrollbackAdapter.state,
            viewportSize: viewportSize,
            rowHeight: nativeScrollRowHeight
        )
    }

    func routeScroll(_ input: AlanTerminalScrollInput) -> AlanTerminalScrollRoutingDecision {
        guard isSurfaceReady else { return .ignored }
        guard !scrollbackAdapter.shouldForwardScrollToTerminal() else { return .terminalScroll }
        guard scrollbackAdapter.shouldConsumeNativeScrollInput(input) else {
            scrollbackAdapter.resetPreciseScrollAccumulator()
            return .terminalScroll
        }
        guard let row = scrollbackAdapter.targetFirstVisibleRow(
            for: input,
            rowHeight: nativeScrollRowHeight
        ) else { return .ignored }
        guard scrollToNativeRow(row) else { return .terminalScroll }
        return .nativeScroll(row: row)
    }

    func routePointer(_ input: AlanTerminalPointerInput) -> AlanTerminalPointerRoutingDecision {
        inputRouter.routePointer(
            input,
            terminalMode: scrollbackAdapter.state.metrics.mode,
            surfaceReady: isSurfaceReady
        )
    }

    func routeKeyboard(
        _ input: AlanTerminalKeyInput,
        hasMarkedText: Bool
    ) -> AlanTerminalKeyboardRoutingDecision {
        inputRouter.routeKeyboard(input, hasMarkedText: hasMarkedText)
    }

    func routeLeftMouseDown(
        hitOwnsTerminal: Bool,
        commandSurfaceVisible: Bool,
        isFirstResponder: Bool,
        appIsActive: Bool,
        windowIsKey: Bool
    ) -> AlanTerminalLeftMouseDownRoutingDecision {
        inputRouter.routeLeftMouseDown(
            hitOwnsTerminal: hitOwnsTerminal,
            commandSurfaceVisible: commandSurfaceVisible,
            isFirstResponder: isFirstResponder,
            appIsActive: appIsActive,
            windowIsKey: windowIsKey
        )
    }

    func resetInputRouting() {
        inputRouter.reset()
    }

    @discardableResult
    private func scrollToNativeRow(_ row: Int) -> Bool {
        guard scrollbackEngine?.scrollTo(row: row) == true else { return false }
        scrollbackAdapter.scrollTo(firstVisibleRow: row)
        notifySurfaceStateChanged()
        return true
    }

    func copySelection(to pasteboard: NSPasteboard = .general) -> Bool {
        clipboardAdapter.writeSelectionToPasteboard(selectionEngine?.readSelectionText(), pasteboard: pasteboard)
    }

    func copySelection(to writer: AlanTerminalPasteboardWriting) -> Bool {
        clipboardAdapter.writeSelection(selectionEngine?.readSelectionText(), to: writer)
    }

    func paste(_ text: String) -> TerminalRuntimeDeliveryResult {
        clipboardAdapter.paste(text)
    }

    func readSelectionText() -> String? {
        selectionEngine?.readSelectionText()
    }

    func hasSelection() -> Bool {
        selectionEngine?.hasSelection() ?? false
    }

    func updateSemanticCommands(_ state: AlanTerminalSemanticCommandState) {
        semanticCommandState = state
        notifySurfaceStateChanged()
    }

    func invalidateSemanticCommands(reason: String) {
        semanticCommandState = AlanTerminalSemanticCommandState(
            paneID: semanticCommandState.paneID ?? searchAdapter?.state.paneID ?? surfaceHandle?.paneID,
            boundaryState: .stale(reason: reason),
            segments: semanticCommandState.segments,
            lastUpdatedAt: .now
        )
        notifySurfaceStateChanged()
    }

    var hasReliableSemanticCommandActions: Bool {
        scrollbackAdapter.state.metrics.mode == .normalBuffer
            && semanticCommandState.hasReliableCommandBoundaries
    }

    @discardableResult
    func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              semanticCommandState.hasReliablePromptMarks
        else {
            return false
        }

        let promptRows = semanticCommandState
            .reliableSegments
            .compactMap { $0.promptRange?.lowerBound }
            .sorted()
        guard !promptRows.isEmpty else { return false }

        let currentRow = scrollbackAdapter.state.metrics.firstVisibleRow
        let targetRow: Int?
        switch direction {
        case .previous:
            targetRow = promptRows.reversed().first { $0 < currentRow } ?? promptRows.last
        case .next:
            targetRow = promptRows.first { $0 > currentRow } ?? promptRows.first
        }

        guard let targetRow else { return false }
        return scrollToNativeRow(targetRow)
    }

    @discardableResult
    func copyLastCommandOutput(to writer: AlanTerminalPasteboardWriting) -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              let outputRange = semanticCommandState.lastReliableOutputRange
        else {
            return copySelection(to: writer)
        }
        if outputRange.isEmpty {
            return writer.writeString("")
        }
        guard let output = commandBufferEngine?.readText(in: outputRange) else {
            return copySelection(to: writer)
        }

        return writer.writeString(output)
    }

    @discardableResult
    func copyLastCommandOutput(to pasteboard: NSPasteboard = .general) -> Bool {
        copyLastCommandOutput(to: AlanTerminalSystemPasteboardWriter(pasteboard: pasteboard))
    }

    @discardableResult
    func beginLastCommandOutputSearch() -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              let outputRange = semanticCommandState.lastReliableOutputRange
        else {
            return beginSearch()
        }

        return beginSearch(scope: .commandOutput(outputRange))
    }

    @discardableResult
    func beginSearch(scope: AlanTerminalSearchScope = .scrollback) -> Bool {
        guard let paneID = searchAdapter?.state.paneID ?? surfaceHandle?.paneID else { return false }
        if searchAdapter == nil {
            searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
        }
        if searchAdapter?.state.isActive == true,
           searchAdapter?.state.scope == scope
        {
            searchAdapter?.requestFocus(scope: scope)
            notifySearchStateChanged()
            return true
        }
        guard searchEngine?.startSearch() == true else { return false }
        searchAdapter?.requestFocus(scope: scope)
        searchAdapter?.updateQuery(searchAdapter?.state.query ?? "")
        return true
    }

    @discardableResult
    func updateSearchQuery(_ query: String) -> Bool {
        let scope = searchAdapter?.state.isActive == true
            ? (searchAdapter?.state.scope ?? .scrollback)
            : .scrollback
        guard beginSearch(scope: scope) else { return false }
        guard searchEngine?.updateSearchQuery(query) == true else { return false }
        searchAdapter?.updateQuery(query)
        return true
    }

    func nextSearchMatch() {
        if searchEngine?.navigateSearch(.next) != true {
            searchAdapter?.next()
        }
    }

    func previousSearchMatch() {
        if searchEngine?.navigateSearch(.previous) != true {
            searchAdapter?.previous()
        }
    }

    func dismissSearch() {
        _ = searchEngine?.endSearch()
        searchAdapter?.dismiss()
    }

    private func applySearchEngineUpdate(_ update: AlanTerminalSearchEngineUpdate) {
        switch update {
        case .started(let query):
            guard let paneID = searchAdapter?.state.paneID ?? surfaceHandle?.paneID else { return }
            if searchAdapter == nil {
                searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
            }
            searchAdapter?.updateQuery(query)
        case .ended:
            searchAdapter?.dismiss()
        case .matches(let total):
            searchAdapter?.updateMatches(
                total: total,
                selectedIndex: searchAdapter?.state.selectedIndex
            )
        case .selected(let index):
            searchAdapter?.updateMatches(
                total: searchAdapter?.state.totalMatches,
                selectedIndex: index
            )
        }
        notifySearchStateChanged()
    }

    private func applyScrollbackMetrics(_ metrics: AlanTerminalScrollbackMetrics) {
        let updateStartedAt = performanceDiagnosticsStartTime()
        scrollbackAdapter.updateMetrics(metrics)
        notifySurfaceStateChanged()
        if let updateStartedAt {
            recordPerformanceDiagnostic(
                .terminalScrollbackUpdate,
                durationMs: performanceDurationMs(since: updateStartedAt),
                counts: AlanPerformanceDiagnosticCounts(lines: metrics.totalRows)
            )
        }
    }

    private func recordPerformanceDiagnostic(
        _ kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double,
        priority: TerminalRuntimeRenderPriority? = nil,
        counts: AlanPerformanceDiagnosticCounts? = nil
    ) {
        if let performanceDiagnosticsRecorder {
            guard performanceDiagnosticsRecorder.isEnabled else { return }
        } else {
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else { return }
        }
        let resolvedPriority = priority ?? surfaceHandle?.renderPriority
        let event = AlanPerformanceDiagnosticEvent(
            kind: kind,
            durationMs: durationMs,
            paneID: surfaceHandle?.paneID,
            contentID: surfaceHandle?.contentID,
            priority: resolvedPriority?.diagnosticsValue,
            visibility: resolvedPriority?.diagnosticsVisibility,
            thread: Thread.isMainThread ? "main" : "background",
            counts: counts
        )
        if let performanceDiagnosticsRecorder {
            performanceDiagnosticsRecorder.record(event)
        } else {
            AlanPerformanceDiagnosticsController.shared.record(
                kind,
                durationMs: durationMs,
                paneID: event.paneID,
                contentID: event.contentID,
                priority: event.priority,
                visibility: event.visibility,
                thread: event.thread,
                counts: event.counts
            )
        }
    }

    private func performanceDurationMs(since start: DispatchTime) -> Double {
        let end = DispatchTime.now()
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    private func performanceDiagnosticsStartTime() -> DispatchTime? {
        if let performanceDiagnosticsRecorder {
            return performanceDiagnosticsRecorder.isEnabled ? DispatchTime.now() : nil
        }
        return AlanPerformanceDiagnosticsController.shared.isEnabled ? DispatchTime.now() : nil
    }

    private func resetPerSurfaceStateForSurfaceChange() {
        let previousState = scrollbackAdapter.state
        let previousRenderer = latestRenderer
        let previousMetadata = latestMetadata
        let previousSemanticCommandState = semanticCommandState
        let previousReadonly = readonly
        let previousSecureInput = secureInput
        scrollbackAdapter.reset()
        inputRouter.reset()
        if let paneID = surfaceHandle?.paneID {
            semanticCommandState = .unavailable(
                paneID: paneID,
                reason: "Terminal surface changed; command boundary ranges were invalidated."
            )
        } else {
            semanticCommandState = .placeholder
        }
        nativeScrollRowHeight = 1
        latestRenderer = .placeholder
        latestMetadata = .placeholder
        readonly = false
        secureInput = false
        if previousState != .empty
            || previousRenderer != .placeholder
            || previousMetadata != .placeholder
            || (
                previousSemanticCommandState.hasReliableCommandBoundaries
                    && previousSemanticCommandState != semanticCommandState
            )
            || previousReadonly
            || previousSecureInput
        {
            notifySurfaceStateChanged()
        }
    }

    private func notifySearchStateChanged() {
        onSearchStateChange?()
        notifySurfaceStateChanged()
    }

    private func notifySurfaceStateChanged() {
        onSurfaceStateChange?()
    }

    private var surfaceReadiness: AlanTerminalSurfaceReadiness {
        guard let surfaceHandle else { return .unready(reason: .missingSurface) }
        if latestMetadata.processExited || surfaceHandle.snapshot.metadata.processExited {
            return .unready(reason: .childExited)
        }
        if latestRenderer.phase == .failed || surfaceHandle.snapshot.renderer.phase == .failed {
            return .unready(reason: .rendererFailed)
        }
        if readonly {
            return .unready(reason: .readonly)
        }
        guard surfaceHandle.isSurfaceReady else {
            return .unready(reason: .inputNotReady)
        }
        return .ready
    }
}

#if canImport(GhosttyKit)
extension AlanTerminalSurfaceController {
    var ghosttySurfaceHandle: AlanGhosttyEventSurfaceHandle? {
        surfaceHandle as? AlanGhosttyEventSurfaceHandle
    }

    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e {
        ghosttySurfaceHandle?.keyTranslationMods(for: mods) ?? mods
    }

    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        ghosttySurfaceHandle?.sendKey(keyEvent) ?? false
    }

    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool {
        ghosttySurfaceHandle?.keyIsBinding(keyEvent, flags: flags) ?? false
    }

    func sendProgrammaticText(_ text: String) {
        ghosttySurfaceHandle?.sendProgrammaticText(text)
    }

    func sendPreedit(_ text: String?) {
        ghosttySurfaceHandle?.sendPreedit(text)
    }

    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e) {
        ghosttySurfaceHandle?.sendMousePosition(x: x, y: y, mods: mods)
    }

    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool {
        ghosttySurfaceHandle?.sendMouseButton(state: state, button: button, mods: mods) ?? false
    }

    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t) {
        ghosttySurfaceHandle?.sendMouseScroll(x: x, y: y, mods: mods)
    }

    func sendMousePressure(stage: UInt32, pressure: Double) {
        ghosttySurfaceHandle?.sendMousePressure(stage: stage, pressure: pressure)
    }

    func imeRect(in view: NSView) -> NSRect? {
        ghosttySurfaceHandle?.imeRect(in: view)
    }
}
#endif
#endif
