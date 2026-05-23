import Foundation

enum ShellStateMutationError: String, Error {
    case spaceNotFound = "space_not_found"
    case tabNotFound = "tab_not_found"
    case paneNotFound = "pane_not_found"
    case splitNotFound = "split_not_found"
    case spatialFocusTargetNotFound = "spatial_focus_target_not_found"
    case lastTab = "last_tab"
    case lastPane = "last_pane"
    case invalidMoveTarget = "invalid_move_target"
    case invalidTabOrganizationTarget = "invalid_tab_organization_target"
}

struct ShellStateMutationResult {
    let state: ShellStateSnapshot
    let spaceID: String?
    let tabID: String?
    let paneID: String?
}

private struct ShellPreparedContentMount {
    let pane: ShellPane
    let paneSlot: ShellPaneSlot?
    let content: ShellContentInstance?
    let title: String
}

extension ShellStateSnapshot {
    private static func defaultShellWorkingDirectory() -> String {
        FileManager.default.homeDirectoryForCurrentUser.path
    }

    static func bootstrapDefault(
        windowID: String = "window_main",
        workingDirectory: String = defaultShellWorkingDirectory()
    ) -> ShellStateSnapshot {
        let spaceID = "space_main"
        let tabID = "tab_main"
        let paneID = "pane_1"

        return ShellStateSnapshot(
            contractVersion: "0.1",
            windowID: windowID,
            focusedSpaceID: spaceID,
            focusedTabID: tabID,
            focusedPaneID: paneID,
            spaces: [
                ShellSpace(
                    spaceID: spaceID,
                    title: "Terminal",
                    attention: .active,
                    tabs: [
                        ShellTab(
                            tabID: tabID,
                            kind: .terminal,
                            title: "Shell",
                            paneTree: ShellPaneTreeNode(
                                nodeID: "node_\(paneID)",
                                kind: .pane,
                                direction: nil,
                                paneID: paneID,
                                children: nil
                            )
                        )
                    ]
                )
            ],
            panes: [
                ShellPane(
                    paneID: paneID,
                    tabID: tabID,
                    spaceID: spaceID,
                    launchTarget: .shell,
                    cwd: workingDirectory,
                    process: Self.defaultProcessBinding(for: .shell),
                    attention: .active,
                    context: nil,
                    viewport: ShellViewportSnapshot(
                        title: Self.defaultViewportTitle(for: .shell),
                        summary: "ready to launch login shell",
                        visibleExcerpt: nil,
                        lastActivityAt: nil
                    ),
                    alanBinding: nil
                )
            ]
        )
    }


    func focusingPane(_ paneID: String) throws -> ShellStateMutationResult {
        guard let targetPane = pane(paneID: paneID) else {
            throw ShellStateMutationError.paneNotFound
        }

        let acknowledgedPanes = panesAcknowledgingCommandFailureActivities(
            in: targetPane.tabID,
            focusedPaneID: paneID
        )
        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: spaces, panes: acknowledgedPanes),
                panes: acknowledgedPanes,
                focusedPaneID: paneID
            ),
            spaceID: targetPane.spaceID,
            tabID: targetPane.tabID,
            paneID: paneID
        )
    }

    private func panesAcknowledgingCommandFailureActivities(
        in tabID: String,
        focusedPaneID: String
    ) -> [ShellPane] {
        panes.map { current in
            guard current.tabID == tabID,
                  current.activity?.isCommandFailure == true
            else { return current }

            let acknowledgedAttention: ShellAttentionState
            if current.attention == .notable {
                acknowledgedAttention = current.paneID == focusedPaneID ? .active : .idle
            } else {
                acknowledgedAttention = current.attention
            }

            return ShellPane(
                paneID: current.paneID,
                tabID: current.tabID,
                spaceID: current.spaceID,
                launchTarget: current.launchTarget,
                cwd: current.cwd,
                process: current.process,
                attention: acknowledgedAttention,
                context: current.context,
                viewport: current.viewport,
                activity: nil,
                alanBinding: current.alanBinding
            )
        }
    }

    func focusingAdjacentPane(_ direction: ShellSpatialFocusDirection) throws -> ShellStateMutationResult {
        guard let focusedPaneID,
              let focusedPane = pane(paneID: focusedPaneID),
              let focusedTab = tab(tabID: focusedPane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }

        guard let targetPaneID = focusedTab.paneTree.adjacentPaneID(
            from: focusedPaneID,
            direction: direction
        ) else {
            throw ShellStateMutationError.spatialFocusTargetNotFound
        }

        return try focusingPane(targetPaneID)
    }

    func applyingAgentActivity(
        _ activity: TerminalActivitySnapshot,
        to paneID: String,
        workingDirectory: String?
    ) throws -> ShellStateMutationResult {
        guard let targetPane = pane(paneID: paneID) else {
            throw ShellStateMutationError.paneNotFound
        }

        let updatedPanes = panes.map { current in
            guard current.paneID == paneID else { return current }
            return ShellPane(
                paneID: current.paneID,
                tabID: current.tabID,
                spaceID: current.spaceID,
                launchTarget: current.launchTarget,
                cwd: workingDirectory ?? current.cwd,
                process: current.process,
                attention: current.attention,
                context: current.context,
                viewport: current.viewport,
                activity: activity,
                alanBinding: current.alanBinding
            )
        }
        let nextSpaces = rebuildingAttention(in: spaces, panes: updatedPanes)
        return ShellStateMutationResult(
            state: replacing(
                spaces: nextSpaces,
                panes: updatedPanes,
                focusedPaneID: focusedPaneID
            ),
            spaceID: targetPane.spaceID,
            tabID: targetPane.tabID,
            paneID: targetPane.paneID
        )
    }

    func creatingSpace(
        launchTarget: ShellLaunchTarget,
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) -> ShellStateMutationResult {
        let spaceIndex = spaces.count + 1
        let spaceID = nextID(prefix: "space", existing: spaces.map(\.spaceID))
        let tabID = nextID(prefix: "tab", existing: spaces.flatMap { $0.tabs.map(\.tabID) })
        let paneID = nextID(prefix: "pane", existing: panes.map(\.paneID) + Array(reservedPaneIDs))
        let pane = makeTerminalPane(
            paneID: paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: launchTarget,
            workingDirectory: workingDirectory ?? defaultWorkingDirectory,
            summary: launchTarget == .alan ? "new alan space scaffolded" : "new shell space scaffolded",
            now: now
        )
        let tab = ShellTab(
            tabID: tabID,
            kind: .terminal,
            title: launchTarget == .alan ? "alan" : "Shell",
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            )
        )
        let space = ShellSpace(
            spaceID: spaceID,
            title: title ?? (launchTarget == .alan ? "alan space \(spaceIndex)" : "Space \(spaceIndex)"),
            attention: .active,
            tabs: [tab]
        )
        let nextPanes = panes + [pane]
        let nextSpaces = rebuildingAttention(in: spaces + [space], panes: nextPanes)

        return ShellStateMutationResult(
            state: replacing(
                spaces: nextSpaces,
                panes: nextPanes,
                focusedPaneID: paneID
            ),
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID
        )
    }

    func creatingAlanSpace(
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) -> ShellStateMutationResult {
        creatingSpace(
            launchTarget: .alan,
            title: title,
            workingDirectory: workingDirectory,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func creatingTerminalSpace(
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) -> ShellStateMutationResult {
        creatingSpace(
            launchTarget: .shell,
            title: title,
            workingDirectory: workingDirectory,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func deletingSpace(
        _ spaceID: String,
        defaultWorkingDirectory: String = defaultShellWorkingDirectory()
    ) throws -> ShellStateMutationResult {
        guard let targetSpace = space(spaceID: spaceID) else {
            throw ShellStateMutationError.spaceNotFound
        }

        let removedPaneIDs = Set(targetSpace.tabs.flatMap(\.paneTree.paneIDs))
        let remainingSpaces = spaces.filter { $0.spaceID != spaceID }
        let remainingPanes = panes.filter { !removedPaneIDs.contains($0.paneID) }

        guard !remainingSpaces.isEmpty else {
            let defaultState = ShellStateSnapshot.bootstrapDefault(
                windowID: windowID,
                workingDirectory: defaultWorkingDirectory
            )
            return ShellStateMutationResult(
                state: defaultState,
                spaceID: defaultState.focusedSpaceID,
                tabID: defaultState.focusedTabID,
                paneID: defaultState.focusedPaneID
            )
        }

        let focusedPaneID = remainingPanes.first?.paneID
        let focusedPane = focusedPaneID.flatMap { paneID in
            remainingPanes.first { $0.paneID == paneID }
        }
        let retained = retainedContentRecords(in: remainingSpaces)
        let nextState = ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedPane?.spaceID ?? remainingSpaces.first?.spaceID,
            focusedTabID: focusedPane?.tabID,
            focusedPaneID: focusedPaneID,
            spaces: rebuildingAttention(in: remainingSpaces, panes: remainingPanes),
            panes: remainingPanes,
            paneSlots: retained.paneSlots,
            contents: retained.contents,
            quickTerminal: quickTerminal
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: nextState.focusedSpaceID,
            tabID: nextState.focusedTabID,
            paneID: nextState.focusedPaneID
        )
    }

    func openingContentTab(
        _ contentIntent: ShellContentIntent,
        in requestedSpaceID: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        if case .settings = contentIntent,
           let existingSettingsResult = try focusingExistingSettingsContent()
        {
            return existingSettingsResult
        }

        let targetSpaceID = requestedSpaceID ?? focusedSpaceID ?? spaces.first?.spaceID
        guard let targetSpaceID,
              let targetSpace = space(spaceID: targetSpaceID)
        else {
            throw ShellStateMutationError.spaceNotFound
        }

        let tabID = nextID(prefix: "tab", existing: spaces.flatMap { $0.tabs.map(\.tabID) })
        let paneID = nextID(prefix: "pane", existing: panes.map(\.paneID) + Array(reservedPaneIDs))
        let prepared = makeContentMount(
            contentIntent,
            paneID: paneID,
            tabID: tabID,
            spaceID: targetSpaceID,
            defaultTerminalTitle: Self.defaultTabTitle(
                for: contentIntent,
                existingTabCount: targetSpace.tabs.count
            ),
            terminalSummary: Self.defaultTerminalSummary(for: contentIntent),
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
        let tab = ShellTab(
            tabID: tabID,
            kind: .terminal,
            title: prepared.title,
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            )
        )
        let nextSpaces = spaces.map { space in
            guard space.spaceID == targetSpaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs + [tab]
            )
        }
        let nextPanes = panes + [prepared.pane]

        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
                panes: nextPanes,
                focusedPaneID: paneID,
                additionalPaneSlots: [prepared.paneSlot].compactMap { $0 },
                additionalContents: [prepared.content].compactMap { $0 }
            ),
            spaceID: targetSpaceID,
            tabID: tabID,
            paneID: paneID
        )
    }

    func openingTab(
        launchTarget: ShellLaunchTarget,
        in requestedSpaceID: String?,
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try openingContentTab(
            .terminal(
                launchTarget: launchTarget,
                title: title,
                workingDirectory: workingDirectory
            ),
            in: requestedSpaceID,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func openingAlanTab(
        in requestedSpaceID: String?,
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try openingTab(
            launchTarget: .alan,
            in: requestedSpaceID,
            title: title,
            workingDirectory: workingDirectory,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func openingTerminalTab(
        in requestedSpaceID: String?,
        title: String?,
        workingDirectory: String?,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try openingTab(
            launchTarget: .shell,
            in: requestedSpaceID,
            title: title,
            workingDirectory: workingDirectory,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func openingMarkdownTab(
        fileURL: URL,
        in requestedSpaceID: String?,
        title: String?,
        reservedPaneIDs: Set<String> = [],
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try openingContentTab(
            .markdown(fileURL: fileURL, title: title),
            in: requestedSpaceID,
            reservedPaneIDs: reservedPaneIDs,
            now: now
        )
    }

    func openingSettingsTab(
        in requestedSpaceID: String?,
        title: String?,
        reservedPaneIDs: Set<String> = [],
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try openingContentTab(
            .settings(title: title),
            in: requestedSpaceID,
            reservedPaneIDs: reservedPaneIDs,
            now: now
        )
    }

    func showingQuickTerminal(
        workingDirectory: String?,
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) -> ShellStateMutationResult {
        let paneID = quickTerminal?.paneID ?? ShellQuickTerminalSlot.globalPaneID
        let pane = pane(paneID: paneID)
        let resolvedWorkingDirectory =
            pane?.cwd
            ?? workingDirectory
            ?? quickTerminal?.lastWorkingDirectory
            ?? defaultWorkingDirectory
        let nextPane = pane
            ?? makeTerminalPane(
                paneID: paneID,
                tabID: ShellQuickTerminalSlot.globalTabID,
                spaceID: ShellQuickTerminalSlot.globalSpaceID,
                launchTarget: .shell,
                workingDirectory: resolvedWorkingDirectory,
                summary: "quick terminal scaffolded",
                now: now
            )
        let nextPanes = pane == nil ? panes + [nextPane] : panes
        let nextQuickTerminal = ShellQuickTerminalSlot(
            paneID: paneID,
            presentation: .visible,
            lastWorkingDirectory: resolvedWorkingDirectory
        )
        let retained = retainedContentRecords(in: spaces)

        return ShellStateMutationResult(
            state: ShellStateSnapshot(
                contractVersion: contractVersion,
                windowID: windowID,
                focusedSpaceID: focusedSpaceID,
                focusedTabID: focusedTabID,
                focusedPaneID: focusedPaneID,
                spaces: spaces,
                panes: nextPanes,
                paneSlots: retained.paneSlots,
                contents: retained.contents,
                quickTerminal: nextQuickTerminal
            ),
            spaceID: focusedSpaceID,
            tabID: focusedTabID,
            paneID: paneID
        )
    }

    func hidingQuickTerminal() throws -> ShellStateMutationResult {
        guard let quickTerminal,
              pane(paneID: quickTerminal.paneID) != nil
        else {
            throw ShellStateMutationError.paneNotFound
        }

        let retained = retainedContentRecords(in: spaces)
        return ShellStateMutationResult(
            state: ShellStateSnapshot(
                contractVersion: contractVersion,
                windowID: windowID,
                focusedSpaceID: focusedSpaceID,
                focusedTabID: focusedTabID,
                focusedPaneID: focusedPaneID,
                spaces: spaces,
                panes: panes,
                paneSlots: retained.paneSlots,
                contents: retained.contents,
                quickTerminal: ShellQuickTerminalSlot(
                    paneID: quickTerminal.paneID,
                    presentation: .hidden,
                    lastWorkingDirectory: quickTerminal.lastWorkingDirectory
                )
            ),
            spaceID: focusedSpaceID,
            tabID: focusedTabID,
            paneID: quickTerminal.paneID
        )
    }

    func closingQuickTerminal() throws -> ShellStateMutationResult {
        guard let quickTerminal,
              pane(paneID: quickTerminal.paneID) != nil
        else {
            throw ShellStateMutationError.paneNotFound
        }
        let nextPanes = panes.filter { $0.paneID != quickTerminal.paneID }
        let nextFocusedPaneID =
            focusedPaneID == quickTerminal.paneID
            ? nextPanes.first(where: { !$0.isQuickTerminalPane })?.paneID
            : focusedPaneID
        let retained = retainedContentRecords(in: spaces)

        return ShellStateMutationResult(
            state: ShellStateSnapshot(
                contractVersion: contractVersion,
                windowID: windowID,
                focusedSpaceID: nextFocusedPaneID.flatMap { pane(paneID: $0)?.spaceID } ?? focusedSpaceID,
                focusedTabID: nextFocusedPaneID.flatMap { pane(paneID: $0)?.tabID } ?? focusedTabID,
                focusedPaneID: nextFocusedPaneID,
                spaces: spaces,
                panes: nextPanes,
                paneSlots: retained.paneSlots,
                contents: retained.contents,
                quickTerminal: nil
            ),
            spaceID: focusedSpaceID,
            tabID: focusedTabID,
            paneID: nextFocusedPaneID
        )
    }

    func promotingQuickTerminal(
        to targetSpaceID: String,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        guard let quickTerminal,
              let quickPane = pane(paneID: quickTerminal.paneID)
        else {
            throw ShellStateMutationError.paneNotFound
        }
        guard let targetSpace = space(spaceID: targetSpaceID) else {
            throw ShellStateMutationError.spaceNotFound
        }

        let newTabID = nextID(prefix: "tab", existing: spaces.flatMap { $0.tabs.map(\.tabID) })
        let movedPaneNode = ShellPaneTreeNode(
            nodeID: "node_\(quickPane.paneID)",
            kind: .pane,
            direction: nil,
            paneID: quickPane.paneID,
            children: nil
        )
        let newTab = ShellTab(
            tabID: newTabID,
            kind: .terminal,
            title: quickPane.viewport?.title ?? "Quick Terminal",
            paneTree: movedPaneNode
        )
        let nextSpaces = spaces.map { space in
            guard space.spaceID == targetSpace.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs + [newTab]
            )
        }

        let formatter = ISO8601DateFormatter()
        let nextPanes = panes.map { current in
            guard current.paneID == quickPane.paneID else { return current }
            return ShellPane(
                paneID: current.paneID,
                tabID: newTabID,
                spaceID: targetSpace.spaceID,
                launchTarget: current.launchTarget,
                cwd: current.cwd,
                process: current.process,
                attention: current.attention,
                context: current.context,
                viewport: ShellViewportSnapshot(
                    title: current.viewport?.title,
                    summary: current.viewport?.summary ?? "quick terminal opened in space",
                    visibleExcerpt: current.viewport?.visibleExcerpt,
                    lastActivityAt: formatter.string(from: now)
                ),
                activity: current.activity,
                alanBinding: current.alanBinding
            )
        }

        let retained = retainedContentRecords(in: nextSpaces)
        let nextState = ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: targetSpace.spaceID,
            focusedTabID: newTabID,
            focusedPaneID: quickPane.paneID,
            spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
            panes: nextPanes,
            paneSlots: retained.paneSlots,
            contents: retained.contents,
            quickTerminal: nil
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: targetSpace.spaceID,
            tabID: newTabID,
            paneID: quickPane.paneID
        )
    }

    func splittingPane(
        _ paneID: String,
        direction: ShellSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try splittingPane(
            paneID,
            placement: .defaultPlacement(for: direction),
            contentIntent: contentIntent,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func splittingPane(
        _ paneID: String,
        placement: ShellPaneSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        guard let pane = pane(paneID: paneID),
              let tab = tab(tabID: pane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }
        if case .some(.settings) = contentIntent,
           let existingSettingsResult = try focusingExistingSettingsContent()
        {
            return existingSettingsResult
        }

        let newPaneID = nextID(prefix: "pane", existing: panes.map(\.paneID) + Array(reservedPaneIDs))
        let splitNodeID = nextID(
            prefix: "node",
            existing: spaces.flatMap(\.tabs).flatMap { $0.paneTree.nodeIDs }
        )
        let resolvedContentIntent =
            contentIntent
            ?? .terminal(
                launchTarget: pane.resolvedLaunchTarget,
                title: nil,
                workingDirectory: pane.cwd
            )
        let prepared = makeContentMount(
            resolvedContentIntent,
            paneID: newPaneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            defaultTerminalTitle: Self.defaultViewportTitle(for: pane.resolvedLaunchTarget),
            terminalSummary: "new split scaffolded",
            defaultWorkingDirectory: pane.cwd ?? defaultWorkingDirectory,
            now: now
        )
        let updatedTab = ShellTab(
            tabID: tab.tabID,
            kind: tab.kind,
            title: tab.title,
            paneTree: tab.paneTree.splittingPane(
                paneID,
                placement: placement,
                splitNodeID: splitNodeID,
                newLeafNodeID: "node_\(newPaneID)",
                newPaneID: newPaneID
            ),
            isPinned: tab.isPinned
        )

        let nextSpaces = spaces.map { space in
            guard space.spaceID == pane.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs.map { existingTab in
                    existingTab.tabID == updatedTab.tabID ? updatedTab : existingTab
                }
            )
        }
        let nextPanes = panes + [prepared.pane]

        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
                panes: nextPanes,
                focusedPaneID: newPaneID,
                additionalPaneSlots: [prepared.paneSlot].compactMap { $0 },
                additionalContents: [prepared.content].compactMap { $0 }
            ),
            spaceID: pane.spaceID,
            tabID: pane.tabID,
            paneID: newPaneID
        )
    }

    func resizingSplit(
        _ splitNodeID: String,
        ratio: Double
    ) throws -> ShellStateMutationResult {
        guard let tab = spaces.lazy
            .flatMap(\.tabs)
            .first(where: { $0.paneTree.contains(nodeID: splitNodeID) })
        else {
            throw ShellStateMutationError.splitNotFound
        }

        let resizeResult = tab.paneTree.resizingSplit(splitNodeID, ratio: ratio)
        guard resizeResult.changed else {
            throw ShellStateMutationError.splitNotFound
        }

        return try replacingTabTree(
            tabID: tab.tabID,
            paneTree: resizeResult.node
        )
    }

    func equalizingSplits(in requestedTabID: String?) throws -> ShellStateMutationResult {
        let tabID = requestedTabID ?? focusedTabID
        guard let tabID,
              let tab = tab(tabID: tabID)
        else {
            throw ShellStateMutationError.tabNotFound
        }

        return try replacingTabTree(
            tabID: tab.tabID,
            paneTree: tab.paneTree.equalizedSplits()
        )
    }

    func closingPane(_ paneID: String) throws -> ShellStateMutationResult {
        guard let pane = pane(paneID: paneID),
              let tab = tab(tabID: pane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }

        if tab.paneTree.paneIDs.count == 1 {
            return try closingTab(tab.tabID)
        }

        guard let updatedPaneTree = tab.paneTree.removingPane(paneID) else {
            throw ShellStateMutationError.paneNotFound
        }

        let updatedTab = ShellTab(
            tabID: tab.tabID,
            kind: tab.kind,
            title: tab.title,
            paneTree: updatedPaneTree,
            isPinned: tab.isPinned
        )
        let nextSpaces = spaces.map { space in
            guard space.spaceID == pane.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs.map { existingTab in
                    existingTab.tabID == updatedTab.tabID ? updatedTab : existingTab
                }
            )
        }
        let nextPanes = panes.filter { $0.paneID != paneID }
        let preferredPaneID =
            focusedPaneID == paneID
            ? (updatedTab.paneTree.paneIDs.first
                ?? nextPanes.first(where: { $0.spaceID == pane.spaceID })?.paneID
                ?? nextPanes.first?.paneID)
            : focusedPaneID

        let nextState = replacing(
            spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
            panes: nextPanes,
            focusedPaneID: preferredPaneID
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: pane.spaceID,
            tabID: pane.tabID,
            paneID: nextState.focusedPaneID
        )
    }

    func movingPaneToNewTab(
        _ paneID: String,
        title: String?,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        guard let pane = pane(paneID: paneID),
              let sourceTab = tab(tabID: pane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }

        guard sourceTab.paneTree.paneIDs.count > 1 else {
            throw ShellStateMutationError.lastPane
        }

        guard let sourcePaneTree = sourceTab.paneTree.removingPane(paneID) else {
            throw ShellStateMutationError.paneNotFound
        }

        let newTabID = nextID(
            prefix: "tab",
            existing: spaces.flatMap { $0.tabs.map(\.tabID) }
        )
        let movedPaneNode = ShellPaneTreeNode(
            nodeID: "node_\(paneID)",
            kind: .pane,
            direction: nil,
            paneID: paneID,
            children: nil
        )
        let updatedSourceTab = ShellTab(
            tabID: sourceTab.tabID,
            kind: sourceTab.kind,
            title: sourceTab.title,
            paneTree: sourcePaneTree,
            isPinned: sourceTab.isPinned
        )
        let newTab = ShellTab(
            tabID: newTabID,
            kind: sourceTab.kind,
            title: title ?? pane.viewport?.title ?? "Lifted Pane",
            paneTree: movedPaneNode
        )

        let nextSpaces = spaces.map { space in
            guard space.spaceID == pane.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs.flatMap { existingTab -> [ShellTab] in
                    guard existingTab.tabID == sourceTab.tabID else {
                        return [existingTab]
                    }
                    return [updatedSourceTab, newTab]
                }
            )
        }
        let formatter = ISO8601DateFormatter()
        let nextPanes = panes.map { current in
            guard current.paneID == paneID else { return current }
            return ShellPane(
                paneID: current.paneID,
                tabID: newTabID,
                spaceID: current.spaceID,
                launchTarget: current.launchTarget,
                cwd: current.cwd,
                process: current.process,
                attention: current.attention,
                context: current.context,
                viewport: ShellViewportSnapshot(
                    title: current.viewport?.title,
                    summary: current.viewport?.summary ?? "pane moved to its own tab",
                    visibleExcerpt: current.viewport?.visibleExcerpt,
                    lastActivityAt: formatter.string(from: now)
                ),
                activity: current.activity,
                alanBinding: current.alanBinding
            )
        }

        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
                panes: nextPanes,
                focusedPaneID: paneID
            ),
            spaceID: pane.spaceID,
            tabID: newTabID,
            paneID: paneID
        )
    }

    func movingPane(
        _ paneID: String,
        toTab targetTabID: String,
        direction: ShellSplitDirection,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        guard let pane = pane(paneID: paneID),
              let sourceTab = tab(tabID: pane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }

        guard let targetTab = tab(tabID: targetTabID) else {
            throw ShellStateMutationError.tabNotFound
        }

        guard sourceTab.tabID != targetTab.tabID else {
            throw ShellStateMutationError.invalidMoveTarget
        }

        let targetSpaceID = spaces.first(where: { space in
            space.tabs.contains(where: { $0.tabID == targetTabID })
        })?.spaceID

        guard let targetSpaceID else {
            throw ShellStateMutationError.tabNotFound
        }

        let formatter = ISO8601DateFormatter()
        let moveSummary = "pane moved to \(targetTab.title ?? targetTab.tabID)"
        let newSplitNodeID = nextID(
            prefix: "node",
            existing: spaces.flatMap { $0.tabs.flatMap { $0.paneTree.nodeIDs } }
        )
        let newLeafNodeID = "node_\(paneID)_moved"

        let updatedTargetTab = ShellTab(
            tabID: targetTab.tabID,
            kind: targetTab.kind,
            title: targetTab.title,
            paneTree: targetTab.paneTree.attachingPane(
                paneID,
                direction: direction,
                splitNodeID: newSplitNodeID,
                newLeafNodeID: newLeafNodeID
            ),
            isPinned: targetTab.isPinned
        )

        let updatedSourcePaneTree = sourceTab.paneTree.removingPane(paneID)

        let nextSpaces = spaces.compactMap { space -> ShellSpace? in
            var nextTabs: [ShellTab] = []
            for tab in space.tabs {
                if tab.tabID == sourceTab.tabID {
                    if let updatedSourcePaneTree {
                        nextTabs.append(
                            ShellTab(
                                tabID: sourceTab.tabID,
                                kind: sourceTab.kind,
                                title: sourceTab.title,
                                paneTree: updatedSourcePaneTree,
                                isPinned: sourceTab.isPinned
                            )
                        )
                    }
                    continue
                }

                if tab.tabID == updatedTargetTab.tabID {
                    nextTabs.append(updatedTargetTab)
                } else {
                    nextTabs.append(tab)
                }
            }

            guard !nextTabs.isEmpty else { return nil }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: nextTabs
            )
        }

        let nextPanes = panes.map { current in
            guard current.paneID == paneID else { return current }
            return ShellPane(
                paneID: current.paneID,
                tabID: updatedTargetTab.tabID,
                spaceID: targetSpaceID,
                launchTarget: current.launchTarget,
                cwd: current.cwd,
                process: current.process,
                attention: current.attention,
                context: current.context,
                viewport: ShellViewportSnapshot(
                    title: current.viewport?.title,
                    summary: current.viewport?.summary ?? moveSummary,
                    visibleExcerpt: current.viewport?.visibleExcerpt,
                    lastActivityAt: formatter.string(from: now)
                ),
                activity: current.activity,
                alanBinding: current.alanBinding
            )
        }

        let nextState = replacing(
            spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
            panes: nextPanes,
            focusedPaneID: paneID
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: targetSpaceID,
            tabID: updatedTargetTab.tabID,
            paneID: paneID
        )
    }

    func movingPaneWithinTab(
        _ paneID: String,
        placement: ShellPaneSplitDirection
    ) throws -> ShellStateMutationResult {
        guard let pane = pane(paneID: paneID),
              let tab = tab(tabID: pane.tabID)
        else {
            throw ShellStateMutationError.paneNotFound
        }

        guard tab.paneTree.paneIDs.count > 1 else {
            throw ShellStateMutationError.invalidMoveTarget
        }

        guard let targetPaneID = tab.paneTree.adjacentPaneID(
            from: paneID,
            direction: placement.spatialFocusDirection
        ),
        targetPaneID != paneID
        else {
            throw ShellStateMutationError.invalidMoveTarget
        }

        guard let treeWithoutMovedPane = tab.paneTree.removingPane(paneID),
              treeWithoutMovedPane.paneIDs.contains(targetPaneID)
        else {
            throw ShellStateMutationError.invalidMoveTarget
        }

        let newSplitNodeID = nextID(
            prefix: "node",
            existing: spaces.flatMap { $0.tabs.flatMap { $0.paneTree.nodeIDs } }
        )
        let movedLeafNodeID = "node_\(paneID)_moved_in_tab"
        let movedTree = treeWithoutMovedPane.splittingPane(
            targetPaneID,
            placement: placement,
            splitNodeID: newSplitNodeID,
            newLeafNodeID: movedLeafNodeID,
            newPaneID: paneID
        )
        let updatedTab = ShellTab(
            tabID: tab.tabID,
            kind: tab.kind,
            title: tab.title,
            paneTree: movedTree,
            isPinned: tab.isPinned
        )
        let nextSpaces = spaces.map { space in
            guard space.spaceID == pane.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs.map { existingTab in
                    existingTab.tabID == updatedTab.tabID ? updatedTab : existingTab
                }
            )
        }

        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: nextSpaces, panes: panes),
                panes: panes,
                focusedPaneID: paneID
            ),
            spaceID: pane.spaceID,
            tabID: pane.tabID,
            paneID: paneID
        )
    }

    func organizingTab(
        tabID: String,
        targetSpaceID requestedTargetSpaceID: String? = nil,
        section targetSection: ShellTabOrganizationSection,
        index requestedIndex: Int? = nil
    ) throws -> ShellStateMutationResult {
        guard let sourceSpaceIndex = spaces.firstIndex(where: { space in
            space.tabs.contains(where: { $0.tabID == tabID })
        }) else {
            throw ShellStateMutationError.tabNotFound
        }

        let sourceSpace = spaces[sourceSpaceIndex]
        guard let sourceTabIndex = sourceSpace.tabs.firstIndex(where: { $0.tabID == tabID }) else {
            throw ShellStateMutationError.tabNotFound
        }

        let targetSpaceID = requestedTargetSpaceID ?? sourceSpace.spaceID
        guard let targetSpaceIndex = spaces.firstIndex(where: { $0.spaceID == targetSpaceID }) else {
            throw ShellStateMutationError.spaceNotFound
        }

        let sourceTab = sourceSpace.tabs[sourceTabIndex]
        let updatedTab = ShellTab(
            tabID: sourceTab.tabID,
            kind: sourceTab.kind,
            title: sourceTab.title,
            paneTree: sourceTab.paneTree,
            isPinned: targetSection == .pinned
        )

        var nextSpaces = spaces
        nextSpaces[sourceSpaceIndex] = ShellSpace(
            spaceID: sourceSpace.spaceID,
            title: sourceSpace.title,
            attention: sourceSpace.attention,
            tabs: sourceSpace.tabs.filter { $0.tabID != tabID }
        )

        let targetSpaceAfterRemoval = nextSpaces[targetSpaceIndex]
        let targetSectionTabs = targetSpaceAfterRemoval.tabs(in: targetSection)
        let insertionIndex = requestedIndex ?? targetSectionTabs.count
        guard (0...targetSectionTabs.count).contains(insertionIndex) else {
            throw ShellStateMutationError.invalidTabOrganizationTarget
        }

        let insertionOffset = targetSection == .pinned
            ? insertionIndex
            : targetSpaceAfterRemoval.pinnedTabs.count + insertionIndex
        var targetTabs = targetSpaceAfterRemoval.tabs
        targetTabs.insert(updatedTab, at: insertionOffset)
        nextSpaces[targetSpaceIndex] = ShellSpace(
            spaceID: targetSpaceAfterRemoval.spaceID,
            title: targetSpaceAfterRemoval.title,
            attention: targetSpaceAfterRemoval.attention,
            tabs: targetTabs
        )

        let nextPanes = panes.map { pane in
            guard pane.tabID == tabID,
                  pane.spaceID != targetSpaceID
            else { return pane }

            return ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: targetSpaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.cwd,
                process: pane.process,
                attention: pane.attention,
                context: pane.context,
                viewport: pane.viewport,
                activity: pane.activity,
                alanBinding: pane.alanBinding
            )
        }

        let nextFocusedPaneID: String?
        if focusedTabID == tabID {
            nextFocusedPaneID = focusedPaneID.flatMap { paneID in
                nextPanes.contains { $0.paneID == paneID && $0.tabID == tabID } ? paneID : nil
            } ?? updatedTab.paneTree.paneIDs.first
        } else {
            nextFocusedPaneID = focusedPaneID
        }

        let nextState = replacing(
            spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
            panes: nextPanes,
            focusedPaneID: nextFocusedPaneID
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: targetSpaceID,
            tabID: tabID,
            paneID: nextState.focusedPaneID
        )
    }

    func pinningTab(_ tabID: String) throws -> ShellStateMutationResult {
        guard let tab = tab(tabID: tabID) else {
            throw ShellStateMutationError.tabNotFound
        }
        guard !tab.isPinned else {
            return try organizingTab(
                tabID: tabID,
                section: .pinned,
                index: tabOrganizationLocation(tabID: tabID)?.index
            )
        }
        return try organizingTab(tabID: tabID, section: .pinned)
    }

    func unpinningTab(_ tabID: String) throws -> ShellStateMutationResult {
        guard let tab = tab(tabID: tabID) else {
            throw ShellStateMutationError.tabNotFound
        }
        guard tab.isPinned else {
            return try organizingTab(
                tabID: tabID,
                section: .unpinned,
                index: tabOrganizationLocation(tabID: tabID)?.index
            )
        }
        return try organizingTab(tabID: tabID, section: .unpinned)
    }

    func movingTab(_ tabID: String, sectionOffset: Int) throws -> ShellStateMutationResult {
        guard sectionOffset != 0 else {
            throw ShellStateMutationError.invalidTabOrganizationTarget
        }
        guard let location = tabOrganizationLocation(tabID: tabID) else {
            throw ShellStateMutationError.tabNotFound
        }
        guard let space = space(spaceID: location.spaceID) else {
            throw ShellStateMutationError.spaceNotFound
        }
        let sectionCount = space.tabs(in: location.section).count
        let nextIndex = location.index + sectionOffset
        guard (0..<sectionCount).contains(nextIndex) else {
            throw ShellStateMutationError.invalidTabOrganizationTarget
        }
        return try organizingTab(
            tabID: tabID,
            targetSpaceID: location.spaceID,
            section: location.section,
            index: nextIndex
        )
    }

    func movingTabToSpace(
        tabID: String,
        targetSpaceID: String
    ) throws -> ShellStateMutationResult {
        guard let tab = tab(tabID: tabID) else {
            throw ShellStateMutationError.tabNotFound
        }
        guard tabOrganizationLocation(tabID: tabID)?.spaceID != targetSpaceID else {
            throw ShellStateMutationError.invalidMoveTarget
        }
        return try organizingTab(
            tabID: tabID,
            targetSpaceID: targetSpaceID,
            section: tab.organizationSection
        )
    }

    func settingAttention(
        _ attention: ShellAttentionState,
        for paneID: String
    ) throws -> ShellStateMutationResult {
        guard pane(paneID: paneID) != nil else {
            throw ShellStateMutationError.paneNotFound
        }
        let nextPanes = panes.map { current in
            guard current.paneID == paneID else { return current }
            return ShellPane(
                paneID: current.paneID,
                tabID: current.tabID,
                spaceID: current.spaceID,
                launchTarget: current.launchTarget,
                cwd: current.cwd,
                process: current.process,
                attention: attention,
                context: current.context,
                viewport: current.viewport,
                activity: current.activity,
                alanBinding: current.alanBinding
            )
        }

        return ShellStateMutationResult(
            state: replacing(
                spaces: rebuildingAttention(in: spaces, panes: nextPanes),
                panes: nextPanes,
                focusedPaneID: focusedPaneID ?? paneID
            ),
            spaceID: pane(paneID: paneID)?.spaceID,
            tabID: pane(paneID: paneID)?.tabID,
            paneID: paneID
        )
    }

    private func replacingTabTree(
        tabID: String,
        paneTree: ShellPaneTreeNode
    ) throws -> ShellStateMutationResult {
        guard let targetSpace = spaces.first(where: { space in
            space.tabs.contains(where: { $0.tabID == tabID })
        }),
        let targetTab = targetSpace.tabs.first(where: { $0.tabID == tabID })
        else {
            throw ShellStateMutationError.tabNotFound
        }

        let updatedTab = ShellTab(
            tabID: targetTab.tabID,
            kind: targetTab.kind,
            title: targetTab.title,
            paneTree: paneTree,
            isPinned: targetTab.isPinned
        )
        let nextSpaces = spaces.map { space in
            guard space.spaceID == targetSpace.spaceID else { return space }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: space.tabs.map { tab in
                    tab.tabID == updatedTab.tabID ? updatedTab : tab
                }
            )
        }

        return ShellStateMutationResult(
            state: replacing(
                spaces: nextSpaces,
                panes: panes,
                focusedPaneID: focusedPaneID
            ),
            spaceID: targetSpace.spaceID,
            tabID: updatedTab.tabID,
            paneID: focusedPaneID
        )
    }

    func closingTab(_ tabID: String) throws -> ShellStateMutationResult {
        guard let targetSpace = spaces.first(where: { space in
            space.tabs.contains(where: { $0.tabID == tabID })
        }),
        let targetTab = targetSpace.tabs.first(where: { $0.tabID == tabID })
        else {
            throw ShellStateMutationError.tabNotFound
        }

        let removedPaneIDs = Set(targetTab.paneTree.paneIDs)
        let nextSpaces = spaces.map { space -> ShellSpace in
            let remainingTabs = space.tabs.filter { $0.tabID != tabID }
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: space.attention,
                tabs: remainingTabs
            )
        }
        let nextPanes = panes.filter { !removedPaneIDs.contains($0.paneID) }
        let targetSpaceAfterClose = nextSpaces.first { $0.spaceID == targetSpace.spaceID }
        let preferredPaneID =
            targetSpaceAfterClose?.tabs
                .flatMap(\.paneTree.paneIDs)
                .first { paneID in nextPanes.contains { $0.paneID == paneID } }
            ?? nextPanes.first(where: { $0.spaceID == targetSpace.spaceID })?.paneID
            ?? nextPanes.first?.paneID
        let focusedPane = preferredPaneID.flatMap { paneID in
            nextPanes.first { $0.paneID == paneID }
        }
        let focusedSpaceID = focusedPane?.spaceID ?? targetSpaceAfterClose?.spaceID ?? nextSpaces.first?.spaceID
        let focusedTabID = focusedPane?.tabID
        let retained = retainedContentRecords(in: nextSpaces)
        let nextState = ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneID: preferredPaneID,
            spaces: rebuildingAttention(in: nextSpaces, panes: nextPanes),
            panes: nextPanes,
            paneSlots: retained.paneSlots,
            contents: retained.contents,
            quickTerminal: quickTerminal
        )

        return ShellStateMutationResult(
            state: nextState,
            spaceID: nextState.focusedSpaceID,
            tabID: nextState.focusedTabID,
            paneID: nextState.focusedPaneID
        )
    }

    private func replacing(
        spaces: [ShellSpace],
        panes: [ShellPane],
        focusedPaneID: String?,
        additionalPaneSlots: [ShellPaneSlot] = [],
        additionalContents: [ShellContentInstance] = []
    ) -> ShellStateSnapshot {
        let resolvedFocusedPaneID =
            focusedPaneID.flatMap { candidate in
                panes.contains(where: { $0.paneID == candidate }) ? candidate : nil
            } ?? panes.first?.paneID
        let focusedPane = resolvedFocusedPaneID.flatMap { candidate in
            panes.first(where: { $0.paneID == candidate })
        }
        let retained = retainedContentRecords(in: spaces)
        let nextPaneSlots = (retained.paneSlots ?? []) + additionalPaneSlots
        let nextContents = (retained.contents ?? []) + additionalContents
        let nextContractVersion =
            additionalPaneSlots.isEmpty && additionalContents.isEmpty
            ? contractVersion
            : ShellContentStateSnapshot.currentContractVersion

        return ShellStateSnapshot(
            contractVersion: nextContractVersion,
            windowID: windowID,
            focusedSpaceID: focusedPane?.spaceID ?? spaces.first?.spaceID,
            focusedTabID: focusedPane?.tabID ?? spaces.first?.tabs.first?.tabID,
            focusedPaneID: resolvedFocusedPaneID,
            spaces: spaces,
            panes: panes,
            paneSlots: nextPaneSlots.isEmpty ? nil : nextPaneSlots,
            contents: nextContents.isEmpty ? nil : nextContents,
            quickTerminal: quickTerminal
        )
    }

    private func retainedContentRecords(in spaces: [ShellSpace]) -> (
        paneSlots: [ShellPaneSlot]?,
        contents: [ShellContentInstance]?
    ) {
        let activePaneSlotIDs = Set(spaces.flatMap(\.tabs).flatMap(\.paneTree.paneIDs))
        let retainedPaneSlots = (paneSlots ?? []).filter { activePaneSlotIDs.contains($0.paneSlotID) }
        let retainedContentIDs = Set(retainedPaneSlots.map(\.contentID))
        let retainedContents = (contents ?? []).filter { retainedContentIDs.contains($0.contentID) }

        return (
            paneSlots: retainedPaneSlots.isEmpty ? nil : retainedPaneSlots,
            contents: retainedContents.isEmpty ? nil : retainedContents
        )
    }

    private func rebuildingAttention(in spaces: [ShellSpace], panes: [ShellPane]) -> [ShellSpace] {
        spaces.map { space in
            ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(in: panes.filter { $0.spaceID == space.spaceID }),
                tabs: space.tabs
            )
        }
    }

    private func strongestAttention(in panes: [ShellPane]) -> ShellAttentionState {
        panes
            .map(\.attention)
            .max(by: { Self.attentionRank(for: $0) < Self.attentionRank(for: $1) })
            ?? .idle
    }

    private static func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .idle:
            return 0
        case .active:
            return 1
        case .notable:
            return 2
        case .awaitingUser:
            return 3
        }
    }

    private func nextID(prefix: String, existing: [String]) -> String {
        let nextOrdinal = existing
            .compactMap { identifier -> Int? in
                let components = identifier.split(separator: "_")
                guard let last = components.last else { return nil }
                return Int(last)
            }
            .max()
            .map { $0 + 1 }
            ?? (existing.isEmpty ? 1 : existing.count + 1)

        return "\(prefix)_\(nextOrdinal)"
    }

    private func focusingExistingSettingsContent() throws -> ShellStateMutationResult? {
        let contentState = contentStateProjection()
        guard let existingPaneID = contentState.paneSlots.first(where: { paneSlot in
            guard let content = contentState.content(contentID: paneSlot.contentID) else {
                return false
            }
            return content.kind == .settings
                && content.contentID == ShellContentInstance.settingsContentID
                && content.payload.settings?.surfaceID == ShellContentInstance.settingsSurfaceID
        })?.paneSlotID,
              pane(paneID: existingPaneID) != nil
        else {
            return nil
        }

        return try focusingPane(existingPaneID)
    }

    private func makeContentMount(
        _ contentIntent: ShellContentIntent,
        paneID: String,
        tabID: String,
        spaceID: String,
        defaultTerminalTitle: String,
        terminalSummary: String,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellPreparedContentMount {
        switch contentIntent {
        case .terminal(let launchTarget, let title, let workingDirectory):
            let pane = makeTerminalPane(
                paneID: paneID,
                tabID: tabID,
                spaceID: spaceID,
                launchTarget: launchTarget,
                workingDirectory: workingDirectory ?? defaultWorkingDirectory,
                summary: terminalSummary,
                now: now
            )
            return ShellPreparedContentMount(
                pane: pane,
                paneSlot: nil,
                content: nil,
                title: title ?? defaultTerminalTitle
            )
        case .markdown(let fileURL, let title):
            let resolvedURL = fileURL.isFileURL ? fileURL.standardizedFileURL : fileURL
            let resolvedTitle = Self.markdownTitle(for: resolvedURL, explicitTitle: title)
            let contentID = ShellContentInstance.markdownContentID(forPaneSlotID: paneID)
            let pane = makeContentPlaceholderPane(
                paneID: paneID,
                tabID: tabID,
                spaceID: spaceID,
                title: resolvedTitle,
                summary: "markdown viewer ready",
                now: now
            )
            let paneSlot = ShellPaneSlot(
                paneSlotID: paneID,
                tabID: tabID,
                spaceID: spaceID,
                contentID: contentID,
                attention: .active
            )
            let content = ShellContentInstance(
                contentID: contentID,
                kind: .markdown,
                title: resolvedTitle,
                payload: .markdown(
                    ShellMarkdownContentPayload(
                        fileURL: resolvedURL.absoluteString,
                        title: resolvedTitle
                    )
                ),
                rendererState: ShellContentRendererState(phase: "ready", detail: resolvedURL.path)
            )
            return ShellPreparedContentMount(
                pane: pane,
                paneSlot: paneSlot,
                content: content,
                title: resolvedTitle
            )
        case .settings(let title):
            let resolvedTitle = Self.settingsTitle(explicitTitle: title)
            let pane = makeContentPlaceholderPane(
                paneID: paneID,
                tabID: tabID,
                spaceID: spaceID,
                title: resolvedTitle,
                summary: "settings surface ready",
                now: now
            )
            let paneSlot = ShellPaneSlot(
                paneSlotID: paneID,
                tabID: tabID,
                spaceID: spaceID,
                contentID: ShellContentInstance.settingsContentID,
                attention: .active
            )
            let content = ShellContentInstance(
                contentID: ShellContentInstance.settingsContentID,
                kind: .settings,
                title: resolvedTitle,
                payload: .settings(
                    ShellSettingsContentPayload(
                        surfaceID: ShellContentInstance.settingsSurfaceID,
                        title: resolvedTitle
                    )
                ),
                rendererState: ShellContentRendererState(
                    phase: "ready",
                    detail: ShellContentInstance.settingsSurfaceID
                )
            )
            return ShellPreparedContentMount(
                pane: pane,
                paneSlot: paneSlot,
                content: content,
                title: resolvedTitle
            )
        }
    }

    private func makeTerminalPane(
        paneID: String,
        tabID: String,
        spaceID: String,
        launchTarget: ShellLaunchTarget,
        workingDirectory: String,
        summary: String,
        now: Date
    ) -> ShellPane {
        let formatter = ISO8601DateFormatter()
        return ShellPane(
            paneID: paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: launchTarget,
            cwd: workingDirectory,
            process: Self.defaultProcessBinding(for: launchTarget),
            attention: .active,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: Self.defaultViewportTitle(for: launchTarget),
                summary: summary,
                visibleExcerpt: nil,
                lastActivityAt: formatter.string(from: now)
            ),
            alanBinding: nil
        )
    }

    private func makeContentPlaceholderPane(
        paneID: String,
        tabID: String,
        spaceID: String,
        title: String,
        summary: String,
        now: Date
    ) -> ShellPane {
        let formatter = ISO8601DateFormatter()
        return ShellPane(
            paneID: paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: nil,
            cwd: nil,
            process: nil,
            attention: .active,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: title,
                summary: summary,
                visibleExcerpt: nil,
                lastActivityAt: formatter.string(from: now)
            ),
            alanBinding: nil
        )
    }

    private static func defaultProcessBinding(for launchTarget: ShellLaunchTarget) -> ShellProcessBinding {
        switch launchTarget {
        case .shell:
            let shellPath = defaultShellPath()
            return ShellProcessBinding(
                program: URL(fileURLWithPath: shellPath).lastPathComponent.isEmpty
                    ? "zsh"
                    : URL(fileURLWithPath: shellPath).lastPathComponent,
                argvPreview: ["-l"]
            )
        case .alan:
            return ShellProcessBinding(program: "alan-tui", argvPreview: ["alan", "chat"])
        }
    }

    private static func defaultViewportTitle(for launchTarget: ShellLaunchTarget) -> String {
        switch launchTarget {
        case .shell:
            return "Shell"
        case .alan:
            return "alan"
        }
    }

    private static func defaultTabTitle(
        for contentIntent: ShellContentIntent,
        existingTabCount: Int
    ) -> String {
        switch contentIntent {
        case .terminal(let launchTarget, let title, _):
            if let title {
                return title
            }
            switch launchTarget {
            case .alan:
                return "alan \(existingTabCount + 1)"
            case .shell:
                return "Shell \(existingTabCount + 1)"
            }
        case .markdown(let fileURL, let title):
            let resolvedURL = fileURL.isFileURL ? fileURL.standardizedFileURL : fileURL
            return markdownTitle(for: resolvedURL, explicitTitle: title)
        case .settings(let title):
            return settingsTitle(explicitTitle: title)
        }
    }

    private static func defaultTerminalSummary(for contentIntent: ShellContentIntent) -> String {
        guard case .terminal(let launchTarget, _, _) = contentIntent else {
            return "new shell tab scaffolded"
        }

        return launchTarget == .alan ? "new alan tab scaffolded" : "new shell tab scaffolded"
    }

    private static func markdownTitle(for fileURL: URL, explicitTitle: String?) -> String {
        if let title = explicitTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        let lastPathComponent = fileURL.lastPathComponent.trimmingCharacters(in: .whitespacesAndNewlines)
        return lastPathComponent.isEmpty ? "Markdown" : lastPathComponent
    }

    private static func settingsTitle(explicitTitle: String?) -> String {
        if let title = explicitTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        return "Settings"
    }

    private static func defaultShellPath(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> String {
        let shell = environment["SHELL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let shell, !shell.isEmpty else {
            return "/bin/zsh"
        }
        return shell
    }
}

extension ShellStateSnapshot {
    static let spikePreview = ShellStateSnapshot(
        contractVersion: "0.1",
        windowID: "window_main",
        focusedSpaceID: "space_alan_app",
        focusedTabID: "tab_main",
        focusedPaneID: "pane_1",
        spaces: [
            ShellSpace(
                spaceID: "space_alan_app",
                title: "alan app",
                attention: .awaitingUser,
                tabs: [
                    ShellTab(
                        tabID: "tab_main",
                        kind: .terminal,
                        title: "Main Session",
                        paneTree: ShellPaneTreeNode(
                            nodeID: "node_root",
                            kind: .split,
                            direction: .vertical,
                            paneID: nil,
                            children: [
                                ShellPaneTreeNode(
                                    nodeID: "pane_1",
                                    kind: .pane,
                                    direction: nil,
                                    paneID: "pane_1",
                                    children: nil
                                ),
                                ShellPaneTreeNode(
                                    nodeID: "pane_2",
                                    kind: .pane,
                                    direction: nil,
                                    paneID: "pane_2",
                                    children: nil
                                ),
                            ]
                        )
                    )
                ]
            )
        ],
        panes: [
            ShellPane(
                paneID: "pane_1",
                tabID: "tab_main",
                spaceID: "space_alan_app",
                launchTarget: .alan,
                cwd: "/Users/morris/Developer/Alan",
                process: ShellProcessBinding(program: "alan-tui", argvPreview: ["alan", "chat"]),
                attention: .awaitingUser,
                context: ShellContextSnapshot(
                    workingDirectoryName: "alan",
                    repositoryRoot: "/Users/morris/Developer/Alan",
                    gitBranch: "main",
                    controlPath: "/tmp/alan-shell-control/window_main",
                    alanBindingFile: "/tmp/alan-shell-control/window_main/panes/pane_1/alan-binding.json",
                    launchStrategy: "installed_binary",
                    shellIntegrationSource: "ghostty_shell_integration",
                    processState: "running",
                    lastMetadataAt: "2026-04-01T10:30:00Z",
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: "alan",
                    summary: "waiting for approval",
                    visibleExcerpt: nil,
                    lastActivityAt: "2026-04-01T10:30:00Z"
                ),
                alanBinding: ShellAlanBinding(
                    sessionID: "sess_123",
                    runStatus: "yielded",
                    pendingYield: true,
                    source: "preview",
                    lastProjectedAt: "2026-04-01T10:30:00Z"
                )
            ),
            ShellPane(
                paneID: "pane_2",
                tabID: "tab_main",
                spaceID: "space_alan_app",
                launchTarget: .shell,
                cwd: "/Users/morris/Developer/Alan",
                process: ShellProcessBinding(program: "zsh", argvPreview: nil),
                attention: .idle,
                context: ShellContextSnapshot(
                    workingDirectoryName: "alan",
                    repositoryRoot: "/Users/morris/Developer/Alan",
                    gitBranch: "main",
                    controlPath: "/tmp/alan-shell-control/window_main",
                    alanBindingFile: "/tmp/alan-shell-control/window_main/panes/pane_2/alan-binding.json",
                    launchStrategy: "path_binary",
                    shellIntegrationSource: "ghostty_shell_integration",
                    processState: "running",
                    lastMetadataAt: "2026-04-01T10:24:00Z",
                    lastCommandExitCode: 0
                ),
                viewport: ShellViewportSnapshot(
                    title: "shell",
                    summary: "idle shell",
                    visibleExcerpt: nil,
                    lastActivityAt: nil
                ),
                alanBinding: nil
            ),
        ]
    )
}
