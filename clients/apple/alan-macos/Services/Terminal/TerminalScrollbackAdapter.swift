#if os(macOS)
import Foundation

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

#endif
