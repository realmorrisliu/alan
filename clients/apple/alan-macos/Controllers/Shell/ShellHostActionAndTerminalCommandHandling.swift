import Foundation

#if os(macOS)
extension ShellHostController {
    @discardableResult
    func focusAdjacentPane(direction: ShellSpatialFocusDirection) -> Bool {
        let previousPaneID = shellState.focusedPaneID
        let rustResult: ShellStateMutationResult
        do {
            rustResult = try reducerAdapter.apply(
                state: shellState,
                operation: .focusAdjacentPane(direction: direction)
            )
        } catch {
            return false
        }
        // Mirror focus(paneID:): acknowledge command-failure activity/attention on the newly
        // focused pane so spatial focus also clears a stale failure indicator within the tab.
        let result: ShellStateMutationResult
        if let tabID = rustResult.tabID, let focusedPaneID = rustResult.paneID {
            result = ShellStateMutationResult(
                state: rustResult.state.acknowledgingCommandFailureActivities(
                    in: tabID,
                    focusedPaneID: focusedPaneID
                ),
                spaceID: rustResult.spaceID,
                tabID: rustResult.tabID,
                paneID: rustResult.paneID
            )
        } else {
            result = rustResult
        }
        applyMutationResult(result)
        controlPlane.recordSpatialFocus(
            requestID: nil,
            spaceID: result.spaceID,
            tabID: result.tabID,
            previousPaneID: previousPaneID,
            currentPaneID: result.paneID,
            direction: direction,
            applied: true
        )
        return true
    }

    @discardableResult
    func performShellWorkspaceCommand(_ command: ShellWorkspaceCommand) -> Bool {
        switch command {
        case .newTerminalTab:
            return performShellAutomationCommand(
                .createTab(
                    ShellAutomationCreateTabRequest(
                        launchTarget: .shell,
                        spaceID: nil,
                        title: nil,
                        workingDirectory: nil
                    )
                )
            ).applied
        case .splitLeft:
            return performShellAutomationSplitFromFocusedPane(.left)
        case .splitRight:
            return performShellAutomationSplitFromFocusedPane(.right)
        case .splitUp:
            return performShellAutomationSplitFromFocusedPane(.up)
        case .splitDown:
            return performShellAutomationSplitFromFocusedPane(.down)
        case .focusLeft:
            return focusAdjacentPane(direction: .left)
        case .focusRight:
            return focusAdjacentPane(direction: .right)
        case .focusUp:
            return focusAdjacentPane(direction: .up)
        case .focusDown:
            return focusAdjacentPane(direction: .down)
        case .equalizeSplits:
            return equalizeSelectedTabSplits()
        case .togglePaneZoom:
            return toggleSelectedPaneZoom()
        case .movePaneLeft:
            return moveSelectedPaneWithinTab(.left)
        case .movePaneRight:
            return moveSelectedPaneWithinTab(.right)
        case .movePaneUp:
            return moveSelectedPaneWithinTab(.up)
        case .movePaneDown:
            return moveSelectedPaneWithinTab(.down)
        case .closePane:
            guard let paneID = selectedPane?.paneID else { return false }
            return requestClosePane(paneID: paneID)
        case .closeTab:
            guard let tabID = selectedTabID else { return false }
            return requestCloseTab(tabID: tabID)
        }
    }

    private func performShellAutomationSplitFromFocusedPane(
        _ placement: ShellPaneSplitDirection
    ) -> Bool {
        guard let focusedPaneID = shellState.focusedPaneID else { return false }
        return performShellAutomationCommand(
            .splitPane(ShellAutomationPaneSplitRequest(paneID: focusedPaneID, placement: placement))
        ).applied
    }

    func shellActionTitle(_ id: ShellActionID) -> String {
        actionCoordinator.title(id)
    }

    func shellActionAvailability(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionAvailability {
        actionCoordinator.availability(id, target: target, state: shellState)
    }

    func shellActionShortcut(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionShortcut? {
        actionCoordinator.shortcut(id, target: target)
    }

    @discardableResult
    func performShellAction(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection,
        source: ShellTerminalCommandSource = .keyboardShortcut
    ) -> ShellActionExecutionResult {
        actionCoordinator.perform(
            id,
            target: target,
            source: source,
            state: shellState,
            isModalFlowActive: isPresentingSpaceCreation,
            openSearch: { [weak self] source, target in
                self?.openTerminalSearch(source: source, target: target) ?? false
            },
            effectHandlers: shellActionEffectHandlers
        )
    }

    private var shellActionEffectHandlers: ShellActionEffectHandlers {
        ShellActionEffectHandlers(
            selectedTabID: { [weak self] in self?.selectedTabID },
            selectedPaneID: { [weak self] in self?.selectedPane?.paneID },
            performWorkspaceCommand: { [weak self] command in
                self?.performShellWorkspaceCommand(command) ?? false
            },
            openTab: { [weak self] launchTarget, spaceID in
                self?.performShellAutomationCommand(
                    .createTab(
                        ShellAutomationCreateTabRequest(
                            launchTarget: launchTarget,
                            spaceID: spaceID,
                            title: nil,
                            workingDirectory: nil
                        )
                    )
                ).applied ?? false
            },
            requestCloseTab: { [weak self] tabID in
                self?.requestCloseTab(tabID: tabID) ?? false
            },
            duplicateTab: { [weak self] tabID in
                self?.duplicateTab(tabID: tabID) ?? false
            },
            openTabInSplitView: { [weak self] tabID in
                self?.openTabInSplitView(tabID: tabID) ?? false
            },
            requestClosePane: { [weak self] paneID in
                self?.requestClosePane(paneID: paneID) ?? false
            },
            selectAdjacentTab: { [weak self] offset in
                self?.selectAdjacentTab(offset: offset) ?? false
            },
            selectAdjacentSpace: { [weak self] offset in
                self?.selectAdjacentSpace(offset: offset) ?? false
            },
            selectSpaceAt: { [weak self] index in
                self?.selectSpace(at: index) ?? false
            },
            pinTab: { [weak self] tabID in
                self?.pinTab(tabID: tabID) ?? false
            },
            unpinTab: { [weak self] tabID in
                self?.unpinTab(tabID: tabID) ?? false
            },
            updatePinnedTab: { [weak self] tabID in
                self?.updatePinnedTabSnapshot(tabID: tabID) ?? false
            },
            moveTab: { [weak self] tabID, offset in
                self?.moveTab(tabID: tabID, offset: offset) ?? false
            },
            moveTabToSpace: { [weak self] tabID, spaceID in
                self?.moveTabToSpace(tabID: tabID, targetSpaceID: spaceID) ?? false
            },
            movePaneWithinTab: { [weak self] paneID, placement in
                self?.movePaneWithinTab(paneID: paneID, placement: placement) ?? false
            },
            clearTerminal: { [weak self] paneID in
                self?.clearTerminal(paneID: paneID) ?? false
            }
        )
    }

    @discardableResult
    private func clearTerminal(paneID: String?) -> Bool {
        guard let pane = paneID.flatMap({ shellState.pane(paneID: $0) }) ?? selectedPane,
              let contentID = terminalContentID(mountedIn: pane)
        else {
            return false
        }

        clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
        let delivery = terminalRuntimeRegistry.sendText(toTerminalContentID: contentID, text: "\u{0c}")
        return delivery.applied
    }

    @discardableResult
    func resizeSplit(splitNodeID: String, ratio: Double, persist: Bool = true) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .resizeSplit(splitNodeID: splitNodeID, ratio: ratio)
            )
        } catch {
            return false
        }
        applyMutationResult(result, publish: persist)
        return true
    }

    @discardableResult
    func equalizeSelectedTabSplits() -> Bool {
        let previousTab = selectedTab
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .equalizeSplits(tabID: selectedTabID)
            )
        } catch {
            return false
        }
        let changedSplitIDs = previousTab
            .flatMap { previous in
                result.state.tab(tabID: previous.tabID)?.paneTree
                    .splitNodeIDsWithChangedRatios(comparedTo: previous.paneTree)
            } ?? []
        applyMutationResult(result)
        if let tabID = result.tabID,
           let previousTab,
           !changedSplitIDs.isEmpty
        {
            controlPlane.recordSplitEqualized(
                requestID: nil,
                spaceID: result.spaceID,
                tabID: tabID,
                changedSplitIDs: changedSplitIDs,
                affectedPaneIDs: previousTab.paneTree.paneIDs
            )
        }
        return true
    }

    @discardableResult
    func closeSelectedTab() -> Bool {
        guard let selectedTabID else { return false }
        return requestCloseTab(tabID: selectedTabID)
    }

    @discardableResult
    func closeSelectedPane() -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return requestClosePane(paneID: paneID)
    }

    @discardableResult
    func closePaneByID(_ paneID: String) -> Bool {
        requestClosePane(paneID: paneID)
    }

    @discardableResult
    func liftSelectedPaneToTab(title: String? = nil) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return liftPaneToTab(paneID: paneID, title: title) == .lifted
    }

    @discardableResult
    func moveSelectedPane(
        toTab tabID: String,
        direction: ShellSplitDirection = .vertical
    ) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return movePane(paneID: paneID, toTab: tabID, direction: direction)
    }

    @discardableResult
    func moveSelectedPaneWithinTab(_ placement: ShellPaneSplitDirection) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return movePaneWithinTab(paneID: paneID, placement: placement)
    }

    @discardableResult
    func focusTopRoutingCandidate(preferredPaneID: String? = nil) -> String? {
        guard let candidate = routingCandidates(preferredPaneID: preferredPaneID).first else {
            return nil
        }
        focus(paneID: candidate.paneID)
        return candidate.paneID
    }

    @discardableResult
    func setAttention(_ attention: ShellAttentionState, for paneID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .setAttention(paneSlotID: paneID, attention: attention)
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    func copySnapshotJSON() {
        pasteboard.writeString(snapshotJSON)
        lastCopiedAt = .now
    }

    func terminalCommandResolution(
        for command: ShellTerminalCommand,
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> ShellTerminalCommandResolution {
        ShellCommandTargetResolver.resolveTerminalCommand(
            command,
            source: source,
            target: target,
            state: shellState
        ) { [terminalRuntimeRegistry] paneID in
            terminalRuntimeRegistry.terminalCommandRuntimeState(for: paneID)
        }
    }

    func canCopyTerminalSelection(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .copySelection,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    func canPasteIntoTerminal(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .paste,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    func canOpenTerminalSearch(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .search,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    @discardableResult
    func copyTerminalSelection(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection,
        writer: AlanTerminalPasteboardWriting? = nil
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .copySelection,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        if let writer {
            return terminalRuntimeRegistry.copySelection(for: terminalTarget.paneID, to: writer)
        }
        return terminalRuntimeRegistry.copySelection(for: terminalTarget.paneID)
    }

    @discardableResult
    func pasteIntoTerminalFromPasteboard(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let text = pasteboard.readString(), !text.isEmpty else {
            return false
        }
        return pasteIntoTerminal(text, source: source, target: target)
    }

    @discardableResult
    func pasteIntoTerminal(
        _ text: String,
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .paste,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.pasteText(text, to: terminalTarget.paneID).applied
    }

    @discardableResult
    func openTerminalSearch(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .search,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.beginFindInteraction(for: terminalTarget.paneID)
    }

    var focusedPaneHasReliableSemanticCommands: Bool {
        guard focusedContentSupportsTerminalCommands else { return false }
        guard let paneID = selectedPane?.paneID,
              paneID == terminalRuntime.paneID
        else {
            return false
        }
        return terminalRuntime.surfaceState.terminalMode == .normalBuffer
            && terminalRuntime.surfaceState.semanticCommands.hasReliableCommandBoundaries
    }

    @discardableResult
    func jumpToPreviousPrompt() -> Bool {
        navigateSemanticPrompt(.previous)
    }

    @discardableResult
    func jumpToNextPrompt() -> Bool {
        navigateSemanticPrompt(.next)
    }

    @discardableResult
    func copyLastCommandOutput() -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .copyLastCommandOutput,
            source: .commandUI
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.copyLastCommandOutput(for: terminalTarget.paneID)
    }

    @discardableResult
    func searchLastCommandOutput() -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .searchLastCommandOutput,
            source: .commandUI
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.beginLastCommandOutputSearch(for: terminalTarget.paneID)
    }

    @discardableResult
    private func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return terminalRuntimeRegistry.navigateSemanticPrompt(for: paneID, direction: direction)
    }

}
#endif
