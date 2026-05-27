import Foundation

#if os(macOS)
private struct TerminalSendTextTarget {
    let paneSlot: ShellPaneSlot
    let content: ShellContentInstance
}

private extension ShellContentStateSnapshot {
    func terminalSendTextTarget(paneSlotID: String) -> TerminalSendTextTarget? {
        guard let paneSlot = paneSlot(paneSlotID: paneSlotID),
              let content = content(contentID: paneSlot.contentID)
        else {
            return nil
        }
        return TerminalSendTextTarget(paneSlot: paneSlot, content: content)
    }

    func terminalSendTextTarget(contentID: String) -> TerminalSendTextTarget? {
        guard let content = content(contentID: contentID),
              let paneSlot = paneSlots.first(where: { $0.contentID == contentID })
        else {
            return nil
        }
        return TerminalSendTextTarget(paneSlot: paneSlot, content: content)
    }
}

@MainActor
extension ShellHostController {
    func attentionInboxRows() -> [AlanShellAttentionInboxItem] {
        attentionItems.map { item in
            AlanShellAttentionInboxItem(
                itemID: "attn_\(item.paneID)",
                spaceID: item.spaceID,
                tabID: item.tabID,
                paneID: item.paneID,
                attention: item.attention,
                summary: item.summary
            )
        }
    }

    func routingCandidates(preferredPaneID: String?) -> [AlanShellRoutingCandidate] {
        let preferredPane = preferredPaneID.flatMap { pane(paneID: $0) }
        let focusedPane = self.focusedPane
        let now = Date()

        return shellState.panes.map { candidate in
            var score = 0.0
            var reasons: [String] = []
            let attention = shellEffectiveAttention(for: candidate, now: now)

            if candidate.paneID == preferredPaneID {
                score += 0.4
                reasons.append("requested")
            }
            if candidate.paneID == shellState.focusedPaneID {
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
            if candidate.alanBinding?.pendingYield == true {
                score += 0.2
                reasons.append("alan_binding:yielded")
            } else if let runStatus = candidate.alanBinding?.runStatus {
                score += 0.08
                reasons.append("alan_binding:\(runStatus)")
            }
            if let preferredPane, candidate.tabID == preferredPane.tabID {
                score += 0.1
                reasons.append("same_tab")
            } else if let focusedPane, candidate.tabID == focusedPane.tabID {
                score += 0.08
                reasons.append("same_tab")
            }
            if let preferredPane, candidate.spaceID == preferredPane.spaceID {
                score += 0.05
                reasons.append("same_space")
            } else if let focusedPane, candidate.spaceID == focusedPane.spaceID {
                score += 0.04
                reasons.append("same_space")
            }
            if let process = candidate.process?.program {
                reasons.append("process:\(process)")
            }

            return AlanShellRoutingCandidate(
                paneID: candidate.paneID,
                score: min(score, 1.0),
                reasons: Array(Set(reasons)).sorted()
            )
        }
        .sorted {
            $0.score == $1.score ? $0.paneID < $1.paneID : $0.score > $1.score
        }
    }

    func paneList(tabID: String?) -> [ShellPane] {
        guard let tabID else {
            return shellState.panes
        }
        return shellState.panes.filter { $0.tabID == tabID }
    }

    func tabList(spaceID: String?) -> [ShellTab] {
        if let spaceID {
            return shellState.spaces.first(where: { $0.spaceID == spaceID })?.tabs ?? []
        }
        return shellState.spaces.flatMap(\.tabs)
    }

    func response(
        requestID: String,
        applied: Bool,
        state: ShellStateSnapshot? = nil,
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
        terminalRenderMetrics: TerminalRenderCoordinatorMetrics? = nil,
        latestEventID: String? = nil,
        splitNodeID: String? = nil,
        ratio: Double? = nil,
        changedSplitIDs: [String]? = nil,
        affectedPaneIDs: [String]? = nil,
        zoomedPaneID: String? = nil,
        sourceTabID: String? = nil,
        targetTabID: String? = nil,
        previousFocusedPaneID: String? = nil,
        currentFocusedPaneID: String? = nil,
        previousFocusedPaneSlotID: String? = nil,
        currentFocusedPaneSlotID: String? = nil,
        splitDirection: ShellSplitDirection? = nil,
        spatialDirection: ShellSpatialFocusDirection? = nil,
        placement: ShellPaneSplitDirection? = nil,
        mountedContentInstanceID: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) -> AlanShellControlResponse {
        let contentState = shellState.contentStateProjection()
        let contentProjection = contentState.controlPlaneContentProjection(
            paneSlotID: paneSlotID ?? paneID,
            contentID: contentID
        )
        return AlanShellControlResponse(
            requestID: requestID,
            contractVersion: contentState.contractVersion,
            applied: applied,
            state: state,
            spaces: spaces,
            tabs: tabs,
            panes: panes,
            paneSlots: paneSlots,
            contents: contents,
            pane: pane,
            items: items,
            candidates: candidates,
            events: events,
            focusedPaneID: shellState.focusedPaneID,
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
            terminalRenderMetrics: terminalRenderMetrics,
            latestEventID: latestEventID,
            splitNodeID: splitNodeID,
            ratio: ratio,
            changedSplitIDs: changedSplitIDs,
            affectedPaneIDs: affectedPaneIDs,
            zoomedPaneID: zoomedPaneID,
            sourceTabID: sourceTabID,
            targetTabID: targetTabID,
            previousFocusedPaneID: previousFocusedPaneID,
            currentFocusedPaneID: currentFocusedPaneID,
            previousFocusedPaneSlotID: previousFocusedPaneSlotID ?? previousFocusedPaneID,
            currentFocusedPaneSlotID: currentFocusedPaneSlotID ?? currentFocusedPaneID,
            splitDirection: splitDirection,
            spatialDirection: spatialDirection,
            placement: placement,
            mountedContentInstanceID: contentProjection.contentID ?? mountedContentInstanceID,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    func terminalDeliveryResult(
        from result: ShellAutomationCommandResult
    ) -> TerminalRuntimeDeliveryResult {
        let code = result.deliveryCode.flatMap(TerminalRuntimeDeliveryCode.init(rawValue:))
            ?? terminalDeliveryCode(for: result.code)
        return TerminalRuntimeDeliveryResult(
            code: code,
            acceptedBytes: result.acceptedBytes ?? 0,
            runtimePhase: result.runtimePhase,
            errorCode: result.errorCode,
            errorMessage: result.errorMessage
        )
    }

    private func terminalDeliveryCode(
        for code: ShellAutomationCommandResultCode
    ) -> TerminalRuntimeDeliveryCode {
        switch code {
        case .accepted:
            return .accepted
        case .queued:
            return .queued
        case .rejected:
            return .rejected
        case .missingTarget:
            return .missingTarget
        case .runtimeUnavailable:
            return .unavailableRuntime
        case .timeout:
            return .timeout
        case .invalidRequest, .unsupportedContent, .requiresConfirmation, .lastPane, .lastTab:
            return .rejected
        }
    }

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
                workingDirectory: command.cwd
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
                        workingDirectory: command.cwd
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
            guard let paneID = command.paneID else {
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
                        placement: .defaultPlacement(for: direction)
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

        case .terminalRenderMetrics:
            return response(
                requestID: command.requestID,
                applied: true,
                terminalRenderMetrics: terminalRuntimeRegistry.renderCoordinatorMetrics
            )

        case .agentActivity:
            guard let paneID = command.paneID else {
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
            guard let event = command.agentActivityEvent,
                  let activity = TerminalAgentActivityAdapter.activity(from: event)
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "invalid_agent_activity",
                    errorMessage: "agent_kind and a supported agent_status are required."
                )
            }

            updateTerminalMetadata(
                TerminalPaneMetadataSnapshot(
                    title: nil,
                    workingDirectory: event.workingDirectory,
                    summary: nil,
                    attention: .idle,
                    processExited: false,
                    lastCommandExitCode: nil,
                    lastUpdatedAt: Date(),
                    activeTaskState: nil,
                    activity: activity
                ),
                for: paneID
            )
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: targetPane.spaceID,
                tabID: targetPane.tabID,
                paneID: paneID
            )

        case .attentionInbox:
            return response(
                requestID: command.requestID,
                applied: true,
                items: attentionInboxRows()
            )

        case .attentionSet:
            guard let paneID = command.paneID,
                  let attention = command.attention
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "attention_target_required",
                    errorMessage: "pane_id and attention are required."
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
            guard setAttention(attention, for: paneID) else {
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
                spaceID: targetPane.spaceID,
                tabID: targetPane.tabID,
                paneID: paneID
            )

        case .routingCandidates:
            return response(
                requestID: command.requestID,
                applied: true,
                candidates: routingCandidates(preferredPaneID: command.paneID)
            )

        case .quickTerminalToggle:
            guard let paneID = toggleQuickTerminal() else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "quick_terminal_unavailable",
                    errorMessage: "The quick terminal could not be toggled."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                paneID: paneID
            )

        case .quickTerminalShow:
            guard let paneID = showQuickTerminal() else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "quick_terminal_unavailable",
                    errorMessage: "The quick terminal could not be shown."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                paneID: paneID
            )

        case .quickTerminalHide:
            guard hideQuickTerminal() else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "quick_terminal_not_found",
                    errorMessage: "The quick terminal does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                paneID: shellState.quickTerminal?.paneID
            )

        case .quickTerminalFocus:
            guard let paneID = focusQuickTerminal() else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "quick_terminal_unavailable",
                    errorMessage: "The quick terminal could not be focused."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                paneID: paneID
            )

        case .quickTerminalClose:
            let paneID = shellState.quickTerminal?.paneID
            if let impact = closeGuardImpact(for: .quickTerminal) {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    contentID: impact.activeTerminalContentIDs.first
                        ?? impact.affectedTerminalContentIDs.first,
                    errorCode: "requires_confirmation",
                    errorMessage: "The requested close contains active terminal work and requires confirmation."
                )
            }
            guard closeQuickTerminal() else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    paneID: paneID,
                    errorCode: "quick_terminal_not_found",
                    errorMessage: "The quick terminal does not exist."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: shellState.focusedPaneID
            )

        case .quickTerminalPromote:
            guard let targetSpaceID = command.targetSpaceID ?? command.spaceID else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "quick_terminal_destination_required",
                    errorMessage: "target_space_id is required."
                )
            }
            let paneID = shellState.quickTerminal?.paneID
            guard promoteQuickTerminal(to: targetSpaceID) else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    targetSpaceID: targetSpaceID,
                    paneID: paneID,
                    errorCode: "quick_terminal_promote_failed",
                    errorMessage: "The quick terminal could not be moved to the target space."
                )
            }
            return response(
                requestID: command.requestID,
                applied: true,
                targetSpaceID: targetSpaceID,
                paneID: paneID
            )

        case .eventsRead:
            return controlPlane.specialCommandResponse(for: command)
                ?? response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "events_unavailable",
                    errorMessage: "events.read is handled by the shell control plane."
                )
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
            let result = try shellState.resizingSplit(splitNodeID, ratio: ratio)
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
            let result = try shellState.equalizingSplits(in: tabID)
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
            let result = try shellState.focusingAdjacentPane(direction)
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
            let result = try shellState.movingPaneWithinTab(paneID, placement: placement)
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
