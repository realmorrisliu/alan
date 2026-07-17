import Foundation

#if os(macOS)
struct TerminalSendTextTarget {
    let paneSlot: ShellPaneSlot
    let content: ShellContentInstance
}

extension ShellContentStateSnapshot {
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
            if candidate.alanBinding?.pendingRequest == true {
                score += 0.2
                reasons.append("alan_binding:yielded")
            } else if let machineState = candidate.alanBinding?.machineState {
                score += 0.08
                reasons.append("alan_binding:\(machineState)")
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
        diagnosticsEnabled: Bool? = nil,
        diagnosticsRetainedEventCount: Int? = nil,
        diagnosticsStutterMarkerCount: Int? = nil,
        diagnosticsBundlePath: String? = nil,
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
            diagnosticsEnabled: diagnosticsEnabled,
            diagnosticsRetainedEventCount: diagnosticsRetainedEventCount,
            diagnosticsStutterMarkerCount: diagnosticsStutterMarkerCount,
            diagnosticsBundlePath: diagnosticsBundlePath,
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
}
#endif
