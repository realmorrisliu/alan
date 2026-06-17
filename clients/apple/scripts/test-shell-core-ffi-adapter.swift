import Foundation

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {}

private struct AlanShellCoreByteBuffer {
    let ptr: UnsafeMutablePointer<UInt8>?
    let len: Int
}

@_silgen_name("alan_shell_core_ffi_abi_version")
private func alan_shell_core_ffi_abi_version() -> UInt32

@_silgen_name("alan_shell_core_ffi_handle_request")
private func alan_shell_core_ffi_handle_request(
    _ ptr: UnsafePointer<UInt8>?,
    _ len: Int
) -> AlanShellCoreByteBuffer

@_silgen_name("alan_shell_core_ffi_free_buffer")
private func alan_shell_core_ffi_free_buffer(_ buffer: AlanShellCoreByteBuffer)

private enum TestFailure: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let message):
            return message
        }
    }
}

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    guard condition() else {
        throw TestFailure.message(message)
    }
}

@main
private enum ShellCoreFFIAdapterTestRunner {
    static func main() {
        do {
            try testDescribeAndABIVersion()
            try testSchemaMismatchAndUnknownOperationErrors()
            try testCapabilityRows()
            try testManifestReducerAndActionCalls()
            try testProductionAdapterReducerFocus()
            try testProductionAdapterActions()
            try testProductionAdapterControlCommands()
            try testProductionAdapterTerminalProfiles()
            print("Shell core FFI adapter tests passed.")
        } catch {
            fputs("Shell core FFI adapter tests failed: \(error)\n", stderr)
            exit(1)
        }
    }
}

private struct RawShellCoreFFIAdapter {
    struct Response {
        let payload: [String: Any]?
        let error: [String: Any]?
        let raw: [String: Any]
    }

    let abiVersion: UInt32

    init(expectedABIVersion: UInt32 = 1) throws {
        abiVersion = alan_shell_core_ffi_abi_version()
        guard abiVersion == expectedABIVersion else {
            throw TestFailure.message(
                "unexpected shell-core ABI version \(abiVersion), expected \(expectedABIVersion)"
            )
        }
    }

    func send(
        operation: String,
        payload: Any,
        schemaMajor: Int = 1,
        schemaMinor: Int = 0
    ) throws -> Response {
        let request: [String: Any] = [
            "schema_version": ["major": schemaMajor, "minor": schemaMinor],
            "id": UUID().uuidString.lowercased(),
            "operation": operation,
            "payload": payload,
        ]
        let requestData = try JSONSerialization.data(withJSONObject: request)
        let responseData = try requestData.withUnsafeBytes { requestBytes -> Data in
            let base = requestBytes.bindMemory(to: UInt8.self).baseAddress
            let buffer = alan_shell_core_ffi_handle_request(base, requestData.count)
            defer { alan_shell_core_ffi_free_buffer(buffer) }
            guard let ptr = buffer.ptr else {
                throw TestFailure.message("shell-core returned a null response buffer")
            }
            return Data(bytes: ptr, count: buffer.len)
        }
        guard let response = try JSONSerialization.jsonObject(with: responseData) as? [String: Any]
        else {
            throw TestFailure.message("shell-core response was not a JSON object")
        }

        return Response(
            payload: response["payload"] as? [String: Any],
            error: response["error"] as? [String: Any],
            raw: response
        )
    }
}

private func testDescribeAndABIVersion() throws {
    let adapter = try RawShellCoreFFIAdapter()
    try expect(adapter.abiVersion == 1, "ABI version must be pinned to 1")

    let response = try adapter.send(operation: "facade.describe", payload: [:])
    try expect(response.error == nil, "describe must succeed")
    try expect(response.payload?["binding"] as? String == "c_abi_bytes", "binding must be byte-envelope C ABI")
    try expect(response.payload?["generated_bindings"] as? Bool == false, "first facade must not rely on generated bindings")
    let operations = response.payload?["supported_operations"] as? [String] ?? []
    try expect(
        operations.contains("reducer.apply"),
        "describe must advertise reducer operation"
    )
}

private func testSchemaMismatchAndUnknownOperationErrors() throws {
    let adapter = try RawShellCoreFFIAdapter()

    let mismatch = try adapter.send(
        operation: "facade.describe",
        payload: [:],
        schemaMajor: 99
    )
    try expect(mismatch.payload == nil, "schema mismatch must not return a success payload")
    try expect(
        mismatch.error?["code"] as? String == "schema_version_mismatch",
        "schema mismatch must be a stable shell-core error"
    )

    let unknown = try adapter.send(operation: "missing.operation", payload: [:])
    try expect(
        unknown.error?["code"] as? String == "unknown_operation",
        "unknown operation must be mapped into a stable error"
    )
}

private func testCapabilityRows() throws {
    let adapter = try RawShellCoreFFIAdapter()
    let response = try adapter.send(
        operation: "settings.capability_rows",
        payload: [
            "skills": [
                [
                    "id": "memory",
                    "name": "Memory",
                    "enabled": true,
                    "allow_implicit_invocation": false,
                    "available": true,
                ],
                [
                    "id": "plan",
                    "name": "Plan",
                    "enabled": false,
                    "allow_implicit_invocation": false,
                    "available": true,
                ],
            ],
        ]
    )

    try expect(response.error == nil, "capability rows must succeed")
    guard let rows = response.payload?["rows"] as? [[String: Any]],
          let first = rows.first
    else {
        throw TestFailure.message("capability response must contain rows")
    }
    try expect(first["id"] as? String == "capabilitiesAvailable", "first row id must match Swift summary")
    try expect(first["value"] as? String == "1 of 2", "capability count must be projected by Rust")
    try expect(first["mutability"] as? String == "read_only", "row mutability must decode")
}

private func testManifestReducerAndActionCalls() throws {
    let adapter = try RawShellCoreFFIAdapter()
    let manifestResponse = try adapter.send(
        operation: "manifest.default_manifest",
        payload: [
            "window_id": "window_main",
            "default_working_directory": "/repo/app",
            "now": "2026-06-17T12:00:00Z",
        ]
    )
    try expect(manifestResponse.error == nil, "default manifest must succeed through FFI")
    guard let manifest = manifestResponse.payload?["manifest"] as? [String: Any] else {
        throw TestFailure.message("default manifest response must contain a manifest object")
    }

    let materializeResponse = try adapter.send(
        operation: "manifest.materialize",
        payload: [
            "manifest": manifest,
            "default_working_directory": "/fallback",
            "now": "2026-06-17T12:00:00Z",
        ]
    )
    try expect(materializeResponse.error == nil, "materialize must succeed through FFI")
    guard let state = materializeResponse.payload?["state"] as? [String: Any] else {
        throw TestFailure.message("materialize response must contain a workspace state")
    }
    try expect(state["focused_tab_id"] as? String == "tab_main", "materialized state must preserve focus")

    let reducerResponse = try adapter.send(
        operation: "reducer.apply",
        payload: [
            "state": state,
            "operation": [
                "type": "open_terminal_tab",
                "space_id": NSNull(),
                "title": "Logs",
                "working_directory": "/repo/app/logs",
                "terminal_profile_id": NSNull(),
            ],
        ]
    )
    try expect(reducerResponse.error == nil, "reducer apply must succeed through FFI")
    try expect(reducerResponse.payload?["status"] as? String == "ok", "reducer response must be ok")

    let shortcutResponse = try adapter.send(
        operation: "actions.default_shortcut",
        payload: [
            "id": "shell.tab.new_terminal",
            "target": [
                "type": "current_selection",
            ],
        ]
    )
    try expect(shortcutResponse.error == nil, "action shortcut must succeed through FFI")
    guard let shortcut = shortcutResponse.payload?["shortcut"] as? [String: Any] else {
        throw TestFailure.message("action shortcut response must contain shortcut")
    }
    try expect(shortcut["key"] as? String == "t", "new terminal tab shortcut key must match")
    try expect(shortcut["context"] as? String == "shell", "new terminal tab shortcut context must match")
}

private func testProductionAdapterReducerFocus() throws {
    let adapter = try ShellCoreFFIAdapter()
    let state = ShellStateSnapshot.bootstrapDefault(
        windowID: "window_main",
        workingDirectory: "/repo/app"
    )
    let splitResult = try state.splittingPane("pane_1", placement: .right)
    guard let targetPaneID = splitResult.paneID else {
        throw TestFailure.message("split fixture must create a pane")
    }

    let focusResult = try adapter.applyReducer(
        state: splitResult.state,
        operation: .focusPane(paneSlotID: targetPaneID)
    )
    try expect(focusResult.paneID == targetPaneID, "production adapter must return focused pane id")
    try expect(
        focusResult.state.focusedPaneID == targetPaneID,
        "production adapter must materialize focused pane state"
    )
    try expect(
        focusResult.state.focusedTabID == splitResult.state.focusedTabID,
        "focus reducer must preserve selected tab"
    )

    let adjacentResult = try adapter.applyReducer(
        state: focusResult.state,
        operation: .focusAdjacentPane(direction: .left)
    )
    try expect(
        adjacentResult.paneID == "pane_1",
        "production adapter must apply adjacent focus through Rust"
    )
    try expect(
        adjacentResult.state.focusedPaneID == "pane_1",
        "adjacent focus must materialize the Rust-focused pane"
    )

    let liftedPane = try adapter.applyReducer(
        state: adjacentResult.state,
        operation: .movePaneToNewTab(paneSlotID: targetPaneID, title: "Lifted")
    )
    try expect(
        liftedPane.state.pane(paneID: targetPaneID)?.tabID == "tab_2",
        "production adapter must lift panes to new tabs through Rust"
    )
    try expect(
        liftedPane.state.tab(tabID: "tab_2")?.title == "Lifted",
        "production adapter must pass lifted tab title through Rust"
    )

    let movePaneTargetInput = try adjacentResult.state.openingTerminalTab(
        in: "space_main",
        title: "Move Target",
        workingDirectory: "/repo/target"
    ).state
    let movedPaneToTab = try adapter.applyReducer(
        state: movePaneTargetInput,
        operation: .movePaneToTab(
            paneSlotID: targetPaneID,
            targetTabID: "tab_2",
            direction: .horizontal
        )
    )
    try expect(
        movedPaneToTab.state.pane(paneID: targetPaneID)?.tabID == "tab_2",
        "production adapter must move panes across tabs through Rust"
    )
    try expect(
        movedPaneToTab.state.tab(tabID: "tab_2")?.paneTree.paneIDs.contains(targetPaneID) == true,
        "production adapter must attach moved panes into the target tab tree"
    )

    let closedPane = try adapter.applyReducer(
        state: adjacentResult.state,
        operation: .closePane(paneSlotID: targetPaneID)
    )
    try expect(
        closedPane.state.pane(paneID: targetPaneID) == nil,
        "production adapter must apply pane close through Rust"
    )

    let renamed = try adapter.applyReducer(
        state: closedPane.state,
        operation: .renameTab(tabID: "tab_main", title: " Main ")
    )
    try expect(
        renamed.state.tab(tabID: "tab_main")?.title == "Main",
        "production adapter must apply tab rename through Rust"
    )

    let pinned = try adapter.applyReducer(
        state: renamed.state,
        operation: .pinTab(tabID: "tab_main")
    )
    try expect(
        pinned.state.tab(tabID: "tab_main")?.isPinned == true,
        "production adapter must apply tab pin through Rust"
    )

    let unpinned = try adapter.applyReducer(
        state: pinned.state,
        operation: .unpinTab(tabID: "tab_main")
    )
    try expect(
        unpinned.state.tab(tabID: "tab_main")?.isPinned == false,
        "production adapter must apply tab unpin through Rust"
    )

    let attention = try adapter.applyReducer(
        state: unpinned.state,
        operation: .setAttention(paneSlotID: "pane_1", attention: .awaitingUser)
    )
    try expect(
        attention.state.pane(paneID: "pane_1")?.attention == .awaitingUser,
        "production adapter must apply pane attention through Rust"
    )

    let markdownURL = "file:///repo/Guide.md"
    let openedMarkdown = try adapter.applyReducer(
        state: attention.state,
        operation: .openContentTab(
            spaceID: "space_main",
            kind: .markdown,
            title: "Guide.md",
            payload: .markdown(
                ShellMarkdownContentPayload(
                    fileURL: markdownURL,
                    title: "Guide.md"
                )
            ),
            reservedPaneSlotIDs: []
        )
    )
    guard let openedMarkdownPaneID = openedMarkdown.paneID else {
        throw TestFailure.message("markdown content tab must return a focused pane")
    }
    let openedMarkdownContent = openedMarkdown.state.contentStateProjection().content(
        contentID: ShellContentInstance.markdownContentID(forPaneSlotID: openedMarkdownPaneID)
    )
    try expect(
        openedMarkdownContent?.kind == .markdown,
        "production adapter must open markdown content through Rust"
    )
    try expect(
        openedMarkdownContent?.payload.markdown?.fileURL == markdownURL,
        "production adapter must preserve markdown payload through Rust"
    )
    try expect(
        openedMarkdown.state.focusedPaneID == openedMarkdownPaneID,
        "production adapter must focus the Rust-opened markdown pane"
    )

    let openedSettings = try adapter.applyReducer(
        state: openedMarkdown.state,
        operation: .openContentTab(
            spaceID: "space_main",
            kind: .settings,
            title: "Settings",
            payload: .settings(
                ShellSettingsContentPayload(
                    surfaceID: ShellContentInstance.settingsSurfaceID,
                    title: "Settings"
                )
            ),
            reservedPaneSlotIDs: []
        )
    )
    guard let openedSettingsPaneID = openedSettings.paneID else {
        throw TestFailure.message("settings content tab must return a focused pane")
    }
    let settingsContents = openedSettings.state.contentStateProjection().contents.filter {
        $0.kind == .settings
    }
    try expect(settingsContents.count == 1, "production adapter must open one settings content")
    try expect(
        settingsContents.first?.payload.settings?.surfaceID == ShellContentInstance.settingsSurfaceID,
        "production adapter must preserve settings payload through Rust"
    )

    let reopenedSettings = try adapter.applyReducer(
        state: openedSettings.state,
        operation: .openContentTab(
            spaceID: "space_main",
            kind: .settings,
            title: "Settings",
            payload: .settings(
                ShellSettingsContentPayload(
                    surfaceID: ShellContentInstance.settingsSurfaceID,
                    title: "Settings"
                )
            ),
            reservedPaneSlotIDs: []
        )
    )
    let reopenedSettingsContents = reopenedSettings.state.contentStateProjection().contents.filter {
        $0.kind == .settings
    }
    try expect(
        reopenedSettings.paneID == openedSettingsPaneID,
        "production adapter must focus the existing settings pane through Rust"
    )
    try expect(
        reopenedSettingsContents.count == 1,
        "production adapter must not duplicate settings content through Rust"
    )

    let splitMarkdown = try adapter.applyReducer(
        state: attention.state,
        operation: .splitContentPane(
            paneSlotID: "pane_1",
            placement: .right,
            kind: .markdown,
            title: "Split Guide.md",
            payload: .markdown(
                ShellMarkdownContentPayload(
                    fileURL: markdownURL,
                    title: "Split Guide.md"
                )
            ),
            reservedPaneSlotIDs: []
        )
    )
    guard let splitMarkdownPaneID = splitMarkdown.paneID else {
        throw TestFailure.message("markdown content split must return a focused pane")
    }
    let splitMarkdownContent = splitMarkdown.state.contentStateProjection().content(
        contentID: ShellContentInstance.markdownContentID(forPaneSlotID: splitMarkdownPaneID)
    )
    try expect(
        splitMarkdown.state.tab(tabID: "tab_main")?.paneTree.paneIDs.contains(splitMarkdownPaneID)
            == true,
        "production adapter must attach Rust-split markdown pane to the source tab"
    )
    try expect(
        splitMarkdownContent?.payload.markdown?.title == "Split Guide.md",
        "production adapter must preserve Rust-split markdown payload"
    )

    let quickShown = try adapter.applyReducer(
        state: attention.state,
        operation: .showQuickTerminal(
            workingDirectory: "/repo/quick",
            defaultWorkingDirectory: "/home/test"
        )
    )
    try expect(
        quickShown.state.quickTerminal?.presentation == .visible,
        "production adapter must show quick terminal through Rust"
    )
    try expect(
        quickShown.state.pane(paneID: ShellQuickTerminalSlot.globalPaneID)?.cwd == "/repo/quick",
        "production adapter must materialize quick terminal cwd from Rust"
    )

    let quickHidden = try adapter.applyReducer(
        state: quickShown.state,
        operation: .hideQuickTerminal
    )
    try expect(
        quickHidden.state.quickTerminal?.presentation == .hidden,
        "production adapter must hide quick terminal through Rust"
    )

    let quickPromoted = try adapter.applyReducer(
        state: quickHidden.state,
        operation: .promoteQuickTerminal(targetSpaceID: "space_main")
    )
    try expect(
        quickPromoted.state.quickTerminal == nil,
        "production adapter must clear detached quick terminal after Rust promotion"
    )
    try expect(
        quickPromoted.state.pane(paneID: ShellQuickTerminalSlot.globalPaneID)?.spaceID == "space_main",
        "production adapter must promote quick terminal pane into target space through Rust"
    )

    let quickForClose = try adapter.applyReducer(
        state: attention.state,
        operation: .showQuickTerminal(
            workingDirectory: "/repo/quick",
            defaultWorkingDirectory: "/home/test"
        )
    )
    let quickClosed = try adapter.applyReducer(
        state: quickForClose.state,
        operation: .closeQuickTerminal
    )
    try expect(
        quickClosed.state.quickTerminal == nil,
        "production adapter must close quick terminal through Rust"
    )
    try expect(
        quickClosed.state.pane(paneID: ShellQuickTerminalSlot.globalPaneID) == nil,
        "production adapter must remove quick terminal pane after Rust close"
    )

    let secondTab = try attention.state.openingTerminalTab(
        in: "space_main",
        title: "Second",
        workingDirectory: "/repo/app"
    ).state
    let movedLeft = try adapter.applyReducer(
        state: secondTab,
        operation: .moveTab(tabID: "tab_2", sectionOffset: -1)
    )
    try expect(
        movedLeft.state.space(spaceID: "space_main")?.tabs.map(\.tabID) == ["tab_2", "tab_main"],
        "production adapter must apply tab reordering through Rust"
    )
    let movedToSpaceInput = movedLeft.state.creatingSpace(
        launchTarget: .shell,
        title: "Lab",
        workingDirectory: "/repo/lab"
    ).state
    let organizedToSpace = try adapter.applyReducer(
        state: movedToSpaceInput,
        operation: .organizeTab(
            tabID: "tab_2",
            targetSpaceID: "space_2",
            section: .pinned,
            index: 0
        )
    )
    try expect(
        organizedToSpace.state.space(spaceID: "space_2")?.tabs.first?.tabID == "tab_2",
        "production adapter must apply arbitrary tab organization through Rust"
    )
    try expect(
        organizedToSpace.state.tab(tabID: "tab_2")?.isPinned == true,
        "production adapter must apply tab organization section through Rust"
    )
    try expect(
        organizedToSpace.state.pane(paneID: "pane_2")?.spaceID == "space_2",
        "production adapter must update pane space ownership during tab organization"
    )
    let movedToSpace = try adapter.applyReducer(
        state: movedToSpaceInput,
        operation: .moveTabToSpace(tabID: "tab_2", targetSpaceID: "space_2")
    )
    try expect(
        movedToSpace.state.space(spaceID: "space_2")?.tabs.contains { $0.tabID == "tab_2" } == true,
        "production adapter must apply tab move-to-space through Rust"
    )

    let closeInput = try movedToSpace.state.openingTerminalTab(
        in: "space_main",
        title: "Close Me",
        workingDirectory: "/repo/app"
    ).state
    let closed = try adapter.applyReducer(
        state: closeInput,
        operation: .closeTab(tabID: "tab_3")
    )
    try expect(
        closed.state.tab(tabID: "tab_3") == nil,
        "production adapter must apply tab close through Rust"
    )

    let protectedOpened = try closed.state.openingTerminalTab(
        in: "space_main",
        title: "Protected",
        workingDirectory: "/repo/app"
    )
    guard let protectedTabID = protectedOpened.tabID else {
        throw TestFailure.message("protected tab fixture must create a tab")
    }
    let selectedOpened = try protectedOpened.state.openingTerminalTab(
        in: "space_main",
        title: "Selected",
        workingDirectory: "/repo/app"
    )
    guard let selectedTabID = selectedOpened.tabID else {
        throw TestFailure.message("selected tab fixture must create a tab")
    }
    let cleaned = try adapter.applyReducer(
        state: selectedOpened.state,
        operation: .clearInactiveTemporaryTabs(
            spaceID: "space_main",
            protectedTabIDs: [protectedTabID]
        )
    )
    let cleanedTabIDs = cleaned.state.space(spaceID: "space_main")?.tabs.map(\.tabID) ?? []
    try expect(
        cleanedTabIDs == [protectedTabID, selectedTabID],
        "production adapter must clear only inactive unprotected temporary tabs through Rust; got \(cleanedTabIDs)"
    )

    let reservedOpenPaneID = "pane_4"
    let openedTerminal = try adapter.applyReducer(
        state: cleaned.state,
        operation: .openTerminalTab(
            spaceID: "space_main",
            title: "Rust Open",
            workingDirectory: "/repo/opened",
            terminalProfileID: nil,
            reservedPaneSlotIDs: [reservedOpenPaneID]
        )
    )
    guard let openedPaneID = openedTerminal.paneID else {
        throw TestFailure.message("Rust open terminal tab must report a focused pane")
    }
    try expect(
        openedPaneID != reservedOpenPaneID,
        "production adapter must pass reserved pane IDs when opening terminal tabs"
    )
    try expect(
        openedTerminal.state.pane(paneID: openedPaneID)?.cwd == "/repo/opened",
        "production adapter must open terminal tabs through Rust with working directory"
    )

    let reservedSplitPaneID = "pane_6"
    let splitTerminal = try adapter.applyReducer(
        state: openedTerminal.state,
        operation: .splitPane(
            paneSlotID: openedPaneID,
            placement: .right,
            title: nil,
            workingDirectory: "/repo/split",
            terminalProfileID: nil,
            reservedPaneSlotIDs: [reservedSplitPaneID]
        )
    )
    guard let splitPaneID = splitTerminal.paneID else {
        throw TestFailure.message("Rust split pane must report a focused pane")
    }
    try expect(
        splitPaneID != reservedSplitPaneID,
        "production adapter must pass reserved pane IDs when splitting terminal panes"
    )
    try expect(
        splitTerminal.state.pane(paneID: splitPaneID)?.cwd == "/repo/split",
        "production adapter must split terminal panes through Rust with working directory"
    )

    let reservedDuplicatePaneID = "pane_7"
    let duplicated = try adapter.applyReducer(
        state: splitTerminal.state,
        operation: .duplicateTab(
            tabID: selectedTabID,
            reservedPaneSlotIDs: [reservedDuplicatePaneID]
        )
    )
    guard let duplicatedPaneID = duplicated.paneID else {
        throw TestFailure.message("Rust duplicate tab must report a focused pane")
    }
    try expect(
        duplicatedPaneID != reservedDuplicatePaneID,
        "production adapter must pass reserved pane IDs when duplicating terminal tabs"
    )
    try expect(
        duplicated.state.space(spaceID: "space_main")?.tabs.count == 4,
        "production adapter must duplicate terminal-backed tabs through Rust"
    )
    try expect(
        duplicated.state.pane(paneID: duplicatedPaneID)?.cwd == "/repo/app",
        "production adapter must duplicate terminal tabs through Rust with source cwd"
    )

    let reservedSpacePaneID = "pane_8"
    let createdTerminalSpace = try adapter.applyReducer(
        state: duplicated.state,
        operation: .createTerminalSpace(
            title: nil,
            tabTitle: nil,
            workingDirectory: "/repo/generated/.git",
            terminalProfileID: nil,
            presentationIcon: "folder.fill",
            reservedPaneSlotIDs: [reservedSpacePaneID]
        )
    )
    guard let createdSpaceID = createdTerminalSpace.spaceID,
          let createdPaneID = createdTerminalSpace.paneID,
          let createdSpace = createdTerminalSpace.state.space(spaceID: createdSpaceID)
    else {
        throw TestFailure.message("Rust create terminal space must report created focus ids")
    }
    try expect(
        createdPaneID != reservedSpacePaneID,
        "production adapter must pass reserved pane IDs when creating terminal spaces"
    )
    try expect(
        createdSpace.title == "generated",
        "production adapter must derive untitled terminal Space names from working directory"
    )
    try expect(
        createdSpace.presentationIconSystemName == "folder.fill",
        "production adapter must preserve supported presentation icons for terminal Spaces"
    )

    let profiledSpace = try adapter.applyReducer(
        state: createdTerminalSpace.state,
        operation: .setTerminalProfile(
            spaceID: createdSpaceID,
            terminalProfileID: "profile-alt"
        )
    )
    try expect(
        profiledSpace.state.space(spaceID: createdSpaceID)?.terminalProfileID == "profile-alt",
        "production adapter must update Space terminal profiles through Rust"
    )

    let clearedIcon = try adapter.applyReducer(
        state: profiledSpace.state,
        operation: .setPresentationIcon(
            spaceID: createdSpaceID,
            presentationIcon: "not a symbol!!"
        )
    )
    try expect(
        clearedIcon.state.space(spaceID: createdSpaceID)?.presentationIconSystemName == nil,
        "production adapter must clear unsupported presentation icons through Rust"
    )

    let deletedSpace = try adapter.applyReducer(
        state: clearedIcon.state,
        operation: .deleteSpace(
            spaceID: createdSpaceID,
            defaultWorkingDirectory: "/fallback"
        )
    )
    try expect(
        deletedSpace.state.space(spaceID: createdSpaceID) == nil,
        "production adapter must delete Spaces through Rust"
    )
    try expect(
        deletedSpace.state.focusedSpaceID == "space_main",
        "production adapter must repair focus after deleting a Space"
    )

    do {
        _ = try adapter.applyReducer(
            state: state,
            operation: .focusPane(paneSlotID: "pane_missing")
        )
        throw TestFailure.message("missing pane focus must fail")
    } catch ShellStateMutationError.paneNotFound {
        // Expected stable reducer error mapping.
    }
}

private func testProductionAdapterActions() throws {
    let adapter = try ShellCoreFFIAdapter()
    let state = ShellStateSnapshot.bootstrapDefault(
        windowID: "window_main",
        workingDirectory: "/repo/app"
    )

    let actionTitle = try adapter.actionTitle(for: .newTerminalTab)
    try expect(actionTitle == "New Terminal Tab", "production adapter must read action titles from Rust")
    let shortcut = try adapter.defaultActionShortcut(for: .newTerminalTab)
    try expect(shortcut?.key == "t", "production adapter must decode default shortcut key")
    try expect(shortcut?.modifiers == [.command], "production adapter must decode shortcut modifiers")

    let spaceShortcut = try adapter.defaultActionShortcut(
        for: .spaceSelectByIndex,
        target: .spaceIndex(2)
    )
    try expect(spaceShortcut?.key == "3", "production adapter must decode dynamic Space shortcut")

    guard let shortcut else {
        throw TestFailure.message("new terminal shortcut must be present")
    }
    let keyboardAction = try adapter.keyboardAction(for: shortcut)
    try expect(
        keyboardAction == ShellKeyboardAction(id: .newTerminalTab, target: .currentSelection),
        "production adapter must decode keyboard action target"
    )

    let availability = try adapter.actionAvailability(
        .newTerminalTab,
        target: .currentSelection,
        state: state
    )
    try expect(availability == .available, "production adapter must decode available actions")

    let splitResult = try state.splittingPane("pane_1", placement: .right)
    guard let focusedPaneID = splitResult.paneID else {
        throw TestFailure.message("split fixture must create a pane for action execution")
    }
    let focusedState = try adapter.applyReducer(
        state: splitResult.state,
        operation: .focusPane(paneSlotID: focusedPaneID)
    ).state
    var handledEffect: ShellActionEffect?
    let actionResult = try adapter.executeAction(
        .paneFocusLeft,
        target: .currentSelection,
        state: focusedState
    ) { effect in
        handledEffect = effect
        return true
    }
    try expect(actionResult == .executed, "production adapter must execute available action effects")
    try expect(
        handledEffect == .workspaceCommand(.focusLeft),
        "production adapter must map Rust action effect into Swift effect"
    )
}

private func testProductionAdapterControlCommands() throws {
    let adapter = try ShellCoreFFIAdapter()
    let state = ShellStateSnapshot.bootstrapDefault(
        windowID: "window_main",
        workingDirectory: "/repo/app"
    )
    let splitResult = try state.splittingPane("pane_1", placement: .right)
    guard let targetPaneID = splitResult.paneID else {
        throw TestFailure.message("split fixture must create a pane for control command")
    }

    let focusResult = try adapter.handleControlCommand(
        try controlCommand(
            "pane.focus",
            fields: [
                "pane_id": targetPaneID,
            ]
        ),
        state: splitResult.state
    )
    try expect(
        focusResult.updatedState?.focusedPaneID == targetPaneID,
        "control adapter must apply pane focus through Rust"
    )
    try expect(
        focusResult.response.currentFocusedPaneSlotID == targetPaneID,
        "control adapter must project current focus id"
    )

    let sendResult = try adapter.handleControlCommand(
        try controlCommand(
            "terminal.send_text",
            fields: [
                "pane_id": "pane_1",
                "text": "pwd",
            ]
        ),
        state: state
    )
    try expect(
        sendResult.sideEffect == .sendText(paneID: "pane_1", text: "pwd"),
        "control adapter must map terminal text runtime intent to Swift side effect"
    )
    try expect(
        sendResult.response.contentID == ShellContentInstance.terminalContentID(forPaneID: "pane_1"),
        "control adapter must project terminal content id"
    )
}

private func testProductionAdapterTerminalProfiles() throws {
    let adapter = try ShellCoreFFIAdapter()
    let validDefinition = TerminalProfileDefinition(
        id: "alan",
        title: "Alan",
        launch: .sudoUser(unixUser: "alan"),
        defaultWorkingDirectory: "/repo/app",
        presentation: TerminalProfilePresentation(symbolName: "person.crop.circle", colorName: nil)
    )
    let validDocument = TerminalProfileDocument(
        defaultProfileID: "alan",
        profiles: [validDefinition]
    )
    let validValidation = try adapter.validateTerminalProfileDocument(validDocument)
    try expect(
        validValidation.isValid,
        "profile adapter must validate valid profile documents through Rust"
    )

    let invalidDocument = TerminalProfileDocument(
        defaultProfileID: "missing",
        profiles: [
            TerminalProfileDefinition(
                id: "alan",
                title: "",
                launch: .customCommand(""),
                defaultWorkingDirectory: nil,
                presentation: nil
            ),
        ]
    )
    let validation = try adapter.validateTerminalProfileDocument(invalidDocument)
    try expect(
        validation.errors.contains(.missingTitle("alan")),
        "profile adapter must decode missing title errors"
    )
    try expect(
        validation.errors.contains(.missingCustomCommand("alan")),
        "profile adapter must decode missing custom command errors"
    )
    try expect(
        validation.errors.contains(.missingDefaultProfile("missing")),
        "profile adapter must decode missing default profile errors"
    )

    let editorResult = try adapter.makeTerminalProfileDefinition(
        from: TerminalProfileEditorDraft(
            id: " custom ",
            title: " Custom ",
            launchKind: .customCommand,
            customCommand: "  echo hi  ",
            defaultWorkingDirectory: " /repo/custom ",
            managedTerminalAccountID: " account-main "
        )
    )
    try expect(editorResult.isValid, "profile editor adapter must accept valid drafts")
    try expect(
        editorResult.definition?.id == "custom",
        "profile editor adapter must trim ids through Rust"
    )
    try expect(
        editorResult.definition?.title == "Custom",
        "profile editor adapter must trim titles through Rust"
    )
    try expect(
        editorResult.definition?.defaultWorkingDirectory == "/repo/custom",
        "profile editor adapter must normalize working directory through Rust"
    )
    try expect(
        editorResult.definition?.managedTerminalAccountID == "account-main",
        "profile editor adapter must normalize managed account id through Rust"
    )

    let launchIntent = try adapter.resolveTerminalLaunchIntent(
        terminalProfileReference: "alan",
        terminalProfiles: validDocument,
        executablePaths: ["/usr/bin/sudo", "/bin/zsh"],
        environment: ["SHELL": "/bin/zsh"]
    )
    try expect(
        launchIntent.strategy == "terminal_profile_sudo_user",
        "profile launch intent adapter must decode Rust launch strategy"
    )
    try expect(
        launchIntent.launchPath == "/usr/bin/sudo" && launchIntent.arguments == ["-iu", "alan"],
        "profile launch intent adapter must decode Rust launch argv"
    )
    try expect(
        launchIntent.workingDirectory == "/repo/app",
        "profile launch intent adapter must decode Rust profile working directory"
    )
    try expect(
        launchIntent.profileEnvironment["ALAN_TERMINAL_PROFILE_KIND"] == "sudo_user",
        "profile launch intent adapter must decode Rust profile environment"
    )
}

private func controlCommand(
    _ command: String,
    requestID: String = UUID().uuidString.lowercased(),
    fields: [String: Any] = [:]
) throws -> AlanShellControlCommand {
    var payload = fields
    payload["request_id"] = requestID
    payload["command"] = command
    let data = try JSONSerialization.data(withJSONObject: payload)
    return try JSONDecoder().decode(AlanShellControlCommand.self, from: data)
}
