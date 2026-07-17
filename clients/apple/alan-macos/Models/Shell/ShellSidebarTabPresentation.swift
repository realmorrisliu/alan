import Foundation

struct ShellSidebarTabProjection: Equatable {
    let title: String
    let secondaryLine: String?
    let activity: TerminalActivitySnapshot?
    let progress: TerminalActivityProgress?
    let stateAccessory: ShellSidebarTabStateAccessory?
    let accessibilityActivityLabel: String?
    // True only when `secondaryLine` came from the cwd/branch/process context
    // source (machine facts). Status summaries and content-type hints are human
    // language and set this false so they stay in SF Pro. See
    // docs/design/design-language.md, principle 4.
    var secondaryIsMachineFact: Bool = false
}
struct ShellSidebarTabStateAccessory: Equatable {
    let systemImageName: String
    let accessibilityLabel: String
}

struct ShellSidebarTemporaryTabSectionPresentation: Equatable {
    let showsControlRow: Bool
    let showsDivider: Bool
    let showsClear: Bool
    let isClearEnabled: Bool

    static func model(
        pinnedTabCount: Int,
        unpinnedTabCount: Int,
        clearableTabCount: Int
    ) -> ShellSidebarTemporaryTabSectionPresentation {
        let hasUnpinnedTabs = unpinnedTabCount > 0
        let hasClearableTabs = hasUnpinnedTabs && clearableTabCount > 0
        return ShellSidebarTemporaryTabSectionPresentation(
            showsControlRow: hasUnpinnedTabs,
            showsDivider: hasUnpinnedTabs,
            showsClear: hasClearableTabs,
            isClearEnabled: hasClearableTabs
        )
    }
}

struct ShellSidebarTabContextMenuModel: Equatable {
    let primaryActionTitles: [String]
    let organizationActionTitles: [String]
    let destructiveActionTitles: [String]

    var allActionTitles: [String] {
        primaryActionTitles + organizationActionTitles + destructiveActionTitles
    }

    static func model(
        tabID: String,
        in spaceID: String,
        state: ShellStateSnapshot
    ) throws -> ShellSidebarTabContextMenuModel {
        guard let tab = state.tab(tabID: tabID),
              state.space(spaceID: spaceID)?.tabs.contains(where: { $0.tabID == tabID }) == true
        else {
            throw ShellStateMutationError.tabNotFound
        }

        let primary = [
            "Rename...",
            "Duplicate Tab",
            "Open in Split View",
        ]
        var organization = [
            tab.isPinned ? "Unpin Tab" : "Pin Tab",
        ]
        if state.spaces.contains(where: { $0.spaceID != spaceID }) {
            organization.append("Move to")
        }

        return ShellSidebarTabContextMenuModel(
            primaryActionTitles: primary,
            organizationActionTitles: organization,
            destructiveActionTitles: ["Close Tab"]
        )
    }
}

func shellSidebarTabProjection(
    for tab: ShellTab,
    panes allPanes: [ShellPane],
    contentState: ShellContentStateSnapshot? = nil,
    focusedPaneID: String?,
    focusedTabID: String?,
    now: Date? = nil
) -> ShellSidebarTabProjection {
    let panes = shellOrderedPanes(for: tab, panes: allPanes)
    let primaryPane = shellPrimaryPane(in: panes, focusedPaneID: focusedPaneID)
    let primaryContent = shellPrimaryContent(in: tab, contentState: contentState, focusedPaneID: focusedPaneID)
    let isOwningTabFocused = focusedTabID == tab.tabID

    let activityCandidates = panes.enumerated().compactMap { index, pane -> TerminalActivitySnapshot? in
        if let contentState,
           contentState.contentMounted(in: pane.paneID)?.kind != .terminal
        {
            return nil
        }
        guard let activity = pane.activity,
              activity.isSidebarWorthy(at: now, owningTabFocused: isOwningTabFocused)
        else { return nil }

        let hint: String?
        if panes.count > 1,
           pane.paneID != primaryPane?.paneID
        {
            hint = "Pane \(index + 1)"
        } else {
            hint = nil
        }
        return activity.withPaneHint(hint)
    }
    let primaryActivity = TerminalActivitySnapshot.primarySidebarActivity(activityCandidates, now: now)
    let activityTaskTitle = primaryActivity.flatMap(shellSidebarTaskTitle)
    let fallbackTitle = shellSidebarTabTitle(for: tab, primaryPane: primaryPane, primaryContent: primaryContent)
    let title = tab.isTitleUserLocked ? fallbackTitle : (activityTaskTitle ?? fallbackTitle)
    let contextLine = primaryPane.flatMap { shellSidebarContextLine(for: $0, title: title) }

    if let activity = primaryActivity {
        let subtitle = shellSidebarActivitySubtitle(
            for: activity,
            contextLine: contextLine,
            hasTaskTitle: activityTaskTitle != nil
        )
        return ShellSidebarTabProjection(
            title: title,
            secondaryLine: subtitle,
            activity: activity,
            progress: activity.progress,
            stateAccessory: shellSidebarStateAccessory(for: activity),
            accessibilityActivityLabel: activity.display.sourceFirstLabel
        )
    }

    // Content-type hint ("Document"/"Settings") and status summary ("Exited"/…)
    // are human language; only the cwd/branch/process context line is a machine
    // fact and may render in the mono accent track.
    let humanFallback = shellSidebarContentLine(for: primaryContent)
        ?? meaningfulSidebarFallbackLine(
            primaryPane.flatMap { shellTerminalStatusSummary(for: $0, now: now) },
            title: title
        )
    let machineFallback = meaningfulSidebarContextLine(contextLine, title: title)
    let fallback = humanFallback ?? machineFallback
    return ShellSidebarTabProjection(
        title: title,
        secondaryLine: fallback,
        activity: nil,
        progress: nil,
        stateAccessory: nil,
        accessibilityActivityLabel: nil,
        secondaryIsMachineFact: humanFallback == nil && machineFallback != nil
    )
}

private func shellSidebarTaskTitle(for activity: TerminalActivitySnapshot) -> String? {
    guard let detail = shellVisibleLabel(activity.display.detailLabel) else { return nil }
    let lowercased = detail.lowercased()
    guard !["running", "thinking", "working", "done", "error", "input needed"].contains(lowercased),
          !lowercased.hasPrefix("session "),
          !lowercased.contains("session_id")
    else {
        return nil
    }
    return detail
}

private func shellSidebarActivitySubtitle(
    for activity: TerminalActivitySnapshot,
    contextLine: String?,
    hasTaskTitle: Bool
) -> String? {
    let stateLabel = shellVisibleLabel(activity.display.stateLabel)
    let sourceLabel = shellVisibleLabel(activity.display.sourceLabel)
    let paneHint = shellVisibleLabel(activity.display.paneHint)
    let stableContext = shellVisibleLabel(contextLine)

    switch activity.status {
    case .needsInput, .failed, .paused, .exited:
        return [stateLabel, paneHint, sourceLabel, stableContext]
            .compactMap { $0 }
            .removingAdjacentDuplicates()
            .joined(separator: " · ")
    case .progress, .running:
        if hasTaskTitle {
            return [paneHint, stableContext, sourceLabel]
                .compactMap { $0 }
                .removingAdjacentDuplicates()
                .joined(separator: " · ")
        }
        return activity.display.sourceFirstLabel
    case .bell:
        return [stateLabel, stableContext].compactMap { $0 }.joined(separator: " · ")
    case .done, .idle, .stale:
        return hasTaskTitle ? stableContext : nil
    }
}

private func shellSidebarStateAccessory(
    for activity: TerminalActivitySnapshot
) -> ShellSidebarTabStateAccessory? {
    switch activity.status {
    case .needsInput:
        return ShellSidebarTabStateAccessory(systemImageName: "questionmark.circle.fill", accessibilityLabel: "Input needed")
    case .failed:
        return ShellSidebarTabStateAccessory(systemImageName: "exclamationmark.triangle.fill", accessibilityLabel: "Error")
    case .paused:
        return ShellSidebarTabStateAccessory(systemImageName: "pause.circle.fill", accessibilityLabel: "Paused")
    case .exited:
        return ShellSidebarTabStateAccessory(systemImageName: "xmark.circle.fill", accessibilityLabel: "Exited")
    case .progress, .running:
        return ShellSidebarTabStateAccessory(systemImageName: "circle.dotted", accessibilityLabel: activity.display.stateLabel)
    case .bell:
        return ShellSidebarTabStateAccessory(systemImageName: "bell.fill", accessibilityLabel: activity.display.stateLabel)
    case .done, .idle, .stale:
        return nil
    }
}

private func meaningfulSidebarContextLine(_ contextLine: String?, title: String) -> String? {
    guard let contextLine = shellVisibleLabel(contextLine),
          !shellLabelsMatch(contextLine, title),
          !["terminal", "shell", "sh", "zsh", "bash", "fish"].contains(contextLine.lowercased())
    else {
        return nil
    }
    return contextLine
}

private func meaningfulSidebarFallbackLine(_ line: String?, title: String) -> String? {
    guard let line = shellVisibleLabel(line),
          !shellLabelsMatch(line, title),
          !["terminal", "shell"].contains(line.lowercased())
    else {
        return nil
    }
    return line
}

private extension Array where Element == String {
    func removingAdjacentDuplicates() -> [String] {
        reduce(into: [String]()) { result, value in
            if result.last?.caseInsensitiveCompare(value) != .orderedSame {
                result.append(value)
            }
        }
    }
}

func shellOrderedPanes(for tab: ShellTab, panes allPanes: [ShellPane]) -> [ShellPane] {
    let byID = Dictionary(uniqueKeysWithValues: allPanes.map { ($0.paneID, $0) })
    let ordered = tab.paneTree.paneIDs.compactMap { byID[$0] }
    if !ordered.isEmpty {
        return ordered
    }
    return allPanes.filter { $0.tabID == tab.tabID }
}

private func shellPrimaryPane(in panes: [ShellPane], focusedPaneID: String?) -> ShellPane? {
    if let focusedPaneID,
       let focused = panes.first(where: { $0.paneID == focusedPaneID })
    {
        return focused
    }
    return panes.first
}

private func shellPrimaryContent(
    in tab: ShellTab,
    contentState: ShellContentStateSnapshot?,
    focusedPaneID: String?
) -> ShellContentInstance? {
    guard let contentState else { return nil }
    if let focusedPaneID,
       tab.contains(paneID: focusedPaneID),
       let focusedContent = contentState.contentMounted(in: focusedPaneID)
    {
        return focusedContent
    }

    return tab.paneTree.paneIDs.lazy.compactMap {
        contentState.contentMounted(in: $0)
    }.first
}

private func shellSidebarTabTitle(
    for tab: ShellTab,
    primaryPane: ShellPane?,
    primaryContent: ShellContentInstance?
) -> String {
    if let primaryContent,
       primaryContent.kind != .terminal
    {
        return primaryContent.title
    }

    return shellDisplayTitle(
        rawTitle: tab.title ?? primaryPane?.viewport?.title,
        workingDirectoryName: primaryPane?.context?.workingDirectoryName,
        cwd: primaryPane?.cwd,
        program: primaryPane?.process?.program,
        launchTarget: primaryPane?.resolvedLaunchTarget ?? .shell,
        fallback: shellFallbackTitle(for: tab.kind)
    )
}

private func shellSidebarContentLine(for content: ShellContentInstance?) -> String? {
    guard let content,
          content.kind != .terminal
    else {
        return nil
    }

    return shellContentTypeHint(for: content.kind)
}

private func shellSidebarContextLine(for pane: ShellPane, title: String) -> String? {
    let contextLabel = shellPathLeaf(pane.context?.repositoryRoot)
        ?? shellVisibleLabel(pane.context?.workingDirectoryName)
        ?? shellPathLeaf(pane.cwd)

    if let branch = shellVisibleLabel(pane.context?.gitBranch) {
        if let contextLabel, contextLabel != title {
            return "\(contextLabel) · \(branch)"
        }
        return branch
    }

    if let contextLabel {
        if contextLabel == title,
           let program = shellVisibleLabel(pane.process?.program)
        {
            return program
        }
        return contextLabel
    }

    return shellVisibleLabel(pane.process?.program)
}
