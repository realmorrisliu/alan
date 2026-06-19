import Foundation

enum ShellStateMutationError: String, Error {
    case spaceNotFound = "space_not_found"
    case tabNotFound = "tab_not_found"
    case paneNotFound = "pane_not_found"
    case unsupportedContent = "unsupported_content"
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

extension ShellStateSnapshot {
    private static func defaultShellWorkingDirectory() -> String {
        FileManager.default.homeDirectoryForCurrentUser.path
    }

    func terminalProfileIDForNewTerminal(
        in requestedSpaceID: String?,
        explicit: String?
    ) -> String? {
        if let explicit {
            return explicit
        }
        let targetSpaceID = requestedSpaceID ?? focusedSpaceID ?? spaces.first?.spaceID
        guard let targetSpaceID else { return nil }
        return space(spaceID: targetSpaceID)?.terminalProfileID
    }

    func terminalProfileIDForNewSplit(
        from paneID: String,
        explicit: String?
    ) -> String? {
        if let explicit {
            return explicit
        }
        guard let pane = pane(paneID: paneID) else { return nil }
        return pane.terminalProfileID
            ?? terminalProfileIDForNewTerminal(in: pane.spaceID, explicit: nil)
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
                    ],
                    selectedTabID: tabID
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

    func acknowledgingCommandFailureActivities(
        in tabID: String,
        focusedPaneID: String
    ) -> ShellStateSnapshot {
        let acknowledgedPanes = panesAcknowledgingCommandFailureActivities(
            in: tabID,
            focusedPaneID: focusedPaneID
        )
        return replacingRuntimeSupport(
            spaces: rebuildingAttention(in: spaces, panes: acknowledgedPanes),
            panes: acknowledgedPanes,
            focusedPaneID: focusedPaneID
        )
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
                alanBinding: current.alanBinding,
                terminalProfileID: current.terminalProfileID
            )
        }
        let nextSpaces = rebuildingAttention(in: spaces, panes: updatedPanes)
        return ShellStateMutationResult(
            state: replacingRuntimeSupport(
                spaces: nextSpaces,
                panes: updatedPanes,
                focusedPaneID: focusedPaneID
            ),
            spaceID: targetPane.spaceID,
            tabID: targetPane.tabID,
            paneID: targetPane.paneID
        )
    }

    func clearableInactiveTemporaryTabIDs(
        in spaceID: String,
        activeTaskByTabID: [String: ShellTabActiveTaskState] = [:]
    ) throws -> [String] {
        guard let space = space(spaceID: spaceID) else {
            throw ShellStateMutationError.spaceNotFound
        }

        return space.unpinnedTabs.compactMap { tab in
            guard tab.tabID != space.resolvedSelectedTabID else { return nil }
            let activeTask = activeTaskByTabID[tab.tabID] ?? .inactive
            return activeTask.protectsFromPruning ? nil : tab.tabID
        }
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
                alanBinding: current.alanBinding,
                terminalProfileID: current.terminalProfileID
            )
        }
    }

    private func replacingRuntimeSupport(
        spaces: [ShellSpace],
        panes: [ShellPane],
        focusedPaneID: String?
    ) -> ShellStateSnapshot {
        let resolvedFocusedPaneID =
            focusedPaneID.flatMap { candidate in
                panes.contains(where: { $0.paneID == candidate }) ? candidate : nil
            } ?? panes.first?.paneID
        let focusedPane = resolvedFocusedPaneID.flatMap { candidate in
            panes.first(where: { $0.paneID == candidate })
        }
        let repairedSpaces = spaces.map { space in
            space.repairingSelectedTabID(
                preferredTabID: focusedPane?.spaceID == space.spaceID ? focusedPane?.tabID : nil
            )
        }
        let retained = retainedContentRecords(in: spaces, panes: panes)

        return ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedPane?.spaceID ?? spaces.first?.spaceID,
            focusedTabID: focusedPane?.tabID ?? spaces.first?.tabs.first?.tabID,
            focusedPaneID: resolvedFocusedPaneID,
            spaces: repairedSpaces,
            panes: panes,
            paneSlots: retained.paneSlots,
            contents: retained.contents
        )
    }

    private func retainedContentRecords(
        in spaces: [ShellSpace],
        panes sourcePanes: [ShellPane]
    ) -> (
        paneSlots: [ShellPaneSlot]?,
        contents: [ShellContentInstance]?
    ) {
        let paneSlotLocations = paneSlotLocations(in: spaces)
        let panesByID = sourcePanes.reduce(into: [String: ShellPane]()) { panesByID, pane in
            panesByID[pane.paneID] = pane
        }
        let retainedPaneSlots = (paneSlots ?? []).compactMap { paneSlot -> ShellPaneSlot? in
            guard let location = paneSlotLocations[paneSlot.paneSlotID] else { return nil }
            return ShellPaneSlot(
                paneSlotID: paneSlot.paneSlotID,
                tabID: location.tabID,
                spaceID: location.spaceID,
                contentID: paneSlot.contentID,
                attention: panesByID[paneSlot.paneSlotID]?.attention ?? paneSlot.attention
            )
        }
        let retainedContentIDs = Set(retainedPaneSlots.map(\.contentID))
        let retainedContents = (contents ?? []).filter { retainedContentIDs.contains($0.contentID) }

        return (
            paneSlots: retainedPaneSlots.isEmpty ? nil : retainedPaneSlots,
            contents: retainedContents.isEmpty ? nil : retainedContents
        )
    }

    private func paneSlotLocations(in spaces: [ShellSpace]) -> [String: (spaceID: String, tabID: String)] {
        spaces.reduce(into: [String: (spaceID: String, tabID: String)]()) { locationsByID, space in
            for tab in space.tabs {
                for paneSlotID in tab.paneTree.paneIDs {
                    locationsByID[paneSlotID] = (spaceID: space.spaceID, tabID: tab.tabID)
                }
            }
        }
    }

    private func rebuildingAttention(in spaces: [ShellSpace], panes: [ShellPane]) -> [ShellSpace] {
        spaces.map { space in
            ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(in: panes.filter { $0.spaceID == space.spaceID }),
                tabs: space.tabs,
                selectedTabID: space.resolvedSelectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
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
        }
    }

    private static func defaultViewportTitle(for launchTarget: ShellLaunchTarget) -> String {
        switch launchTarget {
        case .shell:
            return "Shell"
        }
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
