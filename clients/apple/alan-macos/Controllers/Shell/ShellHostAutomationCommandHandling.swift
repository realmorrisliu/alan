import Foundation

#if os(macOS)
extension ShellHostController: ShellAutomationCommandHandling {
    func performShellAutomationCommand(
        _ command: ShellAutomationCommand
    ) -> ShellAutomationCommandResult {
        switch command {
        case .createTab(let request):
            let result: ShellStateMutationResult
            do {
                switch request.launchTarget {
                case .shell:
                    result = try openTerminalTabMutation(
                        in: request.spaceID,
                        title: request.title,
                        workingDirectory: request.workingDirectory,
                        terminalProfileID: request.terminalProfileID
                    )
                }
            } catch let error as ShellStateMutationError {
                return shellAutomationResult(
                    code: .missingTarget,
                    spaceID: request.spaceID,
                    errorCode: error.rawValue,
                    errorMessage: shellStateMutationErrorMessage(error)
                )
            } catch {
                return shellAutomationResult(
                    code: .rejected,
                    spaceID: request.spaceID,
                    errorCode: "shell_mutation_failed",
                    errorMessage: String(describing: error)
                )
            }
            applyMutationResult(result)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: result.tabID,
                paneID: shellState.focusedPaneID
            )

        case .splitPane(let request):
            guard pane(paneID: request.paneID) != nil else {
                return shellAutomationMissingPaneResult(request.paneID)
            }
            // Carry explicit launch fields through a terminal content intent so a requested cwd
            // or title is honored instead of falling back to the source/default launch settings.
            let contentIntent: ShellContentIntent? =
                (request.title != nil || request.workingDirectory != nil)
                ? .terminal(
                    launchTarget: .shell,
                    title: request.title,
                    workingDirectory: request.workingDirectory
                )
                : nil
            guard let paneID = splitPane(
                paneID: request.paneID,
                placement: request.placement,
                contentIntent: contentIntent,
                terminalProfileID: request.terminalProfileID
            ) else {
                return shellAutomationMissingPaneResult(request.paneID)
            }
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )

        case .focusPane(let paneID):
            guard pane(paneID: paneID) != nil else {
                return shellAutomationMissingPaneResult(paneID)
            }
            focus(paneID: paneID)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )

        case .closePane(let paneID):
            switch closePane(paneID: paneID) {
            case .closed:
                return shellAutomationResult(
                    code: .accepted,
                    spaceID: shellState.focusedSpaceID,
                    tabID: shellState.focusedTabID,
                    paneID: shellState.focusedPaneID
                )
            case .paneNotFound:
                return shellAutomationMissingPaneResult(paneID)
            case .lastTab:
                return shellAutomationResult(
                    code: .lastTab,
                    paneID: paneID,
                    errorCode: "last_tab",
                    errorMessage: "alan terminal workspace must keep at least one pane open."
                )
            case .requiresConfirmation(let impact):
                return shellAutomationCloseRequiresConfirmationResult(
                    impact: impact,
                    tabID: shellState.pane(paneID: paneID)?.tabID,
                    paneID: paneID
                )
            }

        case .closeTab(let tabID):
            switch closeTab(tabID: tabID) {
            case .closed:
                return shellAutomationResult(
                    code: .accepted,
                    tabID: tabID,
                    paneID: shellState.focusedPaneID
                )
            case .tabNotFound:
                return shellAutomationResult(
                    code: .missingTarget,
                    tabID: tabID,
                    errorCode: "tab_not_found",
                    errorMessage: "The requested tab does not exist."
                )
            case .lastTab:
                return shellAutomationResult(
                    code: .lastTab,
                    tabID: tabID,
                    errorCode: "last_tab",
                    errorMessage: "alan terminal workspace must keep at least one tab open."
                )
            case .requiresConfirmation(let impact):
                return shellAutomationCloseRequiresConfirmationResult(
                    impact: impact,
                    tabID: tabID,
                    paneID: shellState.tab(tabID: tabID)?.paneTree.paneIDs.first
                )
            }

        case .sendText(let request):
            let delivery: TerminalRuntimeDeliveryResult
            if let terminalContentID = request.terminalContentID {
                delivery = terminalRuntimeRegistry.sendText(
                    toTerminalContentID: terminalContentID,
                    text: request.text
                )
            } else {
                delivery = terminalRuntimeRegistry.sendText(
                    to: request.paneID,
                    text: request.text
                )
            }
            return shellAutomationResult(
                code: shellAutomationResultCode(for: delivery),
                paneID: request.paneID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .sendKey(let request):
            let delivery: TerminalRuntimeDeliveryResult
            if let terminalContentID = request.terminalContentID {
                delivery = terminalRuntimeRegistry.sendKey(
                    toTerminalContentID: terminalContentID,
                    key: request.key
                )
            } else {
                delivery = terminalRuntimeRegistry.sendKey(
                    to: request.paneID,
                    key: request.key
                )
            }
            return shellAutomationResult(
                code: shellAutomationResultCode(for: delivery),
                paneID: request.paneID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .readPaneSummary(let paneID):
            guard let summary = shellState.automationPaneSummary(paneID: paneID) else {
                return shellAutomationMissingPaneResult(paneID)
            }
            return shellAutomationResult(
                code: .accepted,
                summary: summary,
                spaceID: summary.spaceID,
                tabID: summary.tabID,
                paneID: summary.paneID
            )

        case .activateAttentionItem(let paneID):
            guard pane(paneID: paneID) != nil else {
                return shellAutomationMissingPaneResult(paneID)
            }
            focus(paneID: paneID, requestTerminalFocus: true)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )
        }
    }

    private func shellAutomationMissingPaneResult(_ paneID: String) -> ShellAutomationCommandResult {
        shellAutomationResult(
            code: .missingTarget,
            paneID: paneID,
            errorCode: "pane_not_found",
            errorMessage: "The requested pane does not exist."
        )
    }

    private func shellAutomationCloseRequiresConfirmationResult(
        impact: ShellCloseGuardImpact,
        tabID: String? = nil,
        paneID: String? = nil
    ) -> ShellAutomationCommandResult {
        shellAutomationResult(
            code: .requiresConfirmation,
            tabID: tabID,
            paneID: paneID,
            errorCode: "requires_confirmation",
            errorMessage: "The requested close contains active terminal work and requires confirmation."
        )
    }

    private func shellStateMutationErrorMessage(_ error: ShellStateMutationError) -> String {
        switch error {
        case .spaceNotFound:
            return "The requested space does not exist."
        case .tabNotFound:
            return "The requested tab does not exist."
        case .paneNotFound:
            return "The requested pane does not exist."
        case .unsupportedContent:
            return "This action requires terminal content."
        case .splitNotFound:
            return "The requested split does not exist."
        case .spatialFocusTargetNotFound:
            return "There is no pane in that direction."
        case .lastTab:
            return "alan terminal workspace must keep at least one tab open."
        case .lastPane:
            return "alan terminal workspace must keep at least one pane open."
        case .invalidMoveTarget:
            return "The requested move target is not available."
        case .invalidTabOrganizationTarget:
            return "The requested tab organization target is not available."
        }
    }

    private func shellAutomationResult(
        code: ShellAutomationCommandResultCode,
        summary: ShellAutomationPaneSummary? = nil,
        spaceID: String? = nil,
        tabID: String? = nil,
        paneID: String? = nil,
        acceptedBytes: Int? = nil,
        deliveryCode: String? = nil,
        runtimePhase: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) -> ShellAutomationCommandResult {
        let resolvedSummary = summary ?? paneID.flatMap {
            shellState.automationPaneSummary(paneID: $0)
        }
        return ShellAutomationCommandResult(
            code: code,
            summary: resolvedSummary,
            spaceID: spaceID ?? resolvedSummary?.spaceID,
            tabID: tabID ?? resolvedSummary?.tabID,
            paneID: paneID ?? resolvedSummary?.paneID,
            acceptedBytes: acceptedBytes,
            deliveryCode: deliveryCode,
            runtimePhase: runtimePhase,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    private func shellAutomationResultCode(
        for delivery: TerminalRuntimeDeliveryResult
    ) -> ShellAutomationCommandResultCode {
        switch delivery.code {
        case .accepted:
            return .accepted
        case .queued:
            return .queued
        case .rejected:
            return .rejected
        case .missingTarget:
            return .missingTarget
        case .unavailableRuntime:
            return .runtimeUnavailable
        case .timeout:
            return .timeout
        }
    }
}
#endif
