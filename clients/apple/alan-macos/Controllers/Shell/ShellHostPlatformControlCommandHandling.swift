import Foundation

#if os(macOS)
@MainActor
extension ShellHostController {
    func handleControlPlaneCommand(_ command: AlanShellControlCommand) -> AlanShellControlResponse {
        if let closeGuardResponse = controlPlaneCloseGuardResponse(for: command) {
            return closeGuardResponse
        }
        guard let localResult = AlanShellLocalCommandExecutor.execute(
            command: command,
            state: shellState,
            context: AlanShellLocalCommandExecutionContext(
                reservedPaneSlotIDs: terminalRuntimeRegistry.registeredPaneIDs.sorted()
            )
        ) else {
            return handlePlatformControlCommand(command)
        }

        let previousState = shellState
        if let updatedState = localResult.updatedState {
            let pinSnapshotTabIDs = pinSnapshotTabIDs(
                for: command,
                result: localResult
            )
            adoptStateFromControlPlane(updatedState, publish: pinSnapshotTabIDs.isEmpty)
            if !pinSnapshotTabIDs.isEmpty {
                publishControlPlaneState(pinSnapshotTabIDs: pinSnapshotTabIDs)
            }
        }
        recordPortableControlEffects(
            for: command,
            result: localResult,
            previousState: previousState
        )
        var localResponse = localResult.response
        enrichPortableControlResponse(
            &localResponse,
            for: command,
            result: localResult
        )
        guard let sideEffect = localResult.sideEffect else {
            return localResponse
        }
        return handleLocalCommandSideEffect(
            sideEffect,
            command: command,
            response: localResponse
        )
    }

    private func pinSnapshotTabIDs(
        for command: AlanShellControlCommand,
        result: AlanShellLocalCommandResult
    ) -> Set<String> {
        guard result.response.applied == true,
              let tabID = command.tabID ?? result.response.tabID,
              result.updatedState?.tab(tabID: tabID)?.isPinned == true
        else {
            return []
        }
        switch command.command {
        case .tabPin, .tabReorder:
            return [tabID]
        default:
            return []
        }
    }

    private func enrichPortableControlResponse(
        _ response: inout AlanShellControlResponse,
        for command: AlanShellControlCommand,
        result: AlanShellLocalCommandResult
    ) {
        if command.command == .state {
            response.terminalRenderMetrics = terminalRuntimeRegistry.renderCoordinatorMetrics
        }

        let reportsLatestEventID = switch command.command {
        case .paneSpatialFocus:
            true
        case .paneResizeSplit, .paneEqualizeSplits, .paneZoom, .paneUnzoom, .paneMoveWithinTab:
            result.response.applied == true
        default:
            false
        }
        if reportsLatestEventID {
            response.latestEventID = controlPlane.latestEventID
        }
    }

    private func controlPlaneCloseGuardResponse(
        for command: AlanShellControlCommand
    ) -> AlanShellControlResponse? {
        let guardedTarget: (tabID: String?, paneID: String?)?
        switch command.command {
        case .tabClose:
            guard let tabID = command.tabID,
                  closeGuardImpact(for: .tab(tabID)) != nil
            else {
                return nil
            }
            guardedTarget = (tabID, nil)
        case .paneClose:
            guard let paneID = command.paneSlotID ?? command.paneID,
                  closeGuardImpact(for: .paneSlot(paneID)) != nil
            else {
                return nil
            }
            guardedTarget = (shellState.pane(paneID: paneID)?.tabID, paneID)
        default:
            return nil
        }

        return response(
            requestID: command.requestID,
            applied: false,
            tabID: guardedTarget?.tabID,
            paneID: guardedTarget?.paneID,
            errorCode: "requires_confirmation",
            errorMessage: "The requested close contains active terminal work and requires confirmation."
        )
    }

    private func handleLocalCommandSideEffect(
        _ sideEffect: AlanShellLocalCommandSideEffect,
        command: AlanShellControlCommand,
        response localResponse: AlanShellControlResponse
    ) -> AlanShellControlResponse {
        let paneSlotID: String
        let contentID: String
        let delivery: TerminalRuntimeDeliveryResult
        switch sideEffect {
        case .sendText(let targetPaneSlotID, let targetContentID, let text):
            paneSlotID = targetPaneSlotID
            contentID = targetContentID
            delivery = terminalRuntimeRegistry.sendText(
                toTerminalContentID: targetContentID,
                text: text
            )
            controlPlane.recordTextDelivery(
                requestID: command.requestID,
                spaceID: localResponse.spaceID,
                tabID: localResponse.tabID,
                paneID: targetPaneSlotID,
                contentID: targetContentID,
                delivery: delivery
            )
        case .sendKey(let targetPaneSlotID, let targetContentID, let key):
            paneSlotID = targetPaneSlotID
            contentID = targetContentID
            delivery = terminalRuntimeRegistry.sendKey(
                toTerminalContentID: targetContentID,
                key: key
            )
        }

        return response(
            requestID: command.requestID,
            applied: delivery.applied,
            spaceID: localResponse.spaceID,
            tabID: localResponse.tabID,
            paneID: paneSlotID,
            paneSlotID: paneSlotID,
            contentID: contentID,
            contentKind: localResponse.contentKind,
            contentTitle: localResponse.contentTitle,
            contentCapabilities: localResponse.contentCapabilities,
            acceptedBytes: delivery.acceptedBytes,
            deliveryCode: delivery.code.rawValue,
            runtimePhase: delivery.runtimePhase,
            errorCode: delivery.errorCode,
            errorMessage: delivery.errorMessage
        )
    }

    private func recordPortableControlEffects(
        for command: AlanShellControlCommand,
        result: AlanShellLocalCommandResult,
        previousState: ShellStateSnapshot
    ) {
        switch command.command {
        case .paneEqualizeSplits:
            guard result.response.applied == true,
                  let tabID = command.tabID ?? result.response.tabID,
                  let previousTab = previousState.tab(tabID: tabID),
                  let currentTab = shellState.tab(tabID: tabID)
            else {
                return
            }
            let changedSplitIDs = currentTab.paneTree.splitNodeIDsWithChangedRatios(
                comparedTo: previousTab.paneTree
            )
            guard !changedSplitIDs.isEmpty else { return }
            controlPlane.recordSplitEqualized(
                requestID: command.requestID,
                spaceID: result.response.spaceID,
                tabID: tabID,
                changedSplitIDs: changedSplitIDs,
                affectedPaneIDs: currentTab.paneTree.paneIDs
            )
        case .paneZoom:
            guard result.response.applied == true,
                  let tabID = result.response.tabID,
                  let paneID = result.response.zoomedPaneID
            else {
                return
            }
            zoomedPaneIDByTabID[tabID] = paneID
            controlPlane.recordZoomStateChanged(
                requestID: command.requestID,
                spaceID: result.response.spaceID,
                tabID: tabID,
                paneID: paneID,
                zoomedPaneID: paneID
            )
            synchronizeTerminalRenderPriorities()
        case .paneUnzoom:
            guard result.response.applied == true else { return }
            let requestedPane = command.paneID.flatMap { previousState.pane(paneID: $0) }
            guard let tabID = command.tabID ?? requestedPane?.tabID ?? result.response.tabID else {
                return
            }
            let previousZoomedPaneID = previousState.zoomedPaneIDByTabID[tabID]
            controlPlane.recordZoomStateChanged(
                requestID: command.requestID,
                spaceID: requestedPane?.spaceID ?? result.response.spaceID,
                tabID: tabID,
                paneID: previousZoomedPaneID,
                zoomedPaneID: nil
            )
            synchronizeTerminalRenderPriorities()
        case .paneSpatialFocus:
            guard let direction = command.spatialDirection else { return }
            controlPlane.recordSpatialFocus(
                requestID: command.requestID,
                spaceID: result.response.spaceID,
                tabID: result.response.tabID,
                previousPaneID: previousState.focusedPaneID,
                currentPaneID: shellState.focusedPaneID,
                direction: direction,
                applied: result.response.applied == true
            )
        case .paneMoveWithinTab:
            guard result.response.applied == true,
                  let paneID = command.paneSlotID ?? command.paneID,
                  let sourcePane = previousState.pane(paneID: paneID),
                  let placement = command.placement
            else {
                return
            }
            controlPlane.recordPaneMovedInTab(
                requestID: command.requestID,
                spaceID: sourcePane.spaceID,
                tabID: sourcePane.tabID,
                paneID: paneID,
                placement: placement,
                mountedContentInstanceID: paneID
            )
        case .terminalSendText:
            guard result.response.errorCode == "unsupported_content",
                  let paneSlotID = result.response.paneSlotID,
                  let contentID = result.response.contentID,
                  let content = previousState.contentStateProjection().content(contentID: contentID)
            else {
                return
            }
            controlPlane.recordContentCommandRejected(
                requestID: command.requestID,
                command: command.command,
                spaceID: result.response.spaceID,
                tabID: result.response.tabID,
                paneSlotID: paneSlotID,
                content: content,
                errorCode: "unsupported_content",
                errorMessage: result.response.errorMessage ?? "terminal.send_text requires terminal content."
            )
        default:
            break
        }
    }

    private func handlePlatformControlCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        switch command.command {
        case .terminalRenderMetrics:
            return response(
                requestID: command.requestID,
                applied: true,
                terminalRenderMetrics: terminalRuntimeRegistry.renderCoordinatorMetrics
            )

        case .eventsRead:
            return controlPlane.specialCommandResponse(for: command)
                ?? response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "events_unavailable",
                    errorMessage: "events.read is handled by the shell control plane."
                )

        case .performanceDiagnosticsSetEnabled:
            guard let enabled = command.enabled else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "diagnostics_enabled_required",
                    errorMessage: "enabled is required."
                )
            }
            AlanPerformanceDiagnosticsController.shared.setEnabled(enabled)
            let summary = AlanPerformanceDiagnosticsController.shared.summarySnapshot()
            return response(
                requestID: command.requestID,
                applied: true,
                diagnosticsEnabled: AlanPerformanceDiagnosticsController.shared.isEnabled,
                diagnosticsRetainedEventCount: AlanPerformanceDiagnosticsController.shared
                    .eventsSnapshot().count,
                diagnosticsStutterMarkerCount: summary.stutterMarkerCount
            )

        case .performanceDiagnosticsExportRecent:
            guard let exportDirectory = command.exportDirectory?
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !exportDirectory.isEmpty
            else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "diagnostics_export_directory_required",
                    errorMessage: "export_directory is required."
                )
            }
            do {
                let bundleURL = try AlanPerformanceDiagnosticsController.shared.exportRecentDiagnostics(
                    to: URL(fileURLWithPath: exportDirectory, isDirectory: true),
                    appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString")
                        as? String ?? "unknown",
                    installChannel: AlanInstallChannel.current().installChannelID
                )
                let summary = AlanPerformanceDiagnosticsController.shared.summarySnapshot()
                return response(
                    requestID: command.requestID,
                    applied: true,
                    diagnosticsEnabled: AlanPerformanceDiagnosticsController.shared.isEnabled,
                    diagnosticsRetainedEventCount: AlanPerformanceDiagnosticsController.shared
                        .eventsSnapshot().count,
                    diagnosticsStutterMarkerCount: summary.stutterMarkerCount,
                    diagnosticsBundlePath: bundleURL.path
                )
            } catch {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    errorCode: "diagnostics_export_failed",
                    errorMessage: error.localizedDescription
                )
            }

        case .performanceDiagnosticsRecordChildPressure:
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    diagnosticsEnabled: false,
                    errorCode: "diagnostics_disabled",
                    errorMessage: "Performance diagnostics are disabled."
                )
            }
            guard let cpuPercent = command.childCPUPercent else {
                return response(
                    requestID: command.requestID,
                    applied: false,
                    diagnosticsEnabled: true,
                    errorCode: "diagnostics_child_cpu_required",
                    errorMessage: "child_cpu_percent is required."
                )
            }

            let role = command.childProcessRole?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            switch role {
            case nil, "", "terminal_child", "terminalChild":
                AlanPerformanceDiagnosticsController.shared.recordKnownTerminalChildProcesses(
                    [
                        AlanPerformanceChildProcessObservation(
                            processID: 0,
                            cpuPercent: cpuPercent,
                            memoryBytes: command.childMemoryBytes,
                            threadCount: command.childThreadCount
                        )
                    ]
                )
            case "unknown_child", "unknownChild":
                AlanPerformanceDiagnosticsController.shared.recordUnknownChildPressure(
                    cpuPercent: cpuPercent,
                    memoryBytes: command.childMemoryBytes,
                    threadCount: command.childThreadCount
                )
            default:
                return response(
                    requestID: command.requestID,
                    applied: false,
                    diagnosticsEnabled: true,
                    errorCode: "diagnostics_child_role_unknown",
                    errorMessage: "child_process_role must be terminal_child or unknown_child."
                )
            }

            let summary = AlanPerformanceDiagnosticsController.shared.summarySnapshot()
            return response(
                requestID: command.requestID,
                applied: true,
                diagnosticsEnabled: true,
                diagnosticsRetainedEventCount: AlanPerformanceDiagnosticsController.shared
                    .eventsSnapshot().count,
                diagnosticsStutterMarkerCount: summary.stutterMarkerCount
            )

        default:
            return response(
                requestID: command.requestID,
                applied: false,
                errorCode: "unsupported_platform_command",
                errorMessage: "The command is not owned by the platform command handler."
            )
        }
    }
}
#endif
