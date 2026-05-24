import Foundation

#if os(macOS)
@main
struct ShellAutomationCommandSeamsTestRunner {
    static func main() async {
        await ShellAutomationCommandSeamsTests.run()
    }
}

@MainActor
private enum ShellAutomationCommandSeamsTests {
    static func run() async {
        verifiesCommandSurfaceCoversCoreAutomationActions()
        verifiesFakeHandlerRecordsCommandsAndReturnsConfiguredResults()
        verifiesQueuedDeliveryDoesNotCountAsApplied()
        verifiesFakeRuntimeRecordsTextDelivery()
        verifiesPaneSummaryUsesSafeMetadata()
        verifiesPaneSummaryFallsBackPastBlankTitles()
        verifiesMissingPaneSummaryReturnsNil()
        verifiesAppEntityProjectionUsesSafeDisplayNames()
        await verifiesAppEntityQueriesReadActiveSnapshotState()
        verifiesAppIntentRoutingUsesFakeCommandHandler()
        verifiesAppIntentOutcomeAlignsWithCommandResultCategories()
        verifiesAppIntentDialogRedactsSubmittedTextAndRawTargets()
        verifiesAppIntentAvailabilityDocumentsFallback()
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

    private static func verifiesAppEntityProjectionUsesSafeDisplayNames() {
        let secretExcerpt = "SECRET_TOKEN=abc123"
        let state = stateWithVisibleExcerpt(secretExcerpt)

        let projection = ShellAutomationAppEntitySnapshot.projecting(state)

        expect(
            projection.windows.map(\.id) == ["window_main"],
            "window entity must use the stable shell window identifier"
        )
        expect(
            projection.spaces.map(\.id) == ["space_main"],
            "space entity must use the stable shell space identifier"
        )
        expect(
            projection.tabs.map(\.id) == ["tab_main"],
            "tab entity must use the stable shell tab identifier"
        )
        expect(
            projection.panes.map(\.id) == ["pane_1"],
            "pane entity must use the stable shell pane identifier"
        )

        guard let pane = projection.panes.first else {
            fail("expected pane entity")
        }
        expect(pane.displayTitle == "Project shell", "pane entity must use a user-facing title")
        expect(pane.spaceTitle == "Terminal", "pane entity must retain display-safe space context")
        expect(pane.tabTitle == "Shell", "pane entity must retain display-safe tab context")
        expect(
            pane.displaySubtitle?.contains("/Users/morris/Developer/alan") == true,
            "pane entity subtitle must include safe cwd metadata"
        )
        expect(
            pane.displaySubtitle?.contains(secretExcerpt) != true,
            "pane entity subtitle must not expose terminal visible excerpts"
        )
        expect(
            pane.displayTitle.contains("pane_1") == false,
            "pane entity display title must not expose raw pane IDs"
        )

        guard let attentionItem = projection.attentionItems.first else {
            fail("expected attention item entity")
        }
        expect(
            attentionItem.id == "attention:pane_1",
            "attention item must have a stable identifier derived from its owning pane"
        )
        expect(
            attentionItem.displayTitle == "Project shell",
            "attention item must use the owning pane's user-facing title"
        )
        expect(
            attentionItem.displaySubtitle?.contains(secretExcerpt) != true,
            "attention item display text must not expose terminal visible excerpts"
        )
    }

    private static func verifiesAppEntityQueriesReadActiveSnapshotState() async {
        let state = stateWithVisibleExcerpt("SECRET_TOKEN=abc123")
        ShellAutomationEntityStore.install(snapshotProvider: { state })
        defer { ShellAutomationEntityStore.reset() }

        do {
            let spaces = try await AlanShellSpaceQuery().suggestedEntities()
            expect(spaces.map(\.id) == ["space_main"], "space query must return active window spaces")

            let tabs = try await AlanShellTabQuery().suggestedEntities()
            expect(tabs.map(\.id) == ["tab_main"], "tab query must return active window tabs")

            let panes = try await AlanShellPaneQuery().entities(for: ["pane_1", "missing"])
            expect(panes.map(\.id) == ["pane_1"], "pane query must ignore missing pane IDs")

            let attentionItems = try await AlanShellAttentionItemQuery().suggestedEntities()
            expect(
                attentionItems.map(\.paneID) == ["pane_1"],
                "attention query must return non-idle panes as attention items"
            )

            ShellAutomationEntityStore.install(snapshotProvider: { nil })
            let windows = try await AlanShellWindowQuery().suggestedEntities()
            expect(windows.isEmpty, "window query must return empty when no shell state is active")
        } catch {
            fail("entity query failed: \(error)")
        }
    }

    private static func verifiesAppIntentRoutingUsesFakeCommandHandler() {
        let summary = sampleSummary()
        let pane = AlanShellPaneEntity(summary: summary, isFocused: true)
        let attentionItem = AlanShellAttentionItemEntity(pane: pane)
        let handler = FakeShellAutomationCommandHandler { command in
            switch command {
            case .createTab:
                return ShellAutomationCommandResult(
                    code: .accepted,
                    summary: summary,
                    spaceID: summary.spaceID,
                    tabID: summary.tabID,
                    paneID: summary.paneID
                )
            case .sendText:
                return ShellAutomationCommandResult(
                    code: .queued,
                    summary: nil,
                    paneID: summary.paneID,
                    acceptedBytes: 11,
                    deliveryCode: "queued"
                )
            default:
                return ShellAutomationCommandResult(code: .accepted, summary: summary)
            }
        }

        ShellAutomationIntentStore.install(commandHandler: handler)
        defer { ShellAutomationIntentStore.reset() }

        _ = ShellAutomationIntentRouter.createTerminalTab(
            spaceID: "space_main",
            title: "Shell",
            workingDirectory: "/Users/morris/Developer/alan"
        )
        _ = ShellAutomationIntentRouter.createAlanTab(spaceID: "space_main", title: "alan")
        _ = ShellAutomationIntentRouter.splitPane(pane, direction: .right)
        _ = ShellAutomationIntentRouter.focusPane(pane)
        _ = ShellAutomationIntentRouter.closePane(pane)
        _ = ShellAutomationIntentRouter.closeTab(AlanShellTabEntity(
            id: summary.tabID,
            windowID: summary.windowID,
            spaceID: summary.spaceID,
            spaceTitle: summary.spaceTitle,
            displayTitle: summary.tabTitle,
            displaySubtitle: nil,
            kind: ShellTabKind.terminal.rawValue,
            isPinned: false,
            isFocused: true
        ))
        _ = ShellAutomationIntentRouter.sendText("pwd\n", to: pane)
        _ = ShellAutomationIntentRouter.readPaneSummary(for: pane)
        _ = ShellAutomationIntentRouter.openAttentionItem(attentionItem)

        expect(
            handler.recordedCommands == [
                .createTab(ShellAutomationCreateTabRequest(
                    launchTarget: .shell,
                    spaceID: "space_main",
                    title: "Shell",
                    workingDirectory: "/Users/morris/Developer/alan"
                )),
                .createTab(ShellAutomationCreateTabRequest(
                    launchTarget: .alan,
                    spaceID: "space_main",
                    title: "alan",
                    workingDirectory: nil
                )),
                .splitPane(ShellAutomationPaneSplitRequest(paneID: summary.paneID, placement: .right)),
                .focusPane(paneID: summary.paneID),
                .closePane(paneID: summary.paneID),
                .closeTab(tabID: summary.tabID),
                .sendText(ShellAutomationSendTextRequest(paneID: summary.paneID, text: "pwd\n")),
                .readPaneSummary(paneID: summary.paneID),
                .activateAttentionItem(paneID: summary.paneID),
            ],
            "App Intent router must call the shared shell automation command surface"
        )
    }

    private static func verifiesAppIntentOutcomeAlignsWithCommandResultCategories() {
        let missing = ShellAutomationIntentOutcome(
            command: .focusPane(paneID: "pane_missing"),
            result: ShellAutomationCommandResult(
                code: .missingTarget,
                summary: nil,
                paneID: "pane_missing",
                errorCode: "missing_target",
                errorMessage: "Missing pane"
            )
        )
        expect(missing.code == .missingTarget, "intent outcome must preserve missing-target code")
        expect(missing.dialog.contains("target"), "missing-target dialog must explain the category")

        let unavailable = ShellAutomationIntentOutcome(
            command: .sendText(ShellAutomationSendTextRequest(
                paneID: "pane_1",
                text: "printf secret\n"
            )),
            result: ShellAutomationCommandResult(
                code: .runtimeUnavailable,
                summary: nil,
                paneID: "pane_1",
                errorCode: "runtime_unavailable",
                errorMessage: "Runtime unavailable"
            )
        )
        expect(
            unavailable.code == .runtimeUnavailable,
            "intent outcome must preserve runtime-unavailable code"
        )
        expect(
            unavailable.dialog.contains("runtime"),
            "runtime-unavailable dialog must explain the category"
        )
    }

    private static func verifiesAppIntentDialogRedactsSubmittedTextAndRawTargets() {
        let secretText = "SECRET_TOKEN=abc123\n"
        let outcome = ShellAutomationIntentOutcome(
            command: .sendText(ShellAutomationSendTextRequest(
                paneID: "pane_secret",
                text: secretText
            )),
            result: ShellAutomationCommandResult(
                code: .queued,
                summary: nil,
                paneID: "pane_secret",
                acceptedBytes: secretText.utf8.count,
                deliveryCode: "queued"
            )
        )

        expect(outcome.code == .queued, "intent outcome must preserve queued delivery")
        expect(outcome.dialog.contains("queued"), "intent outcome must report queued delivery")
        expect(
            !outcome.dialog.contains(secretText.trimmingCharacters(in: .whitespacesAndNewlines)),
            "intent dialog must not echo submitted text"
        )
        expect(!outcome.dialog.contains("pane_secret"), "intent dialog must not expose raw pane IDs")

        let summary = sampleSummary()
        let summaryOutcome = ShellAutomationIntentOutcome(
            command: .readPaneSummary(paneID: summary.paneID),
            result: ShellAutomationCommandResult(code: .accepted, summary: summary)
        )
        expect(
            !summaryOutcome.dialog.contains(summary.paneID),
            "summary dialog must not expose raw pane IDs"
        )
        expect(
            summaryOutcome.dialog.contains(summary.paneTitle),
            "summary dialog must include display-safe pane metadata"
        )
    }

    private static func verifiesAppIntentAvailabilityDocumentsFallback() {
        expect(
            ShellAutomationIntentAvailability.minimumSupportedMacOS == "macOS 13.0",
            "App Intent availability must declare the supported macOS floor"
        )
        expect(
            ShellAutomationIntentAvailability.fallbackDescription.contains("control plane"),
            "App Intent fallback documentation must point to the control plane"
        )
    }

    private static func sampleSummary() -> ShellAutomationPaneSummary {
        ShellAutomationPaneSummary(
            windowID: "window_main",
            spaceID: "space_main",
            spaceTitle: "Terminal",
            tabID: "tab_main",
            tabTitle: "Shell",
            paneID: "pane_1",
            paneTitle: "Project shell",
            workingDirectory: "/Users/morris/Developer/alan",
            processProgram: "zsh",
            processState: "running",
            attention: .awaitingUser
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
