import Foundation


enum TerminalRuntimeControlKey: String, Codable, Equatable {
    case interrupt
    case endOfTransmission = "end_of_transmission"
    case returnKey = "return"
}

enum ShellAttentionState: String, Codable, CaseIterable {
    case idle
    case active
    case awaitingUser = "awaiting_user"
    case notable

    /// Signal-semantics gate (docs/design/design-language.md, "Signal Semantics"):
    /// only states blocked on the user (input/approval) or a failure needing
    /// intervention may surface `ShellSignal.action`. `.active` is quiet
    /// liveness and `.idle` is silence — both must stay inkless in chrome.
    var requiresUserAction: Bool {
        switch self {
        case .awaitingUser, .notable:
            return true
        case .idle, .active:
            return false
        }
    }
}

enum ShellTabKind: String, Codable, CaseIterable {
    case terminal
    case scratch
    case log
}

enum ShellTabOrganizationSection: String, Codable, CaseIterable {
    case pinned
    case unpinned
}

struct ShellTabOrganizationLocation: Codable, Equatable {
    let spaceID: String
    let section: ShellTabOrganizationSection
    let index: Int

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case section
        case index
    }
}

enum ShellPaneTreeKind: String, Codable {
    case split
    case pane
}

enum ShellSplitDirection: String, Codable {
    case horizontal
    case vertical
}

enum ShellPaneSplitDirection: String, Codable, CaseIterable {
    case left
    case right
    case up
    case down

    var splitDirection: ShellSplitDirection {
        switch self {
        case .left, .right:
            return .vertical
        case .up, .down:
            return .horizontal
        }
    }

    var placesNewPaneBeforeTarget: Bool {
        switch self {
        case .left, .up:
            return true
        case .right, .down:
            return false
        }
    }

    var spatialFocusDirection: ShellSpatialFocusDirection {
        switch self {
        case .left:
            return .left
        case .right:
            return .right
        case .up:
            return .up
        case .down:
            return .down
        }
    }

    static func defaultPlacement(for splitDirection: ShellSplitDirection) -> ShellPaneSplitDirection {
        switch splitDirection {
        case .horizontal:
            return .down
        case .vertical:
            return .right
        }
    }
}

enum ShellSpatialFocusDirection: String, Codable, CaseIterable {
    case left
    case right
    case up
    case down

    var splitDirection: ShellSplitDirection {
        switch self {
        case .left, .right:
            return .vertical
        case .up, .down:
            return .horizontal
        }
    }

    var movesForward: Bool {
        switch self {
        case .right, .down:
            return true
        case .left, .up:
            return false
        }
    }
}

enum ShellWorkspaceCommand: String, Codable, CaseIterable, Identifiable {
    case newTerminalTab
    case splitLeft
    case splitRight
    case splitUp
    case splitDown
    case focusLeft
    case focusRight
    case focusUp
    case focusDown
    case equalizeSplits
    case togglePaneZoom
    case movePaneLeft
    case movePaneRight
    case movePaneUp
    case movePaneDown
    case closePane
    case closeTab

    var id: String { rawValue }
}

enum ShellLaunchTarget: String, Codable, CaseIterable {
    case shell
}
