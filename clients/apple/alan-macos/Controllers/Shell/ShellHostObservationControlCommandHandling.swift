import Foundation

#if os(macOS)
@MainActor
extension ShellHostController {
    func handleObservationControlCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        switch command.command {
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
                errorCode: "unsupported_observation_command",
                errorMessage: "The command is not owned by the observation command handler."
            )
        }
    }
}
#endif
