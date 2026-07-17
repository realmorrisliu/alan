import Foundation

#if os(macOS)
@MainActor
extension ShellHostController {
    func handleControlPlaneCommand(_ command: AlanShellControlCommand) -> AlanShellControlResponse {
        switch command.command {
        case .state:
            let contentState = shellState.contentStateProjection()
            return response(
                requestID: command.requestID,
                applied: true,
                state: shellState,
                paneSlots: contentState.paneSlots,
                contents: contentState.contents,
                paneSlotID: contentState.focusedPaneSlotID,
                terminalRenderMetrics: terminalRuntimeRegistry.renderCoordinatorMetrics
            )

        case .spaceList:
            return response(
                requestID: command.requestID,
                applied: true,
                spaces: shellState.spaces
            )

        case .spaceCreate:
            let failureMessage = "Failed to create a new shell space."
            guard let spaceID = createSpace(
                launchTarget: .shell,
                title: command.title,
                workingDirectory: command.cwd,
                terminalProfileID: command.terminalProfileID
            ) else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "space_create_failed",
                    errorMessage: failureMessage
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: spaceID,
                paneID: shellState.focusedPaneID
            )

        case .spaceSetTerminalProfile:
            guard let spaceID = command.spaceID ?? shellState.focusedSpaceID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "space_required",
                    errorMessage: "space_id is required."
                )
            }
            guard setTerminalProfile(command.terminalProfileID, forSpaceID: spaceID) else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: spaceID,
                    errorCode: "space_not_found",
                    errorMessage: "The requested space does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: spaceID,
                paneID: shellState.focusedPaneID
            )

        case .tabList:
            return response(
                requestID: command.requestID,
                applied: true,
                tabs: tabList(spaceID: command.spaceID),
                spaceID: command.spaceID
            )

        case .tabOpen:
            let result = performShellAutomationCommand(
                .createTab(
                    ShellAutomationCreateTabRequest(
                        launchTarget: .shell,
                        spaceID: command.spaceID,
                        title: command.title,
                        workingDirectory: command.cwd,
                        terminalProfileID: command.terminalProfileID
                    )
                )
            )
            guard result.applied else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: result.spaceID ?? command.spaceID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: result.tabID,
                paneID: result.paneID
            )

        case .tabClose:
            guard let tabID = command.tabID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "tab_required",
                    errorMessage: "tab_id is required."
                )
            }

            let result = performShellAutomationCommand(.closeTab(tabID: tabID))
            switch result.code {
            case .accepted:
                return response(
                    requestID: command.requestID,
                    applied: true,
                    tabID: result.tabID ?? tabID,
                    paneID: result.paneID
                )
            case .missingTarget:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            case .lastTab:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            case .queued, .rejected, .invalidRequest, .unsupportedContent, .runtimeUnavailable,
                    .requiresConfirmation, .timeout, .lastPane:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: result.errorCode ?? result.code.rawValue,
                    errorMessage: result.errorMessage
                )
            }

        case .tabPin:
            let tabID = command.tabID ?? selectedTabID
            guard let tabID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "tab_required",
                    errorMessage: "tab_id is required."
                )
            }
            let sourceLocation = shellState.tabOrganizationLocation(tabID: tabID)
            guard pinTab(tabID: tabID),
                  let currentLocation = shellState.tabOrganizationLocation(tabID: tabID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: "tab_not_found",
                    errorMessage: "The requested tab does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: currentLocation.spaceID,
                sourceSpaceID: sourceLocation?.spaceID,
                tabID: tabID,
                paneID: shellState.focusedPaneID,
                section: currentLocation.section,
                index: currentLocation.index
            )

        case .tabUnpin:
            let tabID = command.tabID ?? selectedTabID
            guard let tabID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "tab_required",
                    errorMessage: "tab_id is required."
                )
            }
            let sourceLocation = shellState.tabOrganizationLocation(tabID: tabID)
            guard unpinTab(tabID: tabID),
                  let currentLocation = shellState.tabOrganizationLocation(tabID: tabID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: "tab_not_found",
                    errorMessage: "The requested tab does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: currentLocation.spaceID,
                sourceSpaceID: sourceLocation?.spaceID,
                tabID: tabID,
                paneID: shellState.focusedPaneID,
                section: currentLocation.section,
                index: currentLocation.index
            )

        case .tabReorder:
            guard let tabID = command.tabID,
                  let section = command.section,
                  let index = command.index
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: command.tabID,
                    errorCode: "tab_reorder_target_required",
                    errorMessage: "tab_id, section, and index are required."
                )
            }
            let sourceLocation = shellState.tabOrganizationLocation(tabID: tabID)
            guard reorderTab(
                tabID: tabID,
                targetSpaceID: command.spaceID,
                section: section,
                index: index
            ),
            let currentLocation = shellState.tabOrganizationLocation(tabID: tabID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: command.spaceID,
                    tabID: tabID,
                    section: section,
                    index: index,
                    errorCode: "invalid_tab_organization_target",
                    errorMessage: "The requested tab organization target is not available."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: currentLocation.spaceID,
                sourceSpaceID: sourceLocation?.spaceID,
                targetSpaceID: command.spaceID,
                tabID: tabID,
                paneID: shellState.focusedPaneID,
                section: currentLocation.section,
                index: currentLocation.index
            )

        case .tabMoveToSpace:
            guard let tabID = command.tabID,
                  let targetSpaceID = command.targetSpaceID ?? command.spaceID
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: command.tabID,
                    errorCode: "tab_move_target_required",
                    errorMessage: "tab_id and target_space_id are required."
                )
            }
            let sourceLocation = shellState.tabOrganizationLocation(tabID: tabID)
            guard moveTabToSpace(tabID: tabID, targetSpaceID: targetSpaceID),
                  let currentLocation = shellState.tabOrganizationLocation(tabID: tabID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    targetSpaceID: targetSpaceID,
                    tabID: tabID,
                    errorCode: "invalid_move_target",
                    errorMessage: "The requested tab could not be moved to the target space."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: currentLocation.spaceID,
                sourceSpaceID: sourceLocation?.spaceID,
                targetSpaceID: targetSpaceID,
                tabID: tabID,
                paneID: shellState.focusedPaneID,
                section: currentLocation.section,
                index: currentLocation.index
            )

        case .paneList:
            let contentState = shellState.contentStateProjection()
            return response(
                requestID: command.requestID,
                applied: true,
                panes: paneList(tabID: command.tabID),
                paneSlots: contentState.controlPlanePaneSlots(in: command.tabID),
                contents: contentState.controlPlaneContents(in: command.tabID),
                tabID: command.tabID
            )

        case .paneSnapshot:
            guard let paneID = command.paneID,
                  let pane = pane(paneID: paneID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: command.paneID,
                    errorCode: "pane_not_found",
                    errorMessage: "The requested pane does not exist."
                )
            }

            return response(
                requestID: command.requestID,
                applied: true,
                pane: pane,
                spaceID: pane.spaceID,
                tabID: pane.tabID,
                paneID: pane.paneID,
                paneSlotID: pane.paneID
            )

        case .paneSplit:
            guard let paneID = command.paneSlotID ?? command.paneID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "pane_required",
                    errorMessage: "pane_id is required."
                )
            }
            guard let direction = command.direction else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "direction_required",
                    errorMessage: "direction is required for pane.split."
                )
            }
            let result = performShellAutomationCommand(
                .splitPane(
                    ShellAutomationPaneSplitRequest(
                        paneID: paneID,
                        placement: .defaultPlacement(for: direction),
                        title: command.title,
                        workingDirectory: command.cwd,
                        terminalProfileID: command.terminalProfileID
                    )
                )
            )
            guard result.applied,
                  let newPaneID = result.paneID
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                state: shellState,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: newPaneID,
                paneSlotID: newPaneID
            )

        case .paneClose:
            guard let paneID = command.paneID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "pane_required",
                    errorMessage: "pane_id is required."
                )
            }

            let result = performShellAutomationCommand(.closePane(paneID: paneID))
            switch result.code {
            case .accepted:
                return response(
                    requestID: command.requestID,
                    applied: true,
                    spaceID: result.spaceID,
                    tabID: result.tabID,
                    paneID: result.paneID
                )
            case .missingTarget:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            case .lastTab:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: result.errorCode,
                    errorMessage: result.errorMessage
                )
            case .queued, .rejected, .invalidRequest, .unsupportedContent, .runtimeUnavailable,
                    .requiresConfirmation, .timeout, .lastPane:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: result.errorCode ?? result.code.rawValue,
                    errorMessage: result.errorMessage
                )
            }

        case .paneLift:
            guard let paneID = command.paneID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "pane_required",
                    errorMessage: "pane_id is required."
                )
            }

            switch liftPaneToTab(paneID: paneID, title: command.title) {
            case .lifted:
                return response(
                    requestID: command.requestID,
                    applied: true,
                    spaceID: shellState.focusedSpaceID,
                    tabID: shellState.focusedTabID,
                    paneID: shellState.focusedPaneID
                )
            case .paneNotFound:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "pane_not_found",
                    errorMessage: "The requested pane does not exist."
                )
            case .lastPane:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "last_pane",
                    errorMessage: "The pane needs at least one sibling before it can be lifted."
                )
            }

        case .paneMove:
            guard let paneID = command.paneID,
                  let targetTabID = command.tabID
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: command.tabID,
                    paneID: command.paneID,
                    errorCode: "pane_move_target_required",
                    errorMessage: "pane_id and tab_id are required."
                )
            }

            let direction = command.direction ?? .vertical
            let sourcePane = pane(paneID: paneID)
            guard movePane(paneID: paneID, toTab: targetTabID, direction: direction) else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: targetTabID,
                    paneID: paneID,
                    errorCode: "invalid_move_target",
                    errorMessage: "The requested pane could not be moved to the target tab."
                )
            }

            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: shellState.focusedPaneID,
                sourceTabID: sourcePane?.tabID,
                targetTabID: targetTabID,
                splitDirection: direction,
                mountedContentInstanceID: paneID
            )

        case .paneMoveWithinTab:
            return handlePaneMoveWithinTabCommand(command)

        case .paneFocus:
            guard let paneID = command.paneID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: command.paneID,
                    errorCode: "pane_required",
                    errorMessage: "pane_id is required."
                )
            }

            let result = performShellAutomationCommand(.focusPane(paneID: paneID))
            guard result.applied else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "pane_not_found",
                    errorMessage: "The requested pane does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: result.tabID,
                paneID: result.paneID
            )

        case .paneSpatialFocus:
            return handlePaneSpatialFocusCommand(command)

        case .paneResizeSplit:
            return handlePaneResizeSplitCommand(command)

        case .paneEqualizeSplits:
            return handlePaneEqualizeSplitsCommand(command)

        case .paneZoom:
            return handlePaneZoomCommand(command)

        case .paneUnzoom:
            return handlePaneUnzoomCommand(command)

        case .terminalSendText:
            let contentState = shellState.contentStateProjection()
            let requestedPaneSlotID = command.paneSlotID ?? command.paneID
            let target: TerminalSendTextTarget?
            if let contentID = command.contentID {
                target = contentState.terminalSendTextTarget(contentID: contentID)
            } else if let requestedPaneSlotID {
                target = contentState.terminalSendTextTarget(paneSlotID: requestedPaneSlotID)
            } else {
                target = nil
            }

            guard let target else {
                let errorCode: String
                if command.contentID == nil && requestedPaneSlotID == nil {
                    errorCode = "terminal_target_required"
                } else if command.contentID != nil {
                    errorCode = "content_not_found"
                } else {
                    errorCode = "pane_not_found"
                }
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: command.paneID,
                    paneSlotID: command.paneSlotID,
                    contentID: command.contentID,
                    errorCode: errorCode,
                    errorMessage: "terminal.send_text requires an existing terminal content target."
                )
            }

            guard target.content.kind == .terminal else {
                let errorCode = "unsupported_content"
                let errorMessage = "terminal.send_text requires terminal content."
                controlPlane.recordContentCommandRejected(
                    requestID: command.requestID,
                    command: command.command,
                    spaceID: target.paneSlot.spaceID,
                    tabID: target.paneSlot.tabID,
                    paneSlotID: target.paneSlot.paneSlotID,
                    content: target.content,
                    errorCode: errorCode,
                    errorMessage: errorMessage
                )
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: target.paneSlot.spaceID,
                    tabID: target.paneSlot.tabID,
                    paneID: target.paneSlot.paneSlotID,
                    paneSlotID: target.paneSlot.paneSlotID,
                    contentID: target.content.contentID,
                    errorCode: errorCode,
                    errorMessage: errorMessage
                )
            }

            let result = performShellAutomationCommand(
                .sendText(
                    ShellAutomationSendTextRequest(
                        paneID: target.paneSlot.paneSlotID,
                        terminalContentID: target.content.contentID,
                        text: command.text ?? ""
                    )
                )
            )
            let delivery = terminalDeliveryResult(from: result)
            controlPlane.recordTextDelivery(
                requestID: command.requestID,
                spaceID: target.paneSlot.spaceID,
                tabID: target.paneSlot.tabID,
                paneID: target.paneSlot.paneSlotID,
                contentID: target.content.contentID,
                delivery: delivery
            )

            return response(
                requestID: command.requestID,
                applied: result.applied,
                spaceID: target.paneSlot.spaceID,
                tabID: target.paneSlot.tabID,
                paneID: target.paneSlot.paneSlotID,
                paneSlotID: target.paneSlot.paneSlotID,
                contentID: target.content.contentID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .terminalSendKey:
            let contentState = shellState.contentStateProjection()
            let requestedPaneSlotID = command.paneSlotID ?? command.paneID
            let target: TerminalSendTextTarget?
            if let contentID = command.contentID {
                target = contentState.terminalSendTextTarget(contentID: contentID)
            } else if let requestedPaneSlotID {
                target = contentState.terminalSendTextTarget(paneSlotID: requestedPaneSlotID)
            } else {
                target = nil
            }

            guard let target else {
                let errorCode: String
                if command.contentID == nil && requestedPaneSlotID == nil {
                    errorCode = "terminal_target_required"
                } else if command.contentID != nil {
                    errorCode = "content_not_found"
                } else {
                    errorCode = "pane_not_found"
                }
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: command.paneID,
                    paneSlotID: command.paneSlotID,
                    contentID: command.contentID,
                    errorCode: errorCode,
                    errorMessage: "terminal.send_key requires an existing terminal content target."
                )
            }

            guard target.content.kind == .terminal else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: target.paneSlot.spaceID,
                    tabID: target.paneSlot.tabID,
                    paneID: target.paneSlot.paneSlotID,
                    paneSlotID: target.paneSlot.paneSlotID,
                    contentID: target.content.contentID,
                    contentKind: target.content.kind,
                    errorCode: "unsupported_content",
                    errorMessage: "terminal.send_key requires terminal content."
                )
            }

            let keyName = command.key?.trimmingCharacters(in: .whitespacesAndNewlines)
            let key: TerminalRuntimeControlKey?
            switch keyName {
            case "return", "enter":
                key = .returnKey
            default:
                key = nil
            }
            guard let key else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    spaceID: target.paneSlot.spaceID,
                    tabID: target.paneSlot.tabID,
                    paneID: target.paneSlot.paneSlotID,
                    paneSlotID: target.paneSlot.paneSlotID,
                    contentID: target.content.contentID,
                    errorCode: "terminal_key_unsupported",
                    errorMessage: "terminal.send_key currently supports return."
                )
            }

            let result = performShellAutomationCommand(
                .sendKey(
                    ShellAutomationSendKeyRequest(
                        paneID: target.paneSlot.paneSlotID,
                        terminalContentID: target.content.contentID,
                        key: key
                    )
                )
            )
            let delivery = terminalDeliveryResult(from: result)
            return response(
                requestID: command.requestID,
                applied: result.applied,
                spaceID: target.paneSlot.spaceID,
                tabID: target.paneSlot.tabID,
                paneID: target.paneSlot.paneSlotID,
                paneSlotID: target.paneSlot.paneSlotID,
                contentID: target.content.contentID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .terminalRenderMetrics,
                .agentActivity,
                .attentionInbox,
                .attentionSet,
                .routingCandidates,
                .eventsRead,
                .performanceDiagnosticsSetEnabled,
                .performanceDiagnosticsExportRecent,
                .performanceDiagnosticsRecordChildPressure:
            return handleObservationControlCommand(command)
        }
    }

    private func handlePaneResizeSplitCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        guard let splitNodeID = command.splitNodeID else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "split_node_required",
                errorMessage: "split_node_id is required."
            )
        }
        guard let ratio = command.ratio else {
            return response(
                requestID: command.requestID,
                applied: false,
                splitNodeID: splitNodeID,
                errorCode: "ratio_required",
                errorMessage: "ratio is required."
            )
        }
        guard let targetTab = shellState.spaces
            .flatMap(\.tabs)
            .first(where: { $0.paneTree.contains(nodeID: splitNodeID) })
        else {
            return response(
                requestID: command.requestID,
                applied: false,
                splitNodeID: splitNodeID,
                errorCode: "split_not_found",
                errorMessage: "The requested split does not exist."
            )
        }

        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .resizeSplit(splitNodeID: splitNodeID, ratio: ratio)
            )
            guard let updatedSplit = result.state.tab(tabID: targetTab.tabID)?
                .paneTree
                .node(nodeID: splitNodeID)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    splitNodeID: splitNodeID,
                    errorCode: "split_not_found",
                    errorMessage: "The requested split does not exist."
                )
            }

            let affectedPaneIDs = updatedSplit.paneIDs
            applyMutationResult(result)
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: targetTab.tabID,
                paneID: affectedPaneIDs.first ?? result.paneID,
                latestEventID: controlPlane.latestEventID,
                splitNodeID: splitNodeID,
                ratio: updatedSplit.splitRatio,
                changedSplitIDs: [splitNodeID],
                affectedPaneIDs: affectedPaneIDs
            )
        } catch {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: targetTab.tabID,
                splitNodeID: splitNodeID,
                errorCode: "split_not_found",
                errorMessage: "The requested split does not exist."
            )
        }
    }

    private func handlePaneEqualizeSplitsCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        let tabID = command.tabID ?? selectedTabID
        guard let tabID else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "tab_required",
                errorMessage: "tab_id is required."
            )
        }
        guard let tab = shellState.tab(tabID: tabID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: tabID,
                errorCode: "tab_not_found",
                errorMessage: "The requested tab does not exist."
            )
        }
        guard !tab.paneTree.splitNodes.isEmpty else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: tabID,
                errorCode: "no_split_branches",
                errorMessage: "The requested tab does not have split branches."
            )
        }

        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .equalizeSplits(tabID: tabID)
            )
            guard let updatedTab = result.state.tab(tabID: tabID) else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    errorCode: "tab_not_found",
                    errorMessage: "The requested tab does not exist."
                )
            }
            let changedSplitIDs = updatedTab.paneTree.splitNodeIDsWithChangedRatios(
                comparedTo: tab.paneTree
            )
            guard !changedSplitIDs.isEmpty else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    tabID: tabID,
                    ratio: 0.5,
                    changedSplitIDs: [],
                    affectedPaneIDs: tab.paneTree.paneIDs,
                    errorCode: "unchanged_state",
                    errorMessage: "The requested split ratios are already equalized."
                )
            }

            let affectedPaneIDs = updatedTab.paneTree.paneIDs
            applyMutationResult(result)
            controlPlane.recordSplitEqualized(
                requestID: command.requestID,
                spaceID: result.spaceID,
                tabID: tabID,
                changedSplitIDs: changedSplitIDs,
                affectedPaneIDs: affectedPaneIDs
            )
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: tabID,
                paneID: affectedPaneIDs.first ?? result.paneID,
                latestEventID: controlPlane.latestEventID,
                ratio: 0.5,
                changedSplitIDs: changedSplitIDs,
                affectedPaneIDs: affectedPaneIDs
            )
        } catch {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: tabID,
                errorCode: "tab_not_found",
                errorMessage: "The requested tab does not exist."
            )
        }
    }

    private func handlePaneZoomCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        let paneID = command.paneID ?? selectedPane?.paneID
        guard let paneID else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "pane_required",
                errorMessage: "pane_id is required."
            )
        }
        guard let targetPane = pane(paneID: paneID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                paneID: paneID,
                errorCode: "pane_not_found",
                errorMessage: "The requested pane does not exist."
            )
        }
        guard canZoomPane(paneID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: targetPane.tabID,
                paneID: paneID,
                errorCode: "split_tab_required",
                errorMessage: "Pane zoom requires a split tab."
            )
        }

        let previousFocusedPaneID = shellState.focusedPaneID
        guard zoomPane(paneID: paneID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: targetPane.tabID,
                paneID: paneID,
                zoomedPaneID: zoomedPaneIDByTabID[targetPane.tabID],
                previousFocusedPaneID: previousFocusedPaneID,
                currentFocusedPaneID: shellState.focusedPaneID,
                mountedContentInstanceID: paneID,
                errorCode: "unchanged_state",
                errorMessage: "The requested pane is already zoomed."
            )
        }

        return response(
            requestID: command.requestID,
            applied: true,
            spaceID: targetPane.spaceID,
            tabID: targetPane.tabID,
            paneID: paneID,
            latestEventID: controlPlane.latestEventID,
            zoomedPaneID: paneID,
            previousFocusedPaneID: previousFocusedPaneID,
            currentFocusedPaneID: shellState.focusedPaneID,
            mountedContentInstanceID: paneID
        )
    }

    private func handlePaneUnzoomCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        let requestedPane = command.paneID.flatMap(pane)
        if command.paneID != nil && requestedPane == nil {
            return response(
                requestID: command.requestID,
                applied: false,
                paneID: command.paneID,
                errorCode: "pane_not_found",
                errorMessage: "The requested pane does not exist."
            )
        }
        let tabID = command.tabID
            ?? requestedPane?.tabID
            ?? selectedTabID
        guard let tabID else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "tab_required",
                errorMessage: "tab_id is required."
            )
        }
        guard shellState.tab(tabID: tabID) != nil else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: tabID,
                errorCode: "tab_not_found",
                errorMessage: "The requested tab does not exist."
            )
        }

        let previousFocusedPaneID = shellState.focusedPaneID
        let previousZoomedPaneID = zoomedPaneIDByTabID[tabID]
        guard unzoomTab(tabID: tabID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                tabID: tabID,
                zoomedPaneID: nil,
                previousFocusedPaneID: previousFocusedPaneID,
                currentFocusedPaneID: shellState.focusedPaneID,
                errorCode: "unchanged_state",
                errorMessage: "The requested tab is not zoomed."
            )
        }

        return response(
            requestID: command.requestID,
            applied: true,
            tabID: tabID,
            paneID: previousZoomedPaneID,
            latestEventID: controlPlane.latestEventID,
            zoomedPaneID: nil,
            previousFocusedPaneID: previousFocusedPaneID,
            currentFocusedPaneID: shellState.focusedPaneID,
            mountedContentInstanceID: previousZoomedPaneID
        )
    }

    private func handlePaneSpatialFocusCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        guard let direction = command.spatialDirection else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "spatial_direction_required",
                errorMessage: "spatial_direction is required."
            )
        }

        let previousFocusedPaneID = shellState.focusedPaneID
        let previousPane = previousFocusedPaneID.flatMap { pane(paneID: $0) }
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .focusAdjacentPane(direction: direction)
            )
            applyMutationResult(result)
            controlPlane.recordSpatialFocus(
                requestID: command.requestID,
                spaceID: result.spaceID,
                tabID: result.tabID,
                previousPaneID: previousFocusedPaneID,
                currentPaneID: result.paneID,
                direction: direction,
                applied: true
            )
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: result.tabID,
                paneID: result.paneID,
                latestEventID: controlPlane.latestEventID,
                previousFocusedPaneID: previousFocusedPaneID,
                currentFocusedPaneID: result.paneID,
                spatialDirection: direction
            )
        } catch ShellStateMutationError.spatialFocusTargetNotFound {
            controlPlane.recordSpatialFocus(
                requestID: command.requestID,
                spaceID: previousPane?.spaceID,
                tabID: previousPane?.tabID,
                previousPaneID: previousFocusedPaneID,
                currentPaneID: previousFocusedPaneID,
                direction: direction,
                applied: false
            )
            return response(
                requestID: command.requestID,
                applied: false,
                spaceID: previousPane?.spaceID,
                tabID: previousPane?.tabID,
                paneID: previousFocusedPaneID,
                latestEventID: controlPlane.latestEventID,
                previousFocusedPaneID: previousFocusedPaneID,
                currentFocusedPaneID: previousFocusedPaneID,
                spatialDirection: direction,
                errorCode: "spatial_focus_target_not_found",
                errorMessage: "There is no pane in that direction."
            )
        } catch {
            return response(
                requestID: command.requestID,
                applied: false,
                paneID: previousFocusedPaneID,
                errorCode: "pane_not_found",
                errorMessage: "The focused pane does not exist."
            )
        }
    }

    private func handlePaneMoveWithinTabCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        guard let paneID = command.paneID else {
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "pane_required",
                errorMessage: "pane_id is required."
            )
        }
        guard let placement = command.placement else {
            return response(
                requestID: command.requestID,
                applied: false,
                paneID: paneID,
                errorCode: "placement_required",
                errorMessage: "placement is required."
            )
        }
        guard let sourcePane = pane(paneID: paneID) else {
            return response(
                requestID: command.requestID,
                applied: false,
                paneID: paneID,
                placement: placement,
                errorCode: "pane_not_found",
                errorMessage: "The requested pane does not exist."
            )
        }

        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .movePaneWithinTab(paneSlotID: paneID, placement: placement)
            )
            applyMutationResult(result)
            controlPlane.recordPaneMovedInTab(
                requestID: command.requestID,
                spaceID: result.spaceID,
                tabID: sourcePane.tabID,
                paneID: paneID,
                placement: placement,
                mountedContentInstanceID: paneID
            )
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: result.spaceID,
                tabID: sourcePane.tabID,
                paneID: paneID,
                latestEventID: controlPlane.latestEventID,
                sourceTabID: sourcePane.tabID,
                targetTabID: sourcePane.tabID,
                placement: placement,
                mountedContentInstanceID: paneID
            )
        } catch {
            return response(
                requestID: command.requestID,
                applied: false,
                spaceID: sourcePane.spaceID,
                tabID: sourcePane.tabID,
                paneID: paneID,
                sourceTabID: sourcePane.tabID,
                targetTabID: sourcePane.tabID,
                placement: placement,
                mountedContentInstanceID: paneID,
                errorCode: "invalid_move_target",
                errorMessage: "The requested in-tab pane movement target is not available."
            )
        }
    }
}
#endif
