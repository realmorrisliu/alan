import Foundation

#if os(macOS)
enum AlanShellLocalCommandSideEffect {
    case sendText(paneID: String, text: String)
}

struct AlanShellLocalCommandResult {
    let response: AlanShellControlResponse
    let updatedState: ShellStateSnapshot?
    let sideEffect: AlanShellLocalCommandSideEffect?
}

enum AlanShellLocalCommandExecutor {
    static func execute(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) -> AlanShellLocalCommandResult? {
        if command.command.isShellCoreLocalCommandSupported {
            let shellCoreCommand = command.resolvingShellCoreDefaults(in: state)
            guard let result = try? ShellCoreFFIAdapter.shared.handleControlCommand(
                shellCoreCommand,
                state: state
            ) else {
                return nil
            }
            return AlanShellLocalCommandResult(shellCoreResult: result)
        }

        switch command.command {
        case .state:
            let contentState = state.contentStateProjection()
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    snapshot: state,
                    paneSlots: contentState.paneSlots,
                    contents: contentState.contents,
                    spaceID: state.focusedSpaceID,
                    tabID: state.focusedTabID,
                    paneID: state.focusedPaneID,
                    paneSlotID: contentState.focusedPaneSlotID
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .spaceList:
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    spaces: state.spaces,
                    spaceID: command.spaceID ?? state.focusedSpaceID
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .spaceCreate:
            let resolvedTerminalProfileID = command.terminalProfileID
                ?? terminalProfileIDForGlobalDefaultPaneCapture()
            guard let result = try? ShellCoreFFIAdapter.shared.applyReducer(
                state: state,
                operation: .createTerminalSpace(
                    title: command.title,
                    tabTitle: nil,
                    workingDirectory: command.cwd,
                    terminalProfileID: resolvedTerminalProfileID,
                    presentationIcon: nil,
                    reservedPaneSlotIDs: []
                )
            ) else {
                return nil
            }
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: result.state,
                    applied: true,
                    spaceID: result.spaceID,
                    tabID: result.tabID,
                    paneID: result.paneID
                ),
                updatedState: result.state,
                sideEffect: nil
            )

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
            guard let result = try? ShellCoreFFIAdapter.shared.applyReducer(
                state: state,
                operation: .setTerminalProfile(
                    spaceID: spaceID,
                    terminalProfileID: command.terminalProfileID
                )
            ) else {
                return nil
            }
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

        case .tabList:
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    tabs: state.tabs(in: command.spaceID),
                    spaceID: command.spaceID ?? state.focusedSpaceID,
                    tabID: state.focusedTabID
                ),
                updatedState: nil,
                sideEffect: nil
            )

        case .tabOpen:
            do {
                let resolvedTerminalProfileID = state.terminalProfileIDForNewTerminal(
                    in: command.spaceID,
                    explicit: command.terminalProfileID
                )
                    ?? terminalProfileIDForGlobalDefaultPaneCapture()
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .openTerminalTab(
                        spaceID: command.spaceID,
                        title: command.title,
                        workingDirectory: command.cwd,
                        terminalProfileID: resolvedTerminalProfileID,
                        reservedPaneSlotIDs: []
                    )
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .tabClose:
            guard let tabID = command.tabID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        tabID: command.tabID,
                        errorCode: "tab_required",
                        errorMessage: "tab_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }

            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .closeTab(tabID: tabID)
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .tabPin:
            guard let tabID = command.tabID ?? state.focusedTabID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "tab_required",
                        errorMessage: "tab_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .pinTab(tabID: tabID)
                )
                let location = result.state.tabOrganizationLocation(tabID: tabID)
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: location?.spaceID,
                        tabID: tabID,
                        paneID: result.state.focusedPaneID,
                        section: location?.section,
                        index: location?.index
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .tabUnpin:
            guard let tabID = command.tabID ?? state.focusedTabID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "tab_required",
                        errorMessage: "tab_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .unpinTab(tabID: tabID)
                )
                let location = result.state.tabOrganizationLocation(tabID: tabID)
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: location?.spaceID,
                        tabID: tabID,
                        paneID: result.state.focusedPaneID,
                        section: location?.section,
                        index: location?.index
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .tabReorder:
            guard let tabID = command.tabID,
                  let section = command.section,
                  let index = command.index
            else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        tabID: command.tabID,
                        errorCode: "tab_reorder_target_required",
                        errorMessage: "tab_id, section, and index are required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .organizeTab(
                        tabID: tabID,
                        targetSpaceID: command.spaceID,
                        section: section,
                        index: index
                    )
                )
                let location = result.state.tabOrganizationLocation(tabID: tabID)
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: location?.spaceID,
                        tabID: tabID,
                        paneID: result.state.focusedPaneID,
                        section: location?.section,
                        index: location?.index
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .tabMoveToSpace:
            guard let tabID = command.tabID,
                  let targetSpaceID = command.targetSpaceID ?? command.spaceID
            else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        tabID: command.tabID,
                        errorCode: "tab_move_target_required",
                        errorMessage: "tab_id and target_space_id are required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .moveTabToSpace(tabID: tabID, targetSpaceID: targetSpaceID)
                )
                let location = result.state.tabOrganizationLocation(tabID: tabID)
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: location?.spaceID,
                        targetSpaceID: targetSpaceID,
                        tabID: tabID,
                        paneID: result.state.focusedPaneID,
                        section: location?.section,
                        index: location?.index
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneList:
            let contentState = state.contentStateProjection()
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: true,
                    panes: state.panes(in: command.tabID),
                    paneSlots: contentState.controlPlanePaneSlots(in: command.tabID),
                    contents: contentState.controlPlaneContents(in: command.tabID),
                    tabID: command.tabID ?? state.focusedTabID
                ),
                updatedState: nil,
                sideEffect: nil
            )

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

        case .paneSplit:
            guard let paneID = command.paneID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "pane_required",
                        errorMessage: "pane_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            guard let direction = command.direction else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        paneID: paneID,
                        errorCode: "direction_required",
                        errorMessage: "direction is required for pane.split."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let resolvedTerminalProfileID = state.terminalProfileIDForNewSplit(
                    from: paneID,
                    explicit: command.terminalProfileID
                )
                    ?? terminalProfileIDForGlobalDefaultPaneCapture()
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .splitPane(
                        paneSlotID: paneID,
                        placement: .defaultPlacement(for: direction),
                        title: nil,
                        workingDirectory: resolvedTerminalProfileID == nil
                            ? state.pane(paneID: paneID)?.cwd
                            : nil,
                        terminalProfileID: resolvedTerminalProfileID,
                        reservedPaneSlotIDs: []
                    )
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        snapshot: result.state,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID,
                        paneSlotID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneClose:
            guard let paneID = command.paneID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "pane_required",
                        errorMessage: "pane_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .closePane(paneSlotID: paneID)
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneLift:
            guard let paneID = command.paneID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "pane_required",
                        errorMessage: "pane_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .movePaneToNewTab(paneSlotID: paneID, title: command.title)
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneMove:
            guard let paneID = command.paneID,
                  let targetTabID = command.tabID
            else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        tabID: command.tabID,
                        paneID: command.paneID,
                        errorCode: "pane_move_target_required",
                        errorMessage: "pane_id and tab_id are required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .movePaneToTab(
                        paneSlotID: paneID,
                        targetTabID: targetTabID,
                        direction: command.direction ?? .vertical
                    )
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneFocus:
            guard let paneID = command.paneID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "pane_required",
                        errorMessage: "pane_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .focusPane(paneSlotID: paneID)
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

        case .paneMoveWithinTab, .paneSpatialFocus, .paneResizeSplit, .paneEqualizeSplits,
             .paneZoom, .paneUnzoom:
            return nil

        case .terminalSendText, .terminalRenderMetrics:
            return nil

        case .agentActivity:
            guard let paneID = command.paneID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "pane_required",
                        errorMessage: "pane_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            guard state.pane(paneID: paneID) != nil else {
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
            guard let event = command.agentActivityEvent,
                  let activity = TerminalAgentActivityAdapter.activity(from: event)
            else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        paneID: paneID,
                        errorCode: "invalid_agent_activity",
                        errorMessage: "agent_kind and a supported agent_status are required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }

            do {
                let result = try state.applyingAgentActivity(
                    activity,
                    to: paneID,
                    workingDirectory: event.workingDirectory
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

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

        case .attentionSet:
            guard let paneID = command.paneID,
                  let attention = command.attention
            else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "attention_target_required",
                        errorMessage: "pane_id and attention are required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            do {
                let result = try ShellCoreFFIAdapter.shared.applyReducer(
                    state: state,
                    operation: .setAttention(paneSlotID: paneID, attention: attention)
                )
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: result.state,
                        applied: true,
                        spaceID: result.spaceID,
                        tabID: result.tabID,
                        paneID: result.paneID
                    ),
                    updatedState: result.state,
                    sideEffect: nil
                )
            } catch let error as ShellStateMutationError {
                return AlanShellLocalCommandResult(
                    response: failureResponse(for: error, command: command, state: state),
                    updatedState: nil,
                    sideEffect: nil
                )
            } catch {
                return nil
            }

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

        case .quickTerminalToggle:
            if state.quickTerminal?.presentation == .visible {
                return quickTerminalResult(
                    command: command,
                    state: state,
                    mutate: {
                        try ShellCoreFFIAdapter.shared.applyReducer(
                            state: $0,
                            operation: .hideQuickTerminal
                        )
                    }
                )
            }
            return quickTerminalResult(
                command: command,
                state: state,
                mutate: {
                    try ShellCoreFFIAdapter.shared.applyReducer(
                        state: $0,
                        operation: .showQuickTerminal(
                            workingDirectory: command.cwd,
                            defaultWorkingDirectory: defaultShellWorkingDirectory()
                        )
                    )
                }
            )

        case .quickTerminalShow, .quickTerminalFocus:
            return quickTerminalResult(
                command: command,
                state: state,
                mutate: {
                    try ShellCoreFFIAdapter.shared.applyReducer(
                        state: $0,
                        operation: .showQuickTerminal(
                            workingDirectory: command.cwd,
                            defaultWorkingDirectory: defaultShellWorkingDirectory()
                        )
                    )
                }
            )

        case .quickTerminalHide:
            return quickTerminalResult(
                command: command,
                state: state,
                mutate: {
                    try ShellCoreFFIAdapter.shared.applyReducer(
                        state: $0,
                        operation: .hideQuickTerminal
                    )
                }
            )

        case .quickTerminalClose:
            return quickTerminalResult(
                command: command,
                state: state,
                mutate: {
                    try ShellCoreFFIAdapter.shared.applyReducer(
                        state: $0,
                        operation: .closeQuickTerminal
                    )
                }
            )

        case .quickTerminalPromote:
            guard let targetSpaceID = command.targetSpaceID ?? command.spaceID else {
                return AlanShellLocalCommandResult(
                    response: response(
                        for: command,
                        state: state,
                        applied: false,
                        errorCode: "quick_terminal_destination_required",
                        errorMessage: "target_space_id is required."
                    ),
                    updatedState: nil,
                    sideEffect: nil
                )
            }
            return quickTerminalResult(
                command: command,
                state: state,
                mutate: {
                    try ShellCoreFFIAdapter.shared.applyReducer(
                        state: $0,
                        operation: .promoteQuickTerminal(targetSpaceID: targetSpaceID)
                    )
                }
            )

        case .terminalSendKey, .performanceDiagnosticsSetEnabled,
            .performanceDiagnosticsExportRecent, .performanceDiagnosticsRecordChildPressure,
            .eventsRead:
            return nil
        }
    }

    private static func quickTerminalResult(
        command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        mutate: (ShellStateSnapshot) throws -> ShellStateMutationResult
    ) -> AlanShellLocalCommandResult {
        do {
            let result = try mutate(state)
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: result.state,
                    applied: true,
                    spaceID: result.spaceID,
                    tabID: result.tabID,
                    paneID: result.paneID
                ),
                updatedState: result.state,
                sideEffect: nil
            )
        } catch let error as ShellStateMutationError {
            return AlanShellLocalCommandResult(
                response: failureResponse(for: error, command: command, state: state),
                updatedState: nil,
                sideEffect: nil
            )
        } catch {
            return AlanShellLocalCommandResult(
                response: response(
                    for: command,
                    state: state,
                    applied: false,
                    errorCode: "quick_terminal_unavailable",
                    errorMessage: "The quick terminal command could not be applied."
                ),
                updatedState: nil,
                sideEffect: nil
            )
        }
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
    init(shellCoreResult: ShellCoreControlCommandResult) {
        self.init(
            response: shellCoreResult.response,
            updatedState: shellCoreResult.updatedState,
            sideEffect: shellCoreResult.sideEffect.map(AlanShellLocalCommandSideEffect.init)
        )
    }
}

private extension AlanShellLocalCommandSideEffect {
    init(_ sideEffect: ShellCoreControlSideEffect) {
        switch sideEffect {
        case .sendText(let paneID, let text):
            self = .sendText(paneID: paneID, text: text)
        }
    }
}

private extension AlanShellControlCommand {
    func resolvingShellCoreDefaults(in state: ShellStateSnapshot) -> AlanShellControlCommand {
        switch command {
        case .spaceCreate:
            let resolvedTerminalProfileID = terminalProfileID
                ?? terminalProfileIDForGlobalDefaultPaneCapture()
            return withTerminalProfileID(resolvedTerminalProfileID)

        case .tabOpen:
            let resolvedTerminalProfileID = state.terminalProfileIDForNewTerminal(
                in: spaceID,
                explicit: terminalProfileID
            )
                ?? terminalProfileIDForGlobalDefaultPaneCapture()
            return withTerminalProfileID(resolvedTerminalProfileID)

        case .paneSplit:
            let resolvedTerminalProfileID = paneID.flatMap {
                state.terminalProfileIDForNewSplit(from: $0, explicit: terminalProfileID)
            }
                ?? terminalProfileID
                ?? terminalProfileIDForGlobalDefaultPaneCapture()
            return withTerminalProfileID(resolvedTerminalProfileID)

        default:
            return self
        }
    }

    func withTerminalProfileID(_ terminalProfileID: String?) -> AlanShellControlCommand {
        AlanShellControlCommand(
            requestID: requestID,
            command: command,
            spaceID: spaceID,
            targetSpaceID: targetSpaceID,
            tabID: tabID,
            paneID: paneID,
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
            cwd: cwd,
            text: text,
            key: key,
            attention: attention,
            agentKind: agentKind,
            agentStatus: agentStatus,
            sessionLabel: sessionLabel,
            projectLabel: projectLabel,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID,
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
             .attentionSet:
            return true
        case .spaceSetTerminalProfile,
             .paneSnapshot,
             .terminalSendText,
             .terminalSendKey,
             .terminalRenderMetrics,
             .agentActivity,
             .attentionInbox,
             .routingCandidates,
             .eventsRead,
             .performanceDiagnosticsSetEnabled,
             .performanceDiagnosticsExportRecent,
             .performanceDiagnosticsRecordChildPressure,
             .quickTerminalToggle,
             .quickTerminalShow,
             .quickTerminalHide,
             .quickTerminalFocus,
             .quickTerminalClose,
             .quickTerminalPromote:
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
                    ?? pane.alanBinding.map { $0.pendingYield ? "alan is waiting for user input" : "alan run status: \($0.runStatus)" }
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
        if pane.alanBinding?.pendingYield == true {
            score += 0.2
            reasons.append("alan_binding:yielded")
        } else if let runStatus = pane.alanBinding?.runStatus {
            score += 0.08
            reasons.append("alan_binding:\(runStatus)")
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

private func defaultShellWorkingDirectory() -> String {
    FileManager.default.homeDirectoryForCurrentUser.path
}

#endif
