#if os(macOS)
import Foundation

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

#endif
