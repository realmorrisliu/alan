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
        verifiesFakeRuntimeRecordsTextDelivery()
        verifiesPaneSummaryUsesSafeMetadata()
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
        expect(result.acceptedBytes == 4, "fake runtime must preserve accepted byte count")
    }

    private static func verifiesPaneSummaryUsesSafeMetadata() {
        let secretExcerpt = "SECRET_TOKEN=abc123"
        let state = stateWithVisibleExcerpt(secretExcerpt)

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
            !summary.displayText.contains("pane_1"),
            "summary display text must not expose raw pane IDs"
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
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/Users/morris/Developer/alan")
        let updatedPanes = base.panes.map { pane in
            ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.cwd,
                process: ShellProcessBinding(program: "zsh", argvPreview: ["zsh", "-l"]),
                attention: .awaitingUser,
                context: ShellContextSnapshot(
                    workingDirectoryName: "alan",
                    repositoryRoot: "/Users/morris/Developer/alan",
                    gitBranch: "main",
                    controlPath: "/tmp/alan-shell-control/window_main/panes/pane_1",
                    alanBindingFile: nil,
                    launchStrategy: "login_shell",
                    shellIntegrationSource: "alan",
                    processState: "running",
                    lastMetadataAt: nil,
                    lastCommandExitCode: nil
                ),
                viewport: ShellViewportSnapshot(
                    title: "Project shell",
                    summary: "ready",
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
