import Foundation

#if os(macOS)
@main
struct ShellAutomationCommandSeamsTestRunner {
    static func main() async {
        await MainActor.run {
            ShellAutomationCommandSeamsTests.run()
        }
    }
}

@MainActor
private enum ShellAutomationCommandSeamsTests {
    static func run() {
        verifiesCommandSurfaceCoversCoreAutomationActions()
        verifiesFakeHandlerRecordsCommandsAndReturnsConfiguredResults()
        verifiesQueuedDeliveryDoesNotCountAsApplied()
        verifiesFakeRuntimeRecordsTextDelivery()
        verifiesPaneSummaryUsesSafeMetadata()
        verifiesPaneSummaryFallsBackPastBlankTitles()
        verifiesMissingPaneSummaryReturnsNil()
        print("Shell automation command seam tests passed.")
    }

    private static func verifiesCommandSurfaceCoversCoreAutomationActions() {
        let commands: [ShellAutomationCommand] = [
            .createTab(
                ShellAutomationCreateTabRequest(
                    launchTarget: .shell,
                    spaceID: "space_main",
                    title: "Shell",
                    workingDirectory: "/Users/morris"
                )
            ),
            .createTab(
                ShellAutomationCreateTabRequest(
                    launchTarget: .alan,
                    spaceID: "space_main",
                    title: "alan",
                    workingDirectory: "/Users/morris/Developer/alan"
                )
            ),
            .splitPane(ShellAutomationPaneSplitRequest(paneID: "pane_1", placement: .right)),
            .focusPane(paneID: "pane_1"),
            .closePane(paneID: "pane_1"),
            .closeTab(tabID: "tab_main"),
            .sendText(ShellAutomationSendTextRequest(paneID: "pane_1", text: "printf test\\n")),
            .readPaneSummary(paneID: "pane_1"),
            .activateAttentionItem(paneID: "pane_1"),
        ]

        expect(commands.count == 9, "automation command surface must cover core shell actions")
        expect(commands.contains(.focusPane(paneID: "pane_1")), "focus command must be equatable")
        expect(
            commands.contains(.activateAttentionItem(paneID: "pane_1")),
            "attention activation must have an explicit command"
        )
    }

    private static func verifiesFakeHandlerRecordsCommandsAndReturnsConfiguredResults() {
        let expectedSummary = ShellAutomationPaneSummary(
            windowID: "window_main",
            spaceID: "space_main",
            spaceTitle: "Terminal",
            tabID: "tab_main",
            tabTitle: "Shell",
            paneID: "pane_1",
            paneTitle: "Shell",
            workingDirectory: "/Users/morris",
            processProgram: "zsh",
            processState: "running",
            attention: .active
        )
        let handler = FakeShellAutomationCommandHandler { command in
            guard command == .readPaneSummary(paneID: "pane_1") else {
                return ShellAutomationCommandResult(
                    code: .missingTarget,
                    summary: nil,
                    errorCode: "unexpected_command",
                    errorMessage: "Unexpected command"
                )
            }
            return ShellAutomationCommandResult(code: .accepted, summary: expectedSummary)
        }

        let result = handler.performShellAutomationCommand(.readPaneSummary(paneID: "pane_1"))

        expect(
            handler.recordedCommands == [.readPaneSummary(paneID: "pane_1")],
            "fake must record commands"
        )
        expect(result.code == .accepted, "fake must return configured result")
        expect(result.summary == expectedSummary, "fake must return configured summary")
    }

    private static func verifiesQueuedDeliveryDoesNotCountAsApplied() {
        expect(
            ShellAutomationCommandResult(code: .accepted, summary: nil).applied,
            "accepted delivery must count as applied"
        )
        expect(
            !ShellAutomationCommandResult(code: .queued, summary: nil).applied,
            "queued delivery must not count as applied before runtime acceptance"
        )
        expect(
            !ShellAutomationCommandResult(code: .rejected, summary: nil).applied,
            "rejected delivery must not count as applied"
        )
    }

    private static func verifiesFakeRuntimeRecordsTextDelivery() {
        let request = ShellAutomationSendTextRequest(paneID: "pane_1", text: "pwd\n")
        let runtime = FakeShellAutomationTextRuntime(
            result: ShellAutomationCommandResult(
                code: .queued,
                summary: nil,
                paneID: "pane_1",
                acceptedBytes: 4,
                deliveryCode: "queued"
            )
        )

        let result = runtime.sendText(request)

        expect(runtime.deliveredText == [request], "fake runtime must record text deliveries")
        expect(result.code == .queued, "fake runtime must preserve queued delivery status")
        expect(!result.applied, "queued delivery must not count as applied")
        expect(result.acceptedBytes == 4, "fake runtime must preserve accepted byte count")
    }

    private static func verifiesPaneSummaryUsesSafeMetadata() {
        let secretExcerpt = "SECRET_TOKEN=abc123"
        let secretViewportSummary = "viewport summary includes SECRET_TOKEN=def456"
        let secretControlPath = "/tmp/alan-shell-control/window_main/panes/pane_1"
        let secretBindingFile = "/tmp/alan-shell-bindings/SECRET_BINDING_FILE.json"
        let state = stateWithPaneMetadata(
            viewportTitle: "Project shell",
            displayName: nil,
            processProgram: "zsh",
            viewportSummary: secretViewportSummary,
            visibleExcerpt: secretExcerpt,
            controlPath: secretControlPath,
            alanBindingFile: secretBindingFile
        )

        guard let summary = state.automationPaneSummary(paneID: "pane_1") else {
            fail("expected pane summary")
        }

        expect(summary.windowID == "window_main", "summary must retain window context")
        expect(summary.spaceTitle == "Terminal", "summary must include user-facing space title")
        expect(summary.tabTitle == "Shell", "summary must include user-facing tab title")
        expect(summary.paneTitle == "Project shell", "summary must include user-facing pane title")
        expect(summary.workingDirectory == "/Users/morris/Developer/alan", "summary must include cwd")
        expect(summary.processProgram == "zsh", "summary must include process program")
        expect(summary.attention == .awaitingUser, "summary must include attention state")
        expect(
            !summary.displayText.contains(secretExcerpt),
            "summary display text must not expose terminal visible excerpts"
        )
        expect(
            !summary.displayText.contains(secretViewportSummary),
            "summary display text must not expose terminal viewport summaries"
        )
        expect(
            !summary.displayText.contains(secretControlPath),
            "summary display text must not expose control paths"
        )
        expect(
            !summary.displayText.contains(secretBindingFile),
            "summary display text must not expose binding file paths"
        )
        expect(
            !summary.displayText.contains("pane_1"),
            "summary display text must not expose raw pane IDs"
        )
    }

    private static func verifiesPaneSummaryFallsBackPastBlankTitles() {
        let displayState = stateWithPaneMetadata(
            viewportTitle: "   ",
            displayName: "Workspace shell",
            processProgram: "zsh",
            visibleExcerpt: "SECRET_TOKEN=abc123"
        )

        guard let displaySummary = displayState.automationPaneSummary(paneID: "pane_1") else {
            fail("expected pane summary for display name fallback")
        }

        expect(
            displaySummary.paneTitle == "Workspace shell",
            "blank viewport title must fall back to display name"
        )

        let processState = stateWithPaneMetadata(
            viewportTitle: "\n\t",
            displayName: " ",
            processProgram: "zsh",
            visibleExcerpt: "SECRET_TOKEN=abc123"
        )

        guard let processSummary = processState.automationPaneSummary(paneID: "pane_1") else {
            fail("expected pane summary for process fallback")
        }

        expect(
            processSummary.paneTitle == "zsh",
            "blank viewport and display titles must fall back to process program"
        )
    }

    private static func verifiesMissingPaneSummaryReturnsNil() {
        let state = ShellStateSnapshot.bootstrapDefault()
        expect(
            state.automationPaneSummary(paneID: "missing") == nil,
            "missing pane summary must be nil"
        )
    }

    private static func stateWithVisibleExcerpt(_ visibleExcerpt: String) -> ShellStateSnapshot {
        stateWithPaneMetadata(
            viewportTitle: "Project shell",
            displayName: nil,
            processProgram: "zsh",
            visibleExcerpt: visibleExcerpt
        )
    }

    private static func stateWithPaneMetadata(
        viewportTitle: String?,
        displayName: String?,
        processProgram: String,
        viewportSummary: String = "ready",
        visibleExcerpt: String,
        controlPath: String? = "/tmp/alan-shell-control/window_main/panes/pane_1",
        alanBindingFile: String? = nil
    ) -> ShellStateSnapshot {
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/Users/morris/Developer/alan")
        let updatedPanes = base.panes.map { pane in
            ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.cwd,
                process: ShellProcessBinding(
                    program: processProgram,
                    argvPreview: [processProgram, "-l"]
                ),
                attention: .awaitingUser,
                context: ShellContextSnapshot(
                    workingDirectoryName: "alan",
                    repositoryRoot: "/Users/morris/Developer/alan",
                    gitBranch: "main",
                    controlPath: controlPath,
                    alanBindingFile: alanBindingFile,
                    launchStrategy: "login_shell",
                    shellIntegrationSource: "alan",
                    processState: "running",
                    displayName: displayName,
                    lastMetadataAt: nil,
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: viewportTitle,
                    summary: viewportSummary,
                    visibleExcerpt: visibleExcerpt,
                    lastActivityAt: nil
                ),
                alanBinding: nil
            )
        }

        return ShellStateSnapshot(
            contractVersion: base.contractVersion,
            windowID: base.windowID,
            focusedSpaceID: base.focusedSpaceID,
            focusedTabID: base.focusedTabID,
            focusedPaneID: base.focusedPaneID,
            spaces: base.spaces,
            panes: updatedPanes,
            paneSlots: base.paneSlots,
            contents: base.contents,
            quickTerminal: base.quickTerminal
        )
    }
}

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    guard condition() else { fail(message) }
}

private func fail(_ message: String) -> Never {
    fputs("Test failed: \(message)\n", stderr)
    exit(1)
}
#endif
