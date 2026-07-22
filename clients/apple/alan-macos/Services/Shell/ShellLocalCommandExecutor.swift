import Foundation

#if os(macOS)
enum AlanShellLocalCommandSideEffect {
    case sendText(paneSlotID: String, contentID: String, text: String)
    case sendKey(
        paneSlotID: String,
        contentID: String,
        key: TerminalRuntimeControlKey
    )
}

struct AlanShellLocalCommandResult {
    let response: AlanShellControlResponse
    let updatedState: ShellStateSnapshot?
    let sideEffect: AlanShellLocalCommandSideEffect?
}

enum AlanShellLocalCommandExecutor {
    private static let reducerAdapter = ShellCoreReducerAdapter()

    static func execute(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        context: AlanShellLocalCommandExecutionContext = .init()
    ) -> AlanShellLocalCommandResult? {
        if command.command.isShellCoreLocalCommandSupported {
            let shellCoreCommand = command.resolvingShellCoreDefaults(in: state)
            do {
                let result = try ShellCoreFFIAdapter.shared.handleControlCommand(
                    shellCoreCommand,
                    state: state,
                    context: context
                )
                return AlanShellLocalCommandResult(
                    shellCoreResult: result,
                    command: command
                )
            } catch {
                // Only host-readable snapshots can fall through when shell-core infrastructure is
                // unavailable. Mutations remain Rust-core-owned and fail closed.
                if command.command.canFallThroughToHostWhenShellCoreUnavailable,
                   let coreError = error as? ShellCoreFFIAdapterError,
                   coreError.indicatesShellCoreUnavailable
                {
                    return hostReadableSnapshotResult(command: command, state: state)
                }
                return shellCoreFailureResult(command: command, state: state, error: error)
            }
        }

        switch command.command {
        case .state,
             .spaceList,
             .spaceCreate,
             .tabList,
             .tabOpen,
             .tabClose,
             .tabReorder,
             .tabPin,
             .tabUnpin,
             .tabMoveToSpace,
             .paneList,
             .paneSplit,
             .paneClose,
             .paneLift,
             .paneMove,
             .paneMoveWithinTab,
             .paneFocus,
             .paneSpatialFocus,
             .paneResizeSplit,
             .paneEqualizeSplits,
             .paneZoom,
             .paneUnzoom,
             .terminalSendText,
             .terminalSendKey,
             .attentionSet:
            return shellCoreRoutingFailureResult(command: command, state: state)

        case .spaceSetTerminalProfile:
            guard let spaceID = command.spaceID ?? state.focusedSpaceID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "space_required",
                        errorMessage: "space_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            guard state.space(spaceID: spaceID) != nil else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        spaceID: spaceID,
                        errorCode: "space_not_found",
                        errorMessage: "The requested space does not exist."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try reducerAdapter.apply(
                    state: state,
                    operation: .setTerminalProfile(
                        spaceID: spaceID,
                        terminalProfileID: command.terminalProfileID
                    )
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        snapshot: result.state,
                        spaceID: spaceID,
                        tabID: result.state.focusedTabID,
                        paneID: result.state.focusedPaneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch {
                return shellCoreFailureResult(command: command, state: state, error: error)
            }

        case .paneSnapshot:
            guard let paneID = command.paneID,
                  let pane = state.pane(paneID: paneID)
            else {
                return AlanShellLocalCommandResult(
                    response: failureResponse(
                        for: .paneNotFound,
                        command: command,
                        state: state
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    pane: pane,
                    spaceID: pane.spaceID,
                    tabID: pane.tabID,
                    paneID: pane.paneID,
                    paneSlotID: pane.paneID
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .attentionInbox:
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    items: attentionInboxItems(from: state)
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .routingCandidates:
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    candidates: routingCandidates(from: state, preferredPaneID: command.paneID)
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .terminalRenderMetrics,
            .agentActivity,
            .performanceDiagnosticsSetEnabled,
            .performanceDiagnosticsExportRecent, .performanceDiagnosticsRecordChildPressure,
            .eventsRead:
            return nil
        }
    }

    private static func hostReadableSnapshotResult(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) -> AlanShellLocalCommandResult {
        let contentState = state.contentStateProjection()
        let localResponse: AlanShellControlResponse
        switch command.command {
        case .state:
            localResponse = response(
                for: command,
                state: state,
                applied: true,
                snapshot: state,
                paneSlots: contentState.paneSlots,
                contents: contentState.contents
            )
        case .spaceList:
            localResponse = response(
                for: command,
                state: state,
                applied: true,
                spaces: state.spaces
            )
        case .tabList:
            let tabs = command.spaceID.flatMap { spaceID in
                state.space(spaceID: spaceID)?.tabs
            } ?? state.spaces.flatMap(\.tabs)
            localResponse = response(
                for: command,
                state: state,
                applied: true,
                tabs: tabs,
                spaceID: command.spaceID
            )
        case .paneList:
            let panes = command.tabID.map { tabID in
                state.panes.filter { $0.tabID == tabID }
            } ?? state.panes
            localResponse = response(
                for: command,
                state: state,
                applied: true,
                panes: panes,
                paneSlots: contentState.controlPlanePaneSlots(in: command.tabID),
                contents: contentState.controlPlaneContents(in: command.tabID),
                tabID: command.tabID
            )
        default:
            return shellCoreRoutingFailureResult(command: command, state: state)
        }
        return AlanShellLocalCommandResult(
            response: localResponse,
            updatedState: nil,
            sideEffect: nil
        )
    }

    private static func failureResponse(
        for error: ShellStateMutationError,
        command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) -> AlanShellControlResponse {
        switch error {
        case .spaceNotFound:
            return response(
                for: command,
                state: state,
                applied: false,
                spaceID: command.spaceID,
                errorCode: error.rawValue,
                errorMessage: "The requested space does not exist."
            )
        case .tabNotFound:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                errorCode: error.rawValue,
                errorMessage: "The requested tab does not exist."
            )
        case .paneNotFound:
            return response(
                for: command,
                state: state,
                applied: false,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "The requested pane does not exist."
            )
        case .unsupportedContent:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "This action requires terminal content."
            )
        case .splitNotFound:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "The requested split does not exist."
            )
        case .spatialFocusTargetNotFound:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "There is no pane in that direction."
            )
        case .lastTab:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                errorCode: error.rawValue,
                errorMessage: "alan terminal workspace must keep at least one tab open."
            )
        case .lastPane:
            return response(
                for: command,
                state: state,
                applied: false,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "This action requires the pane to have at least one sibling."
            )
        case .invalidMoveTarget:
            return response(
                for: command,
                state: state,
                applied: false,
                tabID: command.tabID,
                paneID: command.paneID,
                errorCode: error.rawValue,
                errorMessage: "The pane cannot be moved onto its current tab."
            )
        case .invalidTabOrganizationTarget:
            return response(
                for: command,
                state: state,
                applied: false,
                spaceID: command.spaceID,
                tabID: command.tabID,
                errorCode: error.rawValue,
                errorMessage: "The requested tab organization target is not available."
            )
        }
    }

    private static func shellCoreFailureResult(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        error: Error
    ) -> AlanShellLocalCommandResult {
        AlanShellLocalCommandResult(
            response: response(
                for: command,
                state: state,
                applied: false,
                errorCode: "shell_core_unavailable",
                errorMessage: "shell-core command failed: \(error)"
            ),
            updatedState: nil,
            sideEffect: nil
        )
    }

    private static func shellCoreRoutingFailureResult(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) -> AlanShellLocalCommandResult {
        AlanShellLocalCommandResult(
            response: response(
                for: command,
                state: state,
                applied: false,
                errorCode: "shell_core_routing_failed",
                errorMessage: "shell-core command routing failed."
            ),
            updatedState: nil,
            sideEffect: nil
        )
    }

    private static func mutationFailureResult(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        error: Error
    ) -> AlanShellLocalCommandResult {
        AlanShellLocalCommandResult(
            response: response(
                for: command,
                state: state,
                applied: false,
                errorCode: "shell_mutation_failed",
                errorMessage: "shell mutation failed: \(error)"
            ),
            updatedState: nil,
            sideEffect: nil
        )
    }

    private static func response(
        for command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        applied: Bool,
        snapshot: ShellStateSnapshot? = nil,
        spaces: [ShellSpace]? = nil,
        tabs: [ShellTab]? = nil,
        panes: [ShellPane]? = nil,
        paneSlots: [ShellPaneSlot]? = nil,
        contents: [ShellContentInstance]? = nil,
        pane: ShellPane? = nil,
        items: [AlanShellAttentionInboxItem]? = nil,
        candidates: [AlanShellRoutingCandidate]? = nil,
        events: [AlanShellEventEnvelope]? = nil,
        spaceID: String? = nil,
        sourceSpaceID: String? = nil,
        targetSpaceID: String? = nil,
        tabID: String? = nil,
        paneID: String? = nil,
        paneSlotID: String? = nil,
        contentID: String? = nil,
        contentKind: ShellContentKind? = nil,
        contentTitle: String? = nil,
        contentCapabilities: [ShellContentCapability]? = nil,
        section: ShellTabOrganizationSection? = nil,
        index: Int? = nil,
        acceptedBytes: Int? = nil,
        deliveryCode: String? = nil,
        runtimePhase: String? = nil,
        latestEventID: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) -> AlanShellControlResponse {
        let contentState = state.contentStateProjection()
        let contentProjection = contentState.controlPlaneContentProjection(
            paneSlotID: paneSlotID ?? paneID,
            contentID: contentID
        )
        return AlanShellControlResponse(
            requestID: command.requestID,
            contractVersion: contentState.contractVersion,
            applied: applied,
            state: snapshot,
            spaces: spaces,
            tabs: tabs,
            panes: panes,
            paneSlots: paneSlots,
            contents: contents,
            pane: pane,
            items: items,
            candidates: candidates,
            events: events,
            focusedPaneID: state.focusedPaneID,
            focusedPaneSlotID: contentState.focusedPaneSlotID,
            spaceID: spaceID,
            sourceSpaceID: sourceSpaceID,
            targetSpaceID: targetSpaceID,
            tabID: tabID,
            paneID: paneID,
            paneSlotID: paneSlotID ?? contentProjection.paneSlotID,
            contentID: contentID ?? contentProjection.contentID,
            contentKind: contentKind ?? contentProjection.kind,
            contentTitle: contentTitle ?? contentProjection.title,
            contentCapabilities: contentCapabilities ?? contentProjection.capabilities,
            section: section,
            index: index,
            acceptedBytes: acceptedBytes,
            deliveryCode: deliveryCode,
            runtimePhase: runtimePhase,
            latestEventID: latestEventID,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }
}

private extension AlanShellLocalCommandResult {
    init(
        shellCoreResult: ShellCoreControlCommandResult,
        command: AlanShellControlCommand
    ) {
        let updatedState = shellCoreResult.updatedState.map { state in
            guard command.command == .paneFocus,
                  shellCoreResult.response.applied == true,
                  let tabID = state.focusedTabID,
                  let paneID = state.focusedPaneID
            else {
                return state
            }
            // Rust owns workspace focus. Command-failure activity is still host-projected, so
            // apply its acknowledgement once at the shared Apple executor boundary. This keeps
            // in-process, socket, and file-poll callers on identical state semantics.
            return state.acknowledgingCommandFailureActivities(
                in: tabID,
                focusedPaneID: paneID
            )
        }
        self.init(
            response: shellCoreResult.response,
            updatedState: updatedState,
            sideEffect: shellCoreResult.sideEffect.map(AlanShellLocalCommandSideEffect.init)
        )
    }
}

private extension AlanShellLocalCommandSideEffect {
    init(_ sideEffect: ShellCoreControlSideEffect) {
        switch sideEffect {
        case .sendText(let paneSlotID, let contentID, let text):
            self = .sendText(
                paneSlotID: paneSlotID,
                contentID: contentID,
                text: text
            )
        case .sendKey(let paneSlotID, let contentID, let key):
            self = .sendKey(
                paneSlotID: paneSlotID,
                contentID: contentID,
                key: key
            )
        }
    }
}

private extension AlanShellControlCommand {
    func resolvingShellCoreDefaults(in state: ShellStateSnapshot) -> AlanShellControlCommand {
        switch command {
        case .spaceCreate:
            let resolvedTerminalProfileID = terminalProfileID
            return withShellCoreDefaults(terminalProfileID: resolvedTerminalProfileID)

        case .tabOpen:
            let resolvedTerminalProfileID = state.terminalProfileIDForNewTerminal(
                in: spaceID,
                explicit: terminalProfileID
            )
            let resolvedCWD = state.workingDirectoryForNewTerminal(
                from: state.focusedPaneID,
                explicit: cwd,
                resolvedTerminalProfileID: resolvedTerminalProfileID
            )
            return withShellCoreDefaults(
                terminalProfileID: resolvedTerminalProfileID,
                cwd: resolvedCWD
            )

        case .tabPin, .tabUnpin:
            return withShellCoreDefaults(tabID: tabID ?? state.focusedTabID)

        case .paneSplit:
            let sourcePaneID = paneSlotID ?? paneID
            let resolvedTerminalProfileID = sourcePaneID.flatMap {
                state.terminalProfileIDForNewSplit(from: $0, explicit: terminalProfileID)
            }
                ?? terminalProfileID
            let resolvedCWD = state.workingDirectoryForNewTerminal(
                from: sourcePaneID,
                explicit: cwd,
                resolvedTerminalProfileID: resolvedTerminalProfileID
            )
            return withShellCoreDefaults(
                terminalProfileID: resolvedTerminalProfileID,
                cwd: resolvedCWD
            )

        case .paneZoom:
            let resolvedPaneID = paneID ?? (paneSlotID == nil ? state.focusedPaneID : nil)
            return withShellCoreDefaults(paneID: resolvedPaneID)

        case .paneUnzoom:
            let resolvedTabID = tabID
                ?? (paneSlotID ?? paneID).flatMap { state.pane(paneID: $0)?.tabID }
                ?? state.focusedTabID
            return withShellCoreDefaults(tabID: resolvedTabID)

        default:
            return self
        }
    }

    func withShellCoreDefaults(
        tabID resolvedTabID: String? = nil,
        paneID resolvedPaneID: String? = nil,
        terminalProfileID: String? = nil,
        cwd resolvedCWD: String? = nil
    ) -> AlanShellControlCommand {
        AlanShellControlCommand(
            requestID: requestID,
            command: command,
            spaceID: spaceID,
            targetSpaceID: targetSpaceID,
            tabID: resolvedTabID ?? tabID,
            paneID: resolvedPaneID ?? paneID,
            paneSlotID: paneSlotID,
            contentID: contentID,
            splitNodeID: splitNodeID,
            ratio: ratio,
            section: section,
            index: index,
            direction: direction,
            spatialDirection: spatialDirection,
            placement: placement,
            title: title,
            cwd: resolvedCWD ?? cwd,
            text: text,
            key: key,
            attention: attention,
            agentKind: agentKind,
            agentStatus: agentStatus,
            sessionLabel: sessionLabel,
            projectLabel: projectLabel,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID ?? self.terminalProfileID,
            detail: detail,
            updatedAt: updatedAt,
            afterEventID: afterEventID,
            limit: limit,
            enabled: enabled,
            exportDirectory: exportDirectory,
            childProcessRole: childProcessRole,
            childCPUPercent: childCPUPercent,
            childMemoryBytes: childMemoryBytes,
            childThreadCount: childThreadCount
        )
    }
}

private extension AlanShellControlCommandKind {
    var isShellCoreLocalCommandSupported: Bool {
        switch self {
        case .state,
             .spaceList,
             .spaceCreate,
             .tabList,
             .tabOpen,
             .tabClose,
             .tabReorder,
             .tabPin,
             .tabUnpin,
             .tabMoveToSpace,
             .paneList,
             .paneSplit,
             .paneClose,
             .paneLift,
             .paneMove,
             .paneMoveWithinTab,
             .paneFocus,
             .paneSpatialFocus,
             .paneResizeSplit,
             .paneEqualizeSplits,
             .paneZoom,
             .paneUnzoom,
             .terminalSendText,
             .terminalSendKey,
             .attentionSet:
            return true
        case .spaceSetTerminalProfile,
             .paneSnapshot,
             .terminalRenderMetrics,
             .agentActivity,
             .attentionInbox,
             .routingCandidates,
             .eventsRead,
             .performanceDiagnosticsSetEnabled,
             .performanceDiagnosticsExportRecent,
             .performanceDiagnosticsRecordChildPressure:
            return false
        }
    }
}

extension AlanShellControlCommandKind {
    var canFallThroughToHostWhenShellCoreUnavailable: Bool {
        switch self {
        case .state,
             .spaceList,
             .tabList,
             .paneList:
            return true
        case .spaceCreate,
             .spaceSetTerminalProfile,
             .tabOpen,
             .tabClose,
             .tabReorder,
             .tabPin,
             .tabUnpin,
             .tabMoveToSpace,
             .paneSplit,
             .paneSnapshot,
             .paneClose,
             .paneLift,
             .paneMove,
             .paneMoveWithinTab,
             .paneFocus,
             .paneSpatialFocus,
             .paneResizeSplit,
             .paneEqualizeSplits,
             .paneZoom,
             .paneUnzoom,
             .terminalSendText,
             .terminalSendKey,
             .terminalRenderMetrics,
             .agentActivity,
             .attentionSet,
             .attentionInbox,
             .routingCandidates,
             .eventsRead,
             .performanceDiagnosticsSetEnabled,
             .performanceDiagnosticsExportRecent,
             .performanceDiagnosticsRecordChildPressure:
            return false
        }
    }
}

private func attentionInboxItems(from state: ShellStateSnapshot) -> [AlanShellAttentionInboxItem] {
    let now = Date()
    return state.panes
        .map { (pane: $0, attention: shellEffectiveAttention(for: $0, now: now)) }
        .filter { $0.attention != .idle }
        .sorted {
            attentionRank(for: $0.attention) == attentionRank(for: $1.attention)
                ? $0.pane.paneID < $1.pane.paneID
                : attentionRank(for: $0.attention) > attentionRank(for: $1.attention)
        }
        .map { item in
            let pane = item.pane
            return AlanShellAttentionInboxItem(
                itemID: "attn_\(pane.paneID)",
                spaceID: pane.spaceID,
                tabID: pane.tabID,
                paneID: pane.paneID,
                attention: item.attention,
                summary: pane.viewport?.summary
                    ?? pane.alanBinding.map { $0.pendingRequest ? "Alan is waiting for user input" : "Alan Machine state: \($0.machineState)" }
                    ?? pane.process?.program
                    ?? "Activity detected"
            )
        }
}

private func routingCandidates(
    from state: ShellStateSnapshot,
    preferredPaneID: String?
) -> [AlanShellRoutingCandidate] {
    let preferredPane = preferredPaneID.flatMap(state.pane(paneID:))
    let focusedPane = state.focusedPaneID.flatMap(state.pane(paneID:))
    let now = Date()

    return state.panes.map { pane in
        var score = 0.0
        var reasons: [String] = []
        let attention = shellEffectiveAttention(for: pane, now: now)

        if pane.paneID == preferredPaneID {
            score += 0.4
            reasons.append("requested")
        }
        if pane.paneID == state.focusedPaneID {
            score += 0.3
            reasons.append("focused")
        }
        if attention == .awaitingUser {
            score += 0.25
            reasons.append("attention:awaiting_user")
        } else if attention == .notable {
            score += 0.12
            reasons.append("attention:notable")
        }
        if pane.alanBinding?.pendingRequest == true {
            score += 0.2
            reasons.append("alan_binding:yielded")
        } else if let machineState = pane.alanBinding?.machineState {
            score += 0.08
            reasons.append("alan_binding:\(machineState)")
        }
        if let preferredPane, pane.tabID == preferredPane.tabID {
            score += 0.1
            reasons.append("same_tab")
        } else if let focusedPane, pane.tabID == focusedPane.tabID {
            score += 0.08
            reasons.append("same_tab")
        }
        if let preferredPane, pane.spaceID == preferredPane.spaceID {
            score += 0.05
            reasons.append("same_space")
        } else if let focusedPane, pane.spaceID == focusedPane.spaceID {
            score += 0.04
            reasons.append("same_space")
        }
        if let process = pane.process?.program {
            reasons.append("process:\(process)")
        }

        return AlanShellRoutingCandidate(
            paneID: pane.paneID,
            score: min(score, 1.0),
            reasons: Array(Set(reasons)).sorted()
        )
    }
    .sorted {
        $0.score == $1.score ? $0.paneID < $1.paneID : $0.score > $1.score
    }
}

private func attentionRank(for attention: ShellAttentionState) -> Int {
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

#endif
