import Foundation

@main
struct ShellSidebarTabRowTestRunner {
    static func main() throws {
        try ShellSidebarTabRowTests.run()
    }
}

private enum ShellSidebarTabRowTests {
    static func run() throws {
        try verifiesAgentDetailBecomesPrimaryTabTitle()
        try verifiesFallbackMetadataDoesNotForceSubtitle()
        try verifiesActionableStateRequiresSubtitle()
        try verifiesUserRenameLocksTabTitle()
        try verifiesClearKeepsSelectedPinnedOtherSpaceAndProtectedTabs()
        try verifiesTabContextMenuModelIsTabScoped()
        try verifiesDuplicateAndOpenSplitActionsRouteClickedTab()
        try verifiesDuplicateAndOpenSplitRejectContentTabs()
        try verifiesTemporarySectionPresentationFollowsUnpinnedTabs()
        try verifiesDragPayloadCarriesSourceIdentity()
        try verifiesDragMutationIndexAdjustsSameSectionForwardDrop()
        print("Shell sidebar tab row tests passed.")
    }

    private static func verifiesAgentDetailBecomesPrimaryTabTitle() throws {
        let now = ISO8601DateFormatter().date(from: "2026-06-06T09:00:00Z")!
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/Users/morris/Developer/alan")
        let activity = try requireActivity(
            TerminalAgentActivityAdapter.activity(
                from: TerminalAgentActivityEvent(
                    agentKind: "codex",
                    status: "running",
                    sessionLabel: "main",
                    projectLabel: "alan",
                    workingDirectory: "/Users/morris/Developer/alan",
                    detail: "Fix sidebar tab drag",
                    updatedAt: "2026-06-06T09:00:00Z"
                ),
                now: now
            )
        )
        state = try state.applyingAgentActivity(
            activity,
            to: "pane_1",
            workingDirectory: "/Users/morris/Developer/alan"
        ).state

        let projection = try projection(for: "tab_main", in: state, now: now)

        expect(
            projection.title == "Fix sidebar tab drag",
            "agent-provided task detail must become the primary sidebar title"
        )
        expect(
            projection.secondaryLine?.hasPrefix("alan") == true,
            "task-title rows must keep stable project context in the subtitle"
        )
    }

    private static func verifiesFallbackMetadataDoesNotForceSubtitle() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        let projection = try projection(for: "tab_main", in: state)

        expect(projection.title == "tmp", "fallback title may use the working-directory identity")
        expect(
            projection.secondaryLine == nil,
            "fallback metadata must use single-line row layout, got \(projection.secondaryLine ?? "nil")"
        )
    }

    private static func verifiesActionableStateRequiresSubtitle() throws {
        let now = ISO8601DateFormatter().date(from: "2026-06-06T09:00:00Z")!
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/Users/morris/Developer/alan")
        let activity = try requireActivity(
            TerminalAgentActivityAdapter.activity(
                from: TerminalAgentActivityEvent(
                    agentKind: "codex",
                    status: "needs_input",
                    sessionLabel: nil,
                    projectLabel: "alan",
                    workingDirectory: "/Users/morris/Developer/alan",
                    detail: "Review menu spec",
                    updatedAt: "2026-06-06T09:00:00Z"
                ),
                now: now
            )
        )
        state = try state.applyingAgentActivity(
            activity,
            to: "pane_1",
            workingDirectory: "/Users/morris/Developer/alan"
        ).state

        let projection = try projection(for: "tab_main", in: state, now: now)

        expect(projection.title == "Review menu spec", "task title must stay stable for actionable states")
        expect(
            projection.secondaryLine?.hasPrefix("Input needed") == true,
            "actionable state must be the first subtitle token"
        )
        expect(
            projection.stateAccessory?.accessibilityLabel == "Input needed",
            "actionable state must also project a trailing accessory"
        )
    }

    private static func verifiesUserRenameLocksTabTitle() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        state = try state.renamingTab("tab_main", title: "User Task").state
        state = try state.settingAutomaticTabTitle("tab_main", title: "Terminal Changed").state

        let tab = try requireTab("tab_main", in: state)
        expect(tab.title == "User Task", "automatic titles must not overwrite user-renamed tabs")
        expect(tab.isTitleUserLocked, "renaming from context menu must lock the tab title")
    }

    private static func verifiesClearKeepsSelectedPinnedOtherSpaceAndProtectedTabs() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Clearable", workingDirectory: "/tmp").state
        let clearableTabID = try requireFocusedTabID(in: state)
        state = try state.openingTerminalTab(in: "space_main", title: "Protected", workingDirectory: "/tmp").state
        let protectedTabID = try requireFocusedTabID(in: state)
        state = try state.openingTerminalTab(in: "space_main", title: "Pinned", workingDirectory: "/tmp").state
        let pinnedTabID = try requireFocusedTabID(in: state)
        state = try state.pinningTab(pinnedTabID).state
        state = state.creatingTerminalSpace(title: "Other", workingDirectory: "/tmp").state
        let otherSpaceTabID = try requireFocusedTabID(in: state)
        state = try state.focusingPane("pane_1").state

        let cleared = try state.clearingInactiveTemporaryTabs(
            in: "space_main",
            activeTaskByTabID: [protectedTabID: .foregroundCommand]
        ).state

        expect(cleared.tab(tabID: clearableTabID) == nil, "Clear must remove eligible inactive unpinned tabs")
        expect(cleared.tab(tabID: "tab_main") != nil, "Clear must keep the selected tab")
        expect(cleared.tab(tabID: protectedTabID) != nil, "Clear must keep protected active-task tabs")
        expect(cleared.tab(tabID: pinnedTabID) != nil, "Clear must keep pinned tabs")
        expect(cleared.tab(tabID: otherSpaceTabID) != nil, "Clear must not touch other Spaces")
        expect(cleared.focusedTabID == "tab_main", "Clear must preserve selected tab focus")
        expect(cleared.focusedPaneID == "pane_1", "Clear must preserve selected pane focus")
    }

    private static func verifiesTabContextMenuModelIsTabScoped() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Second", workingDirectory: "/tmp").state
        let clickedTabID = try requireFocusedTabID(in: state)
        state = state.creatingTerminalSpace(title: "Other", workingDirectory: "/tmp").state

        let model = try ShellSidebarTabContextMenuModel.model(
            tabID: clickedTabID,
            in: "space_main",
            state: state
        )

        expect(
            model.primaryActionTitles == ["Rename...", "Duplicate Tab", "Open in Split View"],
            "tab context menu must start with identity, duplicate, and split actions"
        )
        expect(model.organizationActionTitles == ["Pin Tab", "Move to"], "organization actions must stay tab-scoped")
        expect(model.destructiveActionTitles == ["Close Tab"], "Close Tab must be the final destructive action")
        expect(!model.allActionTitles.contains("New Terminal Tab"), "tab context menu must not include Space actions")
        expect(!model.allActionTitles.contains("Clear"), "tab context menu must not include batch cleanup")
    }

    private static func verifiesDuplicateAndOpenSplitActionsRouteClickedTab() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Clicked", workingDirectory: "/tmp").state
        let clickedTabID = try requireFocusedTabID(in: state)
        state = try state.focusingPane("pane_1").state
        let selectedTabBefore = state.focusedTabID

        let adapter = try ShellCoreFFIAdapter()
        var effects: [ShellActionEffect] = []
        let duplicate = try adapter.executeAction(
            .tabDuplicate,
            target: .contextTab(clickedTabID),
            state: state
        ) { effect in
            effects.append(effect)
            return true
        }
        let openSplit = try adapter.executeAction(
            .tabOpenInSplitView,
            target: .contextTab(clickedTabID),
            state: state
        ) { effect in
            effects.append(effect)
            return true
        }

        expect(state.focusedTabID == selectedTabBefore, "context actions must not select before routing")
        expect(duplicate == .executed, "Duplicate Tab must be available for terminal tabs")
        expect(openSplit == .executed, "Open in Split View must be available for terminal tabs")
        expect(
            effects == [.duplicateTab(clickedTabID), .openTabInSplitView(clickedTabID)],
            "context menu actions must route the clicked tab id"
        )
    }

    private static func verifiesDuplicateAndOpenSplitRejectContentTabs() throws {
        let markdownState = try ShellStateSnapshot
            .bootstrapDefault(workingDirectory: "/tmp")
            .openingMarkdownTab(
                fileURL: URL(fileURLWithPath: "/tmp/readme.md"),
                in: "space_main",
                title: "Readme"
            )
            .state
        try expectContentTabActionsRejected(in: markdownState, label: "Markdown")

        let settingsState = try ShellStateSnapshot
            .bootstrapDefault(workingDirectory: "/tmp")
            .openingSettingsTab(
                in: "space_main",
                title: "Settings"
            )
            .state
        try expectContentTabActionsRejected(in: settingsState, label: "Settings")
    }

    private static func expectContentTabActionsRejected(
        in state: ShellStateSnapshot,
        label: String
    ) throws {
        let contentTabID = try requireFocusedTabID(in: state)

        let adapter = try ShellCoreFFIAdapter()
        var effects: [ShellActionEffect] = []
        let duplicate = try adapter.executeAction(
            .tabDuplicate,
            target: .contextTab(contentTabID),
            state: state
        ) { effect in
            effects.append(effect)
            return true
        }
        let openSplit = try adapter.executeAction(
            .tabOpenInSplitView,
            target: .contextTab(contentTabID),
            state: state
        ) { effect in
            effects.append(effect)
            return true
        }

        expect(duplicate == .unavailable(reason: "Tab is not a terminal"), "\(label) tabs must not enable Duplicate Tab")
        expect(openSplit == .unavailable(reason: "Tab cannot be split"), "\(label) tabs must not enable Open in Split View")
        expect(effects.isEmpty, "\(label) tabs must not emit duplicate or split effects")
    }

    private static func verifiesTemporarySectionPresentationFollowsUnpinnedTabs() throws {
        expect(
            ShellSidebarTemporaryTabSectionPresentation.model(
                pinnedTabCount: 0,
                unpinnedTabCount: 0,
                clearableTabCount: 0
            ) == ShellSidebarTemporaryTabSectionPresentation(
                showsControlRow: false,
                showsDivider: false,
                showsClear: false,
                isClearEnabled: false
            ),
            "empty spaces must not reserve temporary-section divider space"
        )

        expect(
            ShellSidebarTemporaryTabSectionPresentation.model(
                pinnedTabCount: 2,
                unpinnedTabCount: 0,
                clearableTabCount: 0
            ) == ShellSidebarTemporaryTabSectionPresentation(
                showsControlRow: false,
                showsDivider: false,
                showsClear: false,
                isClearEnabled: false
            ),
            "pinned-only spaces must place New Tab directly after pinned tabs"
        )

        expect(
            ShellSidebarTemporaryTabSectionPresentation.model(
                pinnedTabCount: 0,
                unpinnedTabCount: 1,
                clearableTabCount: 0
            ) == ShellSidebarTemporaryTabSectionPresentation(
                showsControlRow: true,
                showsDivider: true,
                showsClear: false,
                isClearEnabled: false
            ),
            "unpinned-only spaces without clearable tabs must keep the divider but hide Clear"
        )

        expect(
            ShellSidebarTemporaryTabSectionPresentation.model(
                pinnedTabCount: 1,
                unpinnedTabCount: 2,
                clearableTabCount: 1
            ) == ShellSidebarTemporaryTabSectionPresentation(
                showsControlRow: true,
                showsDivider: true,
                showsClear: true,
                isClearEnabled: true
            ),
            "mixed spaces with clearable temporary tabs must show divider and Clear"
        )
    }

    private static func verifiesDragPayloadCarriesSourceIdentity() throws {
        let source = ShellSidebarTabDragSource(
            tabID: "tab_2",
            sourceSpaceID: "space_main",
            sourceSection: .unpinned,
            sourceIndex: 1
        )

        let encoded = try source.encodedPlainTextPayload()
        let decoded = try ShellSidebarTabDragSource.decodedPlainTextPayload(encoded)

        expect(decoded == source, "drag payload must round-trip tab identity and source location")
    }

    private static func verifiesDragMutationIndexAdjustsSameSectionForwardDrop() throws {
        let source = ShellSidebarTabDragSource(
            tabID: "tab_2",
            sourceSpaceID: "space_main",
            sourceSection: .unpinned,
            sourceIndex: 1
        )
        let target = ShellSidebarTabInsertionTarget(
            spaceID: "space_main",
            section: .unpinned,
            index: 3
        )

        expect(
            ShellSidebarTabDropModel.mutationIndex(for: target, source: source) == 2,
            "same-section forward drops must subtract the removed source row before mutation"
        )
    }

    private static func projection(
        for tabID: String,
        in state: ShellStateSnapshot,
        now: Date? = nil
    ) throws -> ShellSidebarTabProjection {
        let tab = try requireTab(tabID, in: state)
        return shellSidebarTabProjection(
            for: tab,
            panes: state.panes,
            contentState: state.contentStateProjection(),
            focusedPaneID: state.focusedPaneID,
            focusedTabID: state.focusedTabID,
            now: now
        )
    }

    private static func requireActivity(_ activity: TerminalActivitySnapshot?) throws -> TerminalActivitySnapshot {
        guard let activity else {
            throw TestFailure("missing activity")
        }
        return activity
    }

    private static func requireFocusedTabID(in state: ShellStateSnapshot) throws -> String {
        guard let tabID = state.focusedTabID else {
            throw TestFailure("missing focused tab")
        }
        return tabID
    }

    private static func requireTab(_ tabID: String, in state: ShellStateSnapshot) throws -> ShellTab {
        guard let tab = state.tab(tabID: tabID) else {
            throw TestFailure("missing tab \(tabID)")
        }
        return tab
    }

    private static func requirePrimaryPaneID(in tab: ShellTab) throws -> String {
        guard let paneID = tab.paneTree.paneIDs.first else {
            throw TestFailure("missing primary pane in \(tab.tabID)")
        }
        return paneID
    }

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: String
    ) {
        guard condition() else {
            fail(message)
        }
    }

    private static func fail(_ message: String) -> Never {
        fputs("error: \(message)\n", stderr)
        exit(1)
    }
}

private struct TestFailure: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
