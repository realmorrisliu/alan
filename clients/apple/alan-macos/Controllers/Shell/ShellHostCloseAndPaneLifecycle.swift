import Foundation

#if os(macOS)
extension ShellHostController {
    func pane(paneID: String) -> ShellPane? {
        shellState.panes.first { $0.paneID == paneID }
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

    func closeTab(tabID: String) -> ShellTabCloseResult {
        if let impact = closeGuardImpact(for: .tab(tabID)) {
            return .requiresConfirmation(impact)
        }
        return applyCloseTabMutation(tabID: tabID)
    }

    @discardableResult
    func requestCloseTab(tabID: String) -> Bool {
        switch closeTab(tabID: tabID) {
        case .closed:
            return true
        case .requiresConfirmation(let impact):
            return confirmAndApplyClose(impact)
        case .tabNotFound, .lastTab:
            return false
        }
    }

    private func applyCloseTabMutation(tabID: String) -> ShellTabCloseResult {
        do {
            let result = try reducerAdapter.apply(
                state: shellState,
                operation: .closeTab(tabID: tabID)
            )
            applyMutationResult(result)
            return .closed
        } catch ShellStateMutationError.lastTab {
            return .lastTab
        } catch ShellStateMutationError.tabNotFound {
            return .tabNotFound
        } catch {
            return .tabNotFound
        }
    }

    func closePane(paneID: String) -> ShellPaneCloseResult {
        if let impact = closeGuardImpact(for: .paneSlot(paneID)) {
            return .requiresConfirmation(impact)
        }
        return applyClosePaneMutation(paneID: paneID)
    }

    @discardableResult
    func requestClosePane(paneID: String) -> Bool {
        switch closePane(paneID: paneID) {
        case .closed:
            return true
        case .requiresConfirmation(let impact):
            return confirmAndApplyClose(impact)
        case .paneNotFound, .lastTab:
            return false
        }
    }

    private func applyClosePaneMutation(paneID: String) -> ShellPaneCloseResult {
        do {
            let result = try reducerAdapter.apply(
                state: shellState,
                operation: .closePane(paneSlotID: paneID)
            )
            applyMutationResult(result)
            return .closed
        } catch ShellStateMutationError.lastTab {
            return .lastTab
        } catch ShellStateMutationError.paneNotFound {
            return .paneNotFound
        } catch {
            return .paneNotFound
        }
    }

    @discardableResult
    func closePaneAfterTerminalRuntimeExit(paneID: String) -> Bool {
        guard !terminalAutoCloseIsSuppressed(paneID: paneID) else { return false }
        return applyClosePaneMutation(paneID: paneID) == .closed
    }

    func confirmAndApplyClose(_ impact: ShellCloseGuardImpact) -> Bool {
        closeWorkflow.confirmAndPerformClose(
            impact: impact,
            recordDiagnostic: recordControlPlaneDiagnostic,
            applyClose: { transcriptSnapshots in
                applyConfirmedClose(
                    impact,
                    transcriptSnapshotOverrides: transcriptSnapshots
                )
            }
        )
    }

    @discardableResult
    private func applyConfirmedClose(
        _ impact: ShellCloseGuardImpact,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot] = [:]
    ) -> Bool {
        switch impact.scope {
        case .paneSlot(let paneID):
            return applyClosePaneMutation(paneID: paneID) == .closed
        case .tab(let tabID):
            return applyCloseTabMutation(tabID: tabID) == .closed
        case .window, .app:
            persistenceCoordinator.persistCurrentManifest(
                transcriptSnapshotOverrides: transcriptSnapshotOverrides
            )
            shutdownTerminalRuntimes()
            return true
        }
    }

    func closeGuardImpact(for scope: ShellCloseGuardScope) -> ShellCloseGuardImpact? {
        let paneIDs = terminalPaneIDsAffected(by: scope)
        guard !paneIDs.isEmpty else { return nil }
        let contentState = shellState.contentStateProjection()
        let affectedContentIDs = paneIDs.compactMap { paneID -> String? in
            terminalContentIDForCloseGuard(paneID: paneID, contentState: contentState)
        }
        let activeContentIDs = paneIDs.compactMap { paneID -> String? in
            guard let pane = shellState.pane(paneID: paneID),
                  let contentID = terminalContentIDForCloseGuard(
                    paneID: paneID,
                    contentState: contentState
                  ),
                  terminalRequiresCloseConfirmation(pane: pane, contentID: contentID)
            else {
                return nil
            }
            return contentID
        }
        let impact = ShellCloseGuardImpact(
            scope: scope,
            affectedTerminalContentIDs: affectedContentIDs,
            activeTerminalContentIDs: activeContentIDs
        )
        return impact.requiresConfirmation ? impact : nil
    }

    private func terminalContentIDForCloseGuard(
        paneID: String,
        contentState: ShellContentStateSnapshot
    ) -> String? {
        if let content = contentState.contentMounted(in: paneID),
           content.kind == .terminal
        {
            return content.contentID
        }
        return nil
    }

    private func terminalPaneIDsAffected(by scope: ShellCloseGuardScope) -> [String] {
        switch scope {
        case .paneSlot(let paneID):
            return shellState.pane(paneID: paneID).map { [$0.paneID] } ?? []
        case .tab(let tabID):
            return shellState.tab(tabID: tabID)?.paneTree.paneIDs ?? []
        case .window, .app:
            return shellState.spaces.flatMap(\.tabs).flatMap(\.paneTree.paneIDs)
        }
    }

    private func terminalRequiresCloseConfirmation(
        pane: ShellPane,
        contentID: String
    ) -> Bool {
        if pane.alanBinding?.pendingRequest == true {
            return true
        }
        if let processState = pane.context?.processState {
            if processState == "exited" {
                return false
            }
            if processState == ShellTabActiveTaskState.foregroundCommand.rawValue
                || processState == ShellTabActiveTaskState.alanRunning.rawValue
                || processState == ShellTabActiveTaskState.alanPendingYield.rawValue
                || processState == ShellTabActiveTaskState.alanProcess.rawValue
                || processState == ShellTabActiveTaskState.unknown.rawValue
            {
                return true
            }
        }
        let runtime = terminalRuntimeRegistry.snapshot(forTerminalContentID: contentID)
        let metadata = runtime.paneMetadata
        if metadata.processExited {
            return false
        }
        if let activeTaskState = metadata.activeTaskState {
            return activeTaskState.protectsFromPruning
        }
        return terminalRuntimeRegistry.registeredContentIDs.contains(contentID)
    }

    func focusedPaneWorkingDirectory() -> String? {
        guard let pane = focusedPane ?? selectedPane else { return nil }
        let runtimeCwd = runtime(for: pane.paneID).paneMetadata.workingDirectory
        return nonEmptyWorkingDirectory(runtimeCwd)
            ?? nonEmptyWorkingDirectory(pane.cwd)
    }

    func targetTerminalProfileID(in requestedSpaceID: String?, explicit: String?) -> String? {
        shellState.terminalProfileIDForNewTerminal(in: requestedSpaceID, explicit: explicit)
    }

    func targetTerminalProfileID(forSplitFromPaneID paneID: String, explicit: String?) -> String? {
        shellState.terminalProfileIDForNewSplit(from: paneID, explicit: explicit)
    }

    private func nonEmptyWorkingDirectory(_ path: String?) -> String? {
        guard let path else { return nil }
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    @discardableResult
    func closePaneAfterChildExitIfNeeded(
        paneID: String,
        processExited: Bool
    ) -> Bool {
        guard processExited else { return false }
        guard pane(paneID: paneID) != nil else { return false }
        guard !terminalAutoCloseIsSuppressed(paneID: paneID) else { return false }
        return applyClosePaneMutation(paneID: paneID) == .closed
    }

    private func terminalAutoCloseIsSuppressed(paneID: String) -> Bool {
        let contentState = shellState.contentStateProjection()
        let contentID = terminalContentIDForCloseGuard(
            paneID: paneID,
            contentState: contentState
        ) ?? pane(paneID: paneID)?.terminalContentID
        guard let contentID else { return false }
        return closeWorkflow.suppressesAutoClose(forTerminalContentID: contentID)
    }

    func movePane(
        paneID: String,
        toTab targetTabID: String,
        direction: ShellSplitDirection
    ) -> Bool {
        let targetTabTitle = shellState.tab(tabID: targetTabID)?.title ?? targetTabID
        do {
            let result = try reducerAdapter.apply(
                state: shellState,
                operation: .movePaneToTab(
                    paneSlotID: paneID,
                    targetTabID: targetTabID,
                    direction: direction
                )
            )
            let annotatedResult = annotatingPaneViewport(
                result,
                paneID: paneID,
                fallbackSummary: "pane moved to \(targetTabTitle)"
            )
            applyMutationResult(annotatedResult)
            return true
        } catch {
            return false
        }
    }

    func movePaneWithinTab(
        paneID: String,
        placement: ShellPaneSplitDirection
    ) -> Bool {
        movePaneWithinTab(
            paneID: paneID,
            placement: placement,
            source: .explicitCommand
        )
    }

    func movePaneWithinTab(
        paneID: String,
        placement: ShellPaneSplitDirection,
        source: ShellPaneMovementInputSource
    ) -> Bool {
        guard ShellPaneMovementInteractionPolicy.terminalSelectionFirst
            .allowsPaneMovement(from: source)
        else {
            return false
        }

        do {
            let result = try reducerAdapter.apply(
                state: shellState,
                operation: .movePaneWithinTab(paneSlotID: paneID, placement: placement)
            )
            applyMutationResult(result)
            if let tabID = result.tabID {
                controlPlane.recordPaneMovedInTab(
                    requestID: nil,
                    spaceID: result.spaceID,
                    tabID: tabID,
                    paneID: paneID,
                    placement: placement,
                    mountedContentInstanceID: paneID
                )
            }
            return true
        } catch {
            return false
        }
    }

    func liftPaneToTab(paneID: String, title: String? = nil) -> ShellPaneLiftResult {
        let resolvedTitle = title ?? shellState.pane(paneID: paneID)?.viewport?.title ?? "Lifted Pane"
        do {
            let result = try reducerAdapter.apply(
                state: shellState,
                operation: .movePaneToNewTab(
                    paneSlotID: paneID,
                    title: resolvedTitle
                )
            )
            let annotatedResult = annotatingPaneViewport(
                result,
                paneID: paneID,
                fallbackSummary: "pane moved to its own tab"
            )
            applyMutationResult(annotatedResult)
            return .lifted
        } catch ShellStateMutationError.lastPane {
            return .lastPane
        } catch ShellStateMutationError.paneNotFound {
            return .paneNotFound
        } catch {
            return .paneNotFound
        }
    }

    private func annotatingPaneViewport(
        _ result: ShellStateMutationResult,
        paneID: String,
        fallbackSummary: String,
        now: Date = .now
    ) -> ShellStateMutationResult {
        let formatter = ISO8601DateFormatter()
        let timestamp = formatter.string(from: now)
        let nextPanes = result.state.panes.map { pane in
            guard pane.paneID == paneID else { return pane }
            let viewport = ShellViewportSnapshot(
                title: pane.viewport?.title,
                summary: pane.viewport?.summary ?? fallbackSummary,
                visibleExcerpt: pane.viewport?.visibleExcerpt,
                lastActivityAt: timestamp
            )
            return ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.cwd,
                process: pane.process,
                attention: pane.attention,
                context: pane.context,
                viewport: viewport,
                activity: pane.activity,
                alanBinding: pane.alanBinding,
                terminalProfileID: pane.terminalProfileID
            )
        }
        let nextState = ShellStateSnapshot(
            contractVersion: result.state.contractVersion,
            windowID: result.state.windowID,
            focusedSpaceID: result.state.focusedSpaceID,
            focusedTabID: result.state.focusedTabID,
            focusedPaneID: result.state.focusedPaneID,
            spaces: result.state.spaces,
            panes: nextPanes,
            paneSlots: result.state.paneSlots,
            contents: result.state.contents,
            zoomedPaneIDByTabID: result.state.zoomedPaneIDByTabID
        )
        return ShellStateMutationResult(
            state: nextState,
            spaceID: result.spaceID,
            tabID: result.tabID,
            paneID: result.paneID
        )
    }

    private var totalTabCount: Int {
        shellState.spaces.reduce(into: 0) { partialResult, space in
            partialResult += space.tabs.count
        }
    }

    func strongestAttention(in panes: [ShellPane]) -> ShellAttentionState {
        let now = Date()
        return panes
            .map { shellEffectiveAttention(for: $0, now: now) }
            .max(by: { Self.attentionRank(for: $0) < Self.attentionRank(for: $1) })
            ?? .idle
    }

    func publishControlPlaneState(
        pinSnapshotTabIDs: Set<String> = [],
        coalesced: Bool = false
    ) {
        persistenceCoordinator.publishControlPlaneState(
            state: shellState,
            terminalRuntimeRegistry: terminalRuntimeRegistry,
            controlPlane: controlPlane,
            pinSnapshotTabIDs: pinSnapshotTabIDs,
            coalesced: coalesced
        )
    }

    static func attentionRank(for attention: ShellAttentionState) -> Int {
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
}
#endif
