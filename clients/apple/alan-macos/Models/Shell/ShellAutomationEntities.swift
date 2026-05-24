import Foundation

#if os(macOS) && canImport(AppIntents)
import AppIntents

struct ShellAutomationAppEntitySnapshot: Equatable, Sendable {
    let windows: [AlanShellWindowEntity]
    let spaces: [AlanShellSpaceEntity]
    let tabs: [AlanShellTabEntity]
    let panes: [AlanShellPaneEntity]
    let attentionItems: [AlanShellAttentionItemEntity]

    static let empty = ShellAutomationAppEntitySnapshot(
        windows: [],
        spaces: [],
        tabs: [],
        panes: [],
        attentionItems: []
    )

    static func projecting(_ state: ShellStateSnapshot) -> ShellAutomationAppEntitySnapshot {
        let spaces = state.spaces.map { space in
            let displayTitle = firstNonEmptyDisplayTitle([space.title], fallback: "Space")
            return AlanShellSpaceEntity(
                id: space.spaceID,
                windowID: state.windowID,
                displayTitle: displayTitle,
                displaySubtitle: tabCountLabel(space.tabs.count),
                attention: space.attention.rawValue,
                isFocused: space.spaceID == state.focusedSpaceID
            )
        }

        let tabs = state.spaces.flatMap { space in
            let spaceTitle = firstNonEmptyDisplayTitle([space.title], fallback: "Space")
            return space.tabs.map { tab in
                let paneCount = tab.paneTree.paneIDs.count
                let displayTitle = firstNonEmptyDisplayTitle([tab.title], fallback: "Tab")
                return AlanShellTabEntity(
                    id: tab.tabID,
                    windowID: state.windowID,
                    spaceID: space.spaceID,
                    spaceTitle: spaceTitle,
                    displayTitle: displayTitle,
                    displaySubtitle: joinedDisplayParts([
                        spaceTitle,
                        paneCountLabel(paneCount),
                        tab.isPinned ? "Pinned" : nil,
                    ]),
                    kind: tab.kind.rawValue,
                    isPinned: tab.isPinned,
                    isFocused: tab.tabID == state.focusedTabID
                )
            }
        }

        let panes = state.panes.compactMap { pane -> AlanShellPaneEntity? in
            guard let summary = state.automationPaneSummary(paneID: pane.paneID) else {
                return nil
            }
            return AlanShellPaneEntity(summary: summary, isFocused: pane.paneID == state.focusedPaneID)
        }

        let attentionItems = panes
            .filter { $0.attention != ShellAttentionState.idle.rawValue }
            .sorted { lhs, rhs in
                let lhsRank = attentionRank(lhs.attention)
                let rhsRank = attentionRank(rhs.attention)
                return lhsRank == rhsRank ? lhs.displayTitle < rhs.displayTitle : lhsRank > rhsRank
            }
            .map(AlanShellAttentionItemEntity.init(pane:))

        return ShellAutomationAppEntitySnapshot(
            windows: [
                AlanShellWindowEntity(
                    id: state.windowID,
                    displayTitle: "alan shell",
                    displaySubtitle: [
                        spaceCountLabel(spaces.count),
                        tabCountLabel(tabs.count),
                        paneCountLabel(panes.count),
                    ].joined(separator: " - "),
                    focusedSpaceID: state.focusedSpaceID,
                    focusedTabID: state.focusedTabID,
                    focusedPaneID: state.focusedPaneID
                ),
            ],
            spaces: spaces,
            tabs: tabs,
            panes: panes,
            attentionItems: attentionItems
        )
    }

    private static func firstNonEmptyDisplayTitle(
        _ candidates: [String?],
        fallback: String
    ) -> String {
        for candidate in candidates {
            guard let trimmed = candidate?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !trimmed.isEmpty
            else {
                continue
            }
            return trimmed
        }
        return fallback
    }

    private static func spaceCountLabel(_ count: Int) -> String {
        count == 1 ? "1 space" : "\(count) spaces"
    }

    private static func tabCountLabel(_ count: Int) -> String {
        count == 1 ? "1 tab" : "\(count) tabs"
    }

    private static func paneCountLabel(_ count: Int) -> String {
        count == 1 ? "1 pane" : "\(count) panes"
    }

    private static func attentionRank(_ rawAttention: String) -> Int {
        switch ShellAttentionState(rawValue: rawAttention) {
        case .awaitingUser:
            return 3
        case .notable:
            return 2
        case .active:
            return 1
        case .idle, nil:
            return 0
        }
    }
}

@MainActor
enum ShellAutomationEntityStore {
    private static var snapshotProvider: (@MainActor () -> ShellStateSnapshot?)?

    static func install(snapshotProvider: @escaping @MainActor () -> ShellStateSnapshot?) {
        self.snapshotProvider = snapshotProvider
    }

    static func reset() {
        snapshotProvider = nil
    }

    static func currentSnapshot() -> ShellAutomationAppEntitySnapshot {
        guard let state = snapshotProvider?() else {
            return .empty
        }
        return ShellAutomationAppEntitySnapshot.projecting(state)
    }
}

struct AlanShellWindowEntity: AppEntity, Equatable, Sendable {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Shell Window")
    static var defaultQuery = AlanShellWindowQuery()

    let id: String
    let displayTitle: String
    let displaySubtitle: String?
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?

    var displayRepresentation: DisplayRepresentation {
        shellEntityDisplayRepresentation(title: displayTitle, subtitle: displaySubtitle)
    }
}

struct AlanShellSpaceEntity: AppEntity, Equatable, Sendable {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Shell Space")
    static var defaultQuery = AlanShellSpaceQuery()

    let id: String
    let windowID: String
    let displayTitle: String
    let displaySubtitle: String?
    let attention: String
    let isFocused: Bool

    var displayRepresentation: DisplayRepresentation {
        shellEntityDisplayRepresentation(title: displayTitle, subtitle: displaySubtitle)
    }
}

struct AlanShellTabEntity: AppEntity, Equatable, Sendable {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Shell Tab")
    static var defaultQuery = AlanShellTabQuery()

    let id: String
    let windowID: String
    let spaceID: String
    let spaceTitle: String
    let displayTitle: String
    let displaySubtitle: String?
    let kind: String
    let isPinned: Bool
    let isFocused: Bool

    var displayRepresentation: DisplayRepresentation {
        shellEntityDisplayRepresentation(title: displayTitle, subtitle: displaySubtitle)
    }
}

struct AlanShellPaneEntity: AppEntity, Equatable, Sendable {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Shell Pane")
    static var defaultQuery = AlanShellPaneQuery()

    let id: String
    let windowID: String
    let spaceID: String
    let spaceTitle: String
    let tabID: String
    let tabTitle: String
    let displayTitle: String
    let displaySubtitle: String?
    let workingDirectory: String?
    let processProgram: String?
    let processState: String?
    let attention: String
    let isFocused: Bool

    init(summary: ShellAutomationPaneSummary, isFocused: Bool) {
        id = summary.paneID
        windowID = summary.windowID
        spaceID = summary.spaceID
        spaceTitle = summary.spaceTitle
        tabID = summary.tabID
        tabTitle = summary.tabTitle
        displayTitle = summary.paneTitle
        displaySubtitle = joinedDisplayParts([
            summary.workingDirectory,
            summary.processProgram,
            summary.processState,
            summary.attention == .idle ? nil : summary.attention.rawValue,
        ])
        workingDirectory = summary.workingDirectory
        processProgram = summary.processProgram
        processState = summary.processState
        attention = summary.attention.rawValue
        self.isFocused = isFocused
    }

    var displayRepresentation: DisplayRepresentation {
        shellEntityDisplayRepresentation(title: displayTitle, subtitle: displaySubtitle)
    }
}

struct AlanShellAttentionItemEntity: AppEntity, Equatable, Sendable {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Shell Attention Item")
    static var defaultQuery = AlanShellAttentionItemQuery()

    let id: String
    let paneID: String
    let windowID: String
    let spaceID: String
    let spaceTitle: String
    let tabID: String
    let tabTitle: String
    let displayTitle: String
    let displaySubtitle: String?
    let attention: String

    init(pane: AlanShellPaneEntity) {
        id = "attention:\(pane.id)"
        paneID = pane.id
        windowID = pane.windowID
        spaceID = pane.spaceID
        spaceTitle = pane.spaceTitle
        tabID = pane.tabID
        tabTitle = pane.tabTitle
        displayTitle = pane.displayTitle
        displaySubtitle = joinedDisplayParts([
            pane.attention,
            pane.spaceTitle,
            pane.tabTitle,
            pane.workingDirectory,
        ])
        attention = pane.attention
    }

    var displayRepresentation: DisplayRepresentation {
        shellEntityDisplayRepresentation(title: displayTitle, subtitle: displaySubtitle)
    }
}

struct AlanShellWindowQuery: EntityQuery {
    func entities(for identifiers: [AlanShellWindowEntity.ID]) async throws -> [AlanShellWindowEntity] {
        let requested = Set(identifiers)
        let windows = await ShellAutomationEntityStore.currentSnapshot().windows
        return windows.filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [AlanShellWindowEntity] {
        await ShellAutomationEntityStore.currentSnapshot().windows
    }
}

struct AlanShellSpaceQuery: EntityQuery {
    func entities(for identifiers: [AlanShellSpaceEntity.ID]) async throws -> [AlanShellSpaceEntity] {
        let requested = Set(identifiers)
        let spaces = await ShellAutomationEntityStore.currentSnapshot().spaces
        return spaces.filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [AlanShellSpaceEntity] {
        await ShellAutomationEntityStore.currentSnapshot().spaces
    }
}

struct AlanShellTabQuery: EntityQuery {
    func entities(for identifiers: [AlanShellTabEntity.ID]) async throws -> [AlanShellTabEntity] {
        let requested = Set(identifiers)
        let tabs = await ShellAutomationEntityStore.currentSnapshot().tabs
        return tabs.filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [AlanShellTabEntity] {
        await ShellAutomationEntityStore.currentSnapshot().tabs
    }
}

struct AlanShellPaneQuery: EntityQuery {
    func entities(for identifiers: [AlanShellPaneEntity.ID]) async throws -> [AlanShellPaneEntity] {
        let requested = Set(identifiers)
        let panes = await ShellAutomationEntityStore.currentSnapshot().panes
        return panes.filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [AlanShellPaneEntity] {
        await ShellAutomationEntityStore.currentSnapshot().panes
    }
}

struct AlanShellAttentionItemQuery: EntityQuery {
    func entities(
        for identifiers: [AlanShellAttentionItemEntity.ID]
    ) async throws -> [AlanShellAttentionItemEntity] {
        let requested = Set(identifiers)
        let attentionItems = await ShellAutomationEntityStore.currentSnapshot().attentionItems
        return attentionItems.filter { requested.contains($0.id) }
    }

    func suggestedEntities() async throws -> [AlanShellAttentionItemEntity] {
        await ShellAutomationEntityStore.currentSnapshot().attentionItems
    }
}

private func shellEntityDisplayRepresentation(
    title: String,
    subtitle: String?
) -> DisplayRepresentation {
    DisplayRepresentation(
        title: "\(title)",
        subtitle: subtitle.map { "\($0)" }
    )
}

private func joinedDisplayParts(_ parts: [String?]) -> String? {
    let displayParts = parts.compactMap { value in
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed?.isEmpty == false ? trimmed : nil
    }
    return displayParts.isEmpty ? nil : displayParts.joined(separator: " - ")
}
#endif
