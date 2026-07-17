#if os(macOS)
import Foundation

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

#endif
