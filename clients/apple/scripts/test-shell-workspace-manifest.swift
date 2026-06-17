import Foundation

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {}

@main
struct ShellWorkspaceManifestTestRunner {
    static func main() throws {
        try ShellWorkspaceManifestTests.run()
        try ShellWorkspaceManifestFixtureExporter.exportIfRequested()
    }
}

private enum ShellWorkspaceManifestTests {
    private static let referenceDate = Date(timeIntervalSince1970: 1_800_000_000)
    private static let twelveHours: TimeInterval = 12 * 60 * 60

    static func run() throws {
        try verifiesMissingManifestCreatesDefaultWithoutMigratingShellState()
        try verifiesCorruptManifestIsQuarantined()
        try verifiesOldManifestDecodesWithoutSpaceLocalSelection()
        try verifiesOldManifestWithoutSpaceIconUsesDefaultWithoutRewriteEvidence()
        try verifiesContentSpaceIconMetadataRoundTripsSeparatelyFromTerminalProfile()
        try verifiesInvalidSpaceIconFallsBackButPreservesManifestEvidence()
        try verifiesMaterializerPreservesEmptySelectedSpace()
        try verifiesMaterializerPreservesEmptySelectedSpaceWithOtherTabs()
        try verifiesMaterializerPreservesInactiveSpaceSelection()
        try verifiesShellCoreFFIMaterializerPreservesPayloadProfileAndTranscript()
        try verifiesManifestRoundTripPreservesSpaceLocalSelection()
        try verifiesPinnedSnapshotWinsOverLaterLiveSnapshot()
        try verifiesPinnedSplitSnapshotRestoresSplitTree()
        try verifiesTerminalOnlySnapshotMigratesToContentContainerShape()
        try verifiesContentContainerMigrationPreservesWorkspaceMetadata()
        try verifiesContentContainerMigrationPreservesNilRestoreCwd()
        try verifiesUnpinnedTabPruningUsesTtlAndActiveTask()
        try verifiesSelectedTabPruningCanLeaveSelectedSpaceEmpty()
        print("Shell workspace manifest tests passed.")
    }

    private static func verifiesMissingManifestCreatesDefaultWithoutMigratingShellState() throws {
        let fileManager = FileManager.default
        let tempDirectory = try makeTempDirectory()
        let manifestURL = tempDirectory.appendingPathComponent("shell-workspace-window_main.json")
        let legacyStateURL = tempDirectory.appendingPathComponent("shell-state-window_main.json")
        let legacyState = ShellStateSnapshot.bootstrapDefault(
            windowID: "window_main",
            workingDirectory: "/legacy/project"
        )

        try JSONEncoder().encode(legacyState).write(to: legacyStateURL)

        let store = ShellWorkspaceManifestStore(fileManager: fileManager, manifestURL: manifestURL)
        let result = try store.loadOrCreateDefault(
            windowID: "window_main",
            defaultWorkingDirectory: "/fresh/project",
            now: referenceDate
        )

        expect(result.recovery == .createdDefault, "missing manifest must report default creation")
        expect(fileManager.fileExists(atPath: manifestURL.path), "missing manifest must write a new manifest")

        let tab = try requireOnlyContentTab(in: result.manifest)
        let content = try requireOnlyTerminalContent(in: tab.liveSnapshot)
        expect(
            content.payload.terminal?.cwd == "/fresh/project",
            "default manifest must use the requested working directory"
        )

        let persistedManifestText = try String(contentsOf: manifestURL, encoding: .utf8)
        expect(
            !persistedManifestText.contains("/legacy/project"),
            "workspace manifest startup must not migrate legacy ShellStateSnapshot data"
        )
        expect(
            !persistedManifestText.contains("\"panes\""),
            "default workspace manifest must not dual-write terminal-only panes"
        )
    }

    private static func verifiesCorruptManifestIsQuarantined() throws {
        let fileManager = FileManager.default
        let tempDirectory = try makeTempDirectory()
        let manifestURL = tempDirectory.appendingPathComponent("shell-workspace-window_main.json")
        try "not json".write(to: manifestURL, atomically: true, encoding: .utf8)

        let store = ShellWorkspaceManifestStore(fileManager: fileManager, manifestURL: manifestURL)
        let result = try store.loadOrCreateDefault(
            windowID: "window_main",
            defaultWorkingDirectory: "/fresh/project",
            now: referenceDate
        )

        guard case .quarantinedCorruptFile(let corruptURL) = result.recovery else {
            throw TestFailure("corrupt manifest must report a quarantine URL")
        }

        expect(fileManager.fileExists(atPath: corruptURL.path), "corrupt manifest must be preserved")
        expect(fileManager.fileExists(atPath: manifestURL.path), "corrupt manifest recovery must write a replacement")
        let quarantinedText = try String(contentsOf: corruptURL, encoding: .utf8)
        expect(
            quarantinedText == "not json",
            "quarantined corrupt file must keep the unreadable payload"
        )
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        _ = try decoder.decode(
            ShellContentWorkspaceManifest.self,
            from: Data(contentsOf: manifestURL)
        )
    }

    private static func verifiesOldManifestDecodesWithoutSpaceLocalSelection() throws {
        let firstTab = makeContentTab(
            tabID: "tab_first",
            title: "First",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/first/project",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let secondTab = makeContentTab(
            tabID: "tab_second",
            title: "Second",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/second/project",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let oldManifest = makeContentManifest(selectedTabID: secondTab.tabID, tabs: [firstTab, secondTab])
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(oldManifest)
        let text = String(data: data, encoding: .utf8) ?? ""
        let selectedTabFieldCount = text.components(separatedBy: "\"selected_tab_id\"").count - 1
        expect(
            selectedTabFieldCount == 1,
            "old-manifest setup must include only the global selected tab field"
        )

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        var decoded = try decoder.decode(ShellContentWorkspaceManifest.self, from: data)
        decoded.repairSelection()

        expect(decoded.selectedTabID == "tab_second", "old manifest must keep global selected tab")
        expect(
            decoded.spaces.first?.selectedTabID == "tab_second",
            "old manifest repair must seed selected space remembered tab from global selected tab"
        )
    }

    private static func verifiesOldManifestWithoutSpaceIconUsesDefaultWithoutRewriteEvidence() throws {
        let tab = makeContentTab(
            tabID: "tab_icon_default",
            title: "Icon Default",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/icon/default",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let oldManifest = makeContentManifest(selectedTabID: tab.tabID, tabs: [tab])
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let oldData = try encoder.encode(oldManifest)
        let oldText = String(data: oldData, encoding: .utf8) ?? ""
        expect(
            !oldText.contains("\"presentation_icon\""),
            "old-manifest setup must not include Space icon metadata"
        )

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(ShellContentWorkspaceManifest.self, from: oldData)
        let state = ShellWorkspaceMaterializer.materialize(
            manifest: decoded,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(
            decoded.spaces.first?.presentationIconSystemName == nil,
            "absent Space icon metadata must remain absent on the decoded manifest record"
        )
        expect(
            state.spaces.first?.presentationIconSystemName == nil,
            "ShellSpace projection must preserve absent explicit icon metadata"
        )
        expect(
            state.spaces.first?.resolvedPresentationIconSystemName
                == ShellSpacePresentationIcon.defaultSystemName,
            "ShellSpace projection must expose a deterministic default icon for display"
        )

        let rewrittenText = String(data: try encoder.encode(decoded), encoding: .utf8) ?? ""
        expect(
            !rewrittenText.contains("\"presentation_icon\""),
            "default display icon must not rewrite old manifest evidence"
        )
    }

    private static func verifiesContentSpaceIconMetadataRoundTripsSeparatelyFromTerminalProfile() throws {
        let tab = makeContentTab(
            tabID: "tab_icon_explicit",
            title: "Icon Explicit",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/icon/explicit",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: tab.tabID,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: tab.tabID,
                    tabs: [tab],
                    terminalProfileID: "alan",
                    presentationIconSystemName: "rectangle.stack.fill"
                )
            ]
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(manifest)
        let json = String(data: data, encoding: .utf8) ?? ""
        expect(json.contains("\"presentation_icon\""), "explicit Space icon must be stored on the Space record")
        expect(json.contains("\"terminal_profile_id\""), "terminal profile reference must remain separately stored")
        expect(
            !json.contains("\"profile_icon\""),
            "Space presentation icon must not be encoded as Terminal Profile icon metadata"
        )

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(ShellContentWorkspaceManifest.self, from: data)
        let state = ShellWorkspaceMaterializer.materialize(
            manifest: decoded,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(
            decoded.spaces.first?.presentationIconSystemName == "rectangle.stack.fill",
            "decoded manifest Space record must preserve explicit icon metadata"
        )
        expect(
            state.spaces.first?.presentationIconSystemName == "rectangle.stack.fill",
            "ShellSpace projection must expose explicit icon metadata"
        )
        expect(
            state.spaces.first?.terminalProfileID == "alan",
            "Space icon metadata must not rewrite the Terminal Profile reference"
        )
    }

    private static func verifiesShellCoreFFIMaterializerPreservesPayloadProfileAndTranscript() throws {
        let paneSlotID = "pane_profile"
        let contentID = "content_\(paneSlotID)"
        let transcript = TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: nil,
            title: "Profile Shell",
            dimensions: nil,
            viewport: nil,
            transcriptLines: ["restored output"],
            processSummary: nil,
            capturedAt: referenceDate,
            alternateScreen: false
        )
        let snapshot = ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode(
                nodeID: "node_\(paneSlotID)",
                kind: .pane,
                direction: nil,
                paneSlotID: paneSlotID,
                children: nil
            ),
            paneSlots: [
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlotID,
                    contentID: contentID
                ),
            ],
            contents: [
                ShellContentRestoreRecord(
                    contentID: contentID,
                    kind: .terminal,
                    title: "Profile Shell",
                    payload: .terminal(
                        ShellTerminalContentPayload(
                            launchTarget: .shell,
                            cwd: nil,
                            title: "Profile Shell",
                            transcriptSnapshot: transcript,
                            terminalProfileID: "profile-missing"
                        )
                    )
                ),
            ]
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: "tab_profile",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_profile",
                    tabs: [
                        ShellContentWorkspaceTabRecord(
                            tabID: "tab_profile",
                            title: "Profile Shell",
                            kind: .terminal,
                            createdAt: referenceDate,
                            lastActivatedAt: referenceDate,
                            lastActivityAt: referenceDate,
                            isPinned: false,
                            liveSnapshot: snapshot,
                            activeTask: .inactive
                        ),
                    ],
                    terminalProfileID: "profile-missing"
                ),
            ]
        )

        let state = try ShellCoreFFIAdapter().materializeContentWorkspaceManifest(
            manifest: manifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )
        let restoredContent = state.contents?.first { $0.contentID == contentID }
        let terminalPayload = restoredContent?.payload.terminal
        expect(
            terminalPayload?.terminalProfileID == "profile-missing",
            "Rust-backed materialize must preserve terminal profile id"
        )
        expect(
            terminalPayload?.cwd == nil,
            "Rust-backed materialize must not apply fallback cwd when terminal profile is pinned"
        )
        expect(
            terminalPayload?.transcriptSnapshot?.transcriptLines == ["restored output"],
            "Rust-backed materialize must preserve restored transcript payload"
        )
        let restoredPane = state.panes.first { $0.paneID == paneSlotID }
        expect(
            restoredPane?.terminalProfileID == "profile-missing",
            "Rust-backed materialize must project terminal profile id into compatibility pane"
        )
        expect(
            restoredPane?.cwd == nil,
            "Rust-backed materialize compatibility pane must not receive fallback cwd for pinned profile"
        )
    }

    private static func verifiesInvalidSpaceIconFallsBackButPreservesManifestEvidence() throws {
        let tab = makeContentTab(
            tabID: "tab_icon_invalid",
            title: "Icon Invalid",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/icon/invalid",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: tab.tabID,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: tab.tabID,
                    tabs: [tab],
                    presentationIconSystemName: "not a renderable symbol"
                )
            ]
        )

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(
            manifest.spaces.first?.presentationIconSystemName == "not a renderable symbol",
            "invalid Space icon evidence must remain on the manifest record"
        )
        expect(
            state.spaces.first?.presentationIconSystemName == "not a renderable symbol",
            "ShellSpace projection must preserve the explicit invalid icon evidence"
        )
        expect(
            state.spaces.first?.resolvedPresentationIconSystemName
                == ShellSpacePresentationIcon.defaultSystemName,
            "unsupported Space icon metadata must fall back to the deterministic display icon"
        )
        expect(
            state.spaces.first?.tabs.first?.tabID == tab.tabID,
            "invalid Space icon metadata must not drop tabs"
        )
    }

    private static func verifiesMaterializerPreservesEmptySelectedSpace() throws {
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_empty",
            selectedTabID: nil,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_empty",
                    title: "Empty",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: []
                )
            ]
        )

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(state.spaces.count == 1, "materializer must preserve empty spaces")
        expect(state.focusedSpaceID == "space_empty", "selected empty space must remain selected")
        expect(state.focusedTabID == nil, "selected empty space must not fabricate a tab selection")
        expect(state.focusedPaneID == nil, "selected empty space must not fabricate a pane selection")
        expect(state.panes.isEmpty, "selected empty space must not fabricate panes")
    }

    private static func verifiesMaterializerPreservesEmptySelectedSpaceWithOtherTabs() throws {
        let otherTab = makeContentTab(
            tabID: "tab_other",
            title: "Other",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/other/project",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_empty",
            selectedTabID: nil,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_empty",
                    title: "Empty",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: []
                ),
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_other",
                    title: "Other",
                    order: 1,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: [otherTab]
                ),
            ]
        )

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(state.spaces.count == 2, "materializer must preserve empty and populated spaces")
        expect(state.focusedSpaceID == "space_empty", "selected empty space must remain focused")
        expect(state.focusedTabID == nil, "selected empty space must not focus another space's tab")
        expect(state.focusedPaneID == nil, "selected empty space must not focus another space's pane")
        expect(state.pane(paneID: "pane_tab_other") != nil, "other space panes must still materialize")
    }

    private static func verifiesMaterializerPreservesInactiveSpaceSelection() throws {
        let mainFirst = makeContentTab(
            tabID: "tab_main_first",
            title: "Main First",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/main/first",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let mainSecond = makeContentTab(
            tabID: "tab_main_second",
            title: "Main Second",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/main/second",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let otherFirst = makeContentTab(
            tabID: "tab_other_first",
            title: "Other First",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/other/first",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_other",
            selectedTabID: "tab_other_first",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_main_second",
                    tabs: [mainFirst, mainSecond]
                ),
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_other",
                    title: "Other",
                    order: 1,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_other_first",
                    tabs: [otherFirst]
                ),
            ]
        )

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(state.focusedSpaceID == "space_other", "materializer must restore globally selected space")
        expect(state.focusedTabID == "tab_other_first", "materializer must restore globally selected tab")
        expect(
            state.space(spaceID: "space_main")?.selectedTabID == "tab_main_second",
            "materializer must preserve inactive space remembered tab"
        )
    }

    private static func verifiesManifestRoundTripPreservesSpaceLocalSelection() throws {
        let mainFirst = makeContentTab(
            tabID: "tab_roundtrip_main_first",
            title: "Main First",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/main/first",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let mainSecond = makeContentTab(
            tabID: "tab_roundtrip_main_second",
            title: "Main Second",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/main/second",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let otherFirst = makeContentTab(
            tabID: "tab_roundtrip_other_first",
            title: "Other First",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/other/first",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let otherSecond = makeContentTab(
            tabID: "tab_roundtrip_other_second",
            title: "Other Second",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/other/second",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_other",
            selectedTabID: "tab_roundtrip_other_second",
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_roundtrip_main_second",
                    tabs: [mainFirst, mainSecond]
                ),
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_other",
                    title: "Other",
                    order: 1,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_roundtrip_other_second",
                    tabs: [otherFirst, otherSecond]
                ),
            ]
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(manifest)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        var decoded = try decoder.decode(ShellContentWorkspaceManifest.self, from: data)
        decoded.repairSelection()

        expect(decoded.selectedSpaceID == "space_other", "round trip must preserve globally selected space")
        expect(decoded.selectedTabID == "tab_roundtrip_other_second", "round trip must preserve active tab")
        expect(
            decoded.spaces.first { $0.spaceID == "space_main" }?.selectedTabID == "tab_roundtrip_main_second",
            "round trip must preserve main space remembered tab"
        )
        expect(
            decoded.spaces.first { $0.spaceID == "space_other" }?.selectedTabID == "tab_roundtrip_other_second",
            "round trip must preserve other space remembered tab"
        )
    }

    private static func verifiesPinnedSnapshotWinsOverLaterLiveSnapshot() throws {
        let tab = makeContentTab(
            tabID: "tab_pinned",
            title: "Pinned",
            isPinned: true,
            pinCwd: "/pinned/project",
            liveCwd: "/later/project",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        let manifest = makeContentManifest(selectedTabID: tab.tabID, tabs: [tab])

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        let pane = try requirePane("pane_tab_pinned", in: state)
        expect(
            pane.cwd == "/pinned/project",
            "pinned restore must use the explicit pin snapshot, not later live cwd"
        )
    }

    private static func verifiesPinnedSplitSnapshotRestoresSplitTree() throws {
        var tab = makeContentTab(
            tabID: "tab_split",
            title: "Pinned Split",
            isPinned: true,
            pinCwd: nil,
            liveCwd: "/live/single",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        tab.pinSnapshot = makeContentSplitSnapshot(tabID: tab.tabID)
        let manifest = makeContentManifest(selectedTabID: tab.tabID, tabs: [tab])

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: manifest,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        let restoredTab = try requireTab("tab_split", in: state)
        expect(restoredTab.paneTree.paneIDs == ["pane_tab_split_left", "pane_tab_split_right"], "pinned split restore must keep the split pane order")
        expect(state.panes(in: "tab_split").count == 2, "pinned split restore must restore both panes")
        expect(
            state.pane(paneID: "pane_tab_split_left")?.cwd == "/pinned/left",
            "pinned split restore must keep left pane cwd"
        )
        expect(
            state.pane(paneID: "pane_tab_split_right")?.cwd == "/pinned/right",
            "pinned split restore must keep right pane cwd"
        )
    }

    private static func verifiesTerminalOnlySnapshotMigratesToContentContainerShape() throws {
        var tab = makeTab(
            tabID: "tab_split",
            title: "Pinned Split",
            isPinned: true,
            pinCwd: nil,
            liveCwd: "/live/single",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        tab.pinSnapshot = makeSplitSnapshot(tabID: tab.tabID)
        let manifest = makeManifest(selectedTabID: tab.tabID, tabs: [tab])

        let migrated = manifest.migratingTerminalRestoreSnapshotsToContentContainers()
        let migratedTab = try requireContentTab("tab_split", in: migrated)
        let snapshot = try requireSnapshot(migratedTab.pinSnapshot)

        expect(
            migrated.contentContractVersion == ShellContentStateSnapshot.currentContractVersion,
            "content manifest migration must use the v0.2 content contract"
        )
        expect(
            snapshot.paneTree.paneSlotIDs == ["pane_tab_split_left", "pane_tab_split_right"],
            "content migration must preserve terminal pane IDs as PaneSlot IDs"
        )
        expect(
            snapshot.paneSlots.map(\.paneSlotID) == ["pane_tab_split_left", "pane_tab_split_right"],
            "content migration must create one PaneSlot per terminal restore pane"
        )
        expect(
            snapshot.paneSlots.map(\.contentID) == [
                "content_pane_tab_split_left",
                "content_pane_tab_split_right",
            ],
            "content migration must assign stable ContentInstance IDs"
        )
        expect(
            snapshot.contents.map(\.kind) == [.terminal, .terminal],
            "terminal-only restore panes must migrate to terminal ContentInstances"
        )
        expect(
            snapshot.contents.map(\.title) == ["Shell", "Shell"],
            "terminal ContentInstances must keep user-facing terminal titles"
        )
        expect(
            snapshot.contents.compactMap(\.payload.terminal?.cwd) == [
                "/pinned/left",
                "/pinned/right",
            ],
            "terminal content payloads must preserve per-pane cwd"
        )
        expect(
            snapshot.contents.allSatisfy { $0.payload.markdown == nil && $0.payload.settings == nil },
            "terminal migration must not fabricate non-terminal payloads"
        )
    }

    private static func verifiesContentContainerMigrationPreservesWorkspaceMetadata() throws {
        let activatedAt = referenceDate.addingTimeInterval(-120)
        let activityAt = referenceDate.addingTimeInterval(-30)
        let tab = makeTab(
            tabID: "tab_active",
            title: "Active",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/fallback",
            lastActivatedAt: activatedAt,
            lastActivityAt: activityAt,
            activeTask: .alanPendingYield
        )
        let manifest = makeManifest(selectedTabID: tab.tabID, tabs: [tab])

        let migrated = manifest.migratingTerminalRestoreSnapshotsToContentContainers()
        let migratedSpace = try requireOnlySpace(in: migrated)
        let migratedTab = try requireOnlyContentTab(in: migrated)

        expect(migrated.selectedSpaceID == manifest.selectedSpaceID, "migration must preserve selected Space")
        expect(migrated.selectedTabID == manifest.selectedTabID, "migration must preserve selected Tab")
        expect(migratedSpace.spaceID == "space_main", "migration must preserve Space identity")
        expect(migratedSpace.order == 0, "migration must preserve Space ordering")
        expect(migratedTab.tabID == tab.tabID, "migration must preserve Tab identity")
        expect(migratedTab.isPinned == tab.isPinned, "migration must preserve pin state")
        expect(
            migratedTab.lastActivatedAt == activatedAt && migratedTab.lastActivityAt == activityAt,
            "migration must preserve TTL anchor timestamps"
        )
        expect(
            migratedTab.activeTask == .alanPendingYield,
            "migration must preserve active-task metadata"
        )
        let snapshot = try requireSnapshot(migratedTab.liveSnapshot)
        expect(
            snapshot.contents.first?.payload.terminal?.cwd == "/fallback",
            "migration must preserve terminal restore payload cwd"
        )
    }

    private static func verifiesContentContainerMigrationPreservesNilRestoreCwd() throws {
        var tab = makeTab(
            tabID: "tab_nil_cwd",
            title: "Nil Cwd",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/will-be-replaced",
            lastActivatedAt: referenceDate,
            lastActivityAt: referenceDate,
            activeTask: .inactive
        )
        tab.liveSnapshot = makeSnapshot(tabID: tab.tabID, cwd: nil)
        let manifest = makeManifest(selectedTabID: tab.tabID, tabs: [tab])

        let migrated = manifest.migratingTerminalRestoreSnapshotsToContentContainers()
        let snapshot = try requireSnapshot(try requireOnlyContentTab(in: migrated).liveSnapshot)

        expect(
            snapshot.contents.first?.payload.terminal?.cwd == nil,
            "migration must preserve nil cwd so restore can resolve the default directory later"
        )

        let state = ShellWorkspaceMaterializer.materialize(
            manifest: migrated,
            defaultWorkingDirectory: "/default/project",
            now: referenceDate
        )
        let pane = try requirePane("pane_tab_nil_cwd", in: state)
        expect(
            pane.cwd == "/default/project",
            "content manifest restore must resolve nil terminal cwd to the workspace default directory"
        )
    }

    private static func verifiesUnpinnedTabPruningUsesTtlAndActiveTask() throws {
        let expiredAt = referenceDate.addingTimeInterval(-(twelveHours + 60))
        let recentAt = referenceDate.addingTimeInterval(-60)
        let expiredInactive = makeContentTab(
            tabID: "tab_expired",
            title: "Expired",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/expired",
            lastActivatedAt: expiredAt,
            lastActivityAt: expiredAt,
            activeTask: .inactive
        )
        let expiredActive = makeContentTab(
            tabID: "tab_active",
            title: "Active",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/active",
            lastActivatedAt: expiredAt,
            lastActivityAt: expiredAt,
            activeTask: .foregroundCommand
        )
        let recentInactive = makeContentTab(
            tabID: "tab_recent",
            title: "Recent",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/recent",
            lastActivatedAt: recentAt,
            lastActivityAt: recentAt,
            activeTask: .inactive
        )
        let manifest = makeContentManifest(
            selectedTabID: expiredInactive.tabID,
            tabs: [expiredInactive, expiredActive, recentInactive]
        )

        let pruned = manifest.pruningExpiredTabs(now: referenceDate, ttl: twelveHours)

        expect(findContentTab("tab_expired", in: pruned) == nil, "expired inactive unpinned tab must be pruned")
        expect(findContentTab("tab_active", in: pruned) != nil, "active unpinned tab must survive TTL pruning")
        expect(findContentTab("tab_recent", in: pruned) != nil, "recent unpinned tab must survive TTL pruning")
        expect(pruned.selectedTabID == "tab_active", "selected pruned tab must repair to first retained tab")
    }

    private static func verifiesSelectedTabPruningCanLeaveSelectedSpaceEmpty() throws {
        let expiredAt = referenceDate.addingTimeInterval(-(twelveHours + 60))
        let expiredInactive = makeContentTab(
            tabID: "tab_expired",
            title: "Expired",
            isPinned: false,
            pinCwd: nil,
            liveCwd: "/expired",
            lastActivatedAt: expiredAt,
            lastActivityAt: expiredAt,
            activeTask: .inactive
        )
        let manifest = makeContentManifest(selectedTabID: expiredInactive.tabID, tabs: [expiredInactive])

        let pruned = manifest.pruningExpiredTabs(now: referenceDate, ttl: twelveHours)
        let state = ShellWorkspaceMaterializer.materialize(
            manifest: pruned,
            defaultWorkingDirectory: "/tmp",
            now: referenceDate
        )

        expect(pruned.spaces.first?.tabs.isEmpty == true, "pruning must keep the selected space even when empty")
        expect(pruned.selectedSpaceID == "space_main", "pruning must preserve selected space")
        expect(pruned.selectedTabID == nil, "pruning all tabs in a selected space must clear selected tab")
        expect(state.focusedSpaceID == "space_main", "materializer must keep the empty selected space focused")
        expect(state.focusedTabID == nil, "materializer must keep empty selected space tabless")
    }

    private static func makeManifest(
        selectedTabID: String?,
        tabs: [ShellWorkspaceTabRecord]
    ) -> ShellWorkspaceManifest {
        ShellWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: selectedTabID,
            spaces: [
                ShellWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: tabs
                )
            ]
        )
    }

    private static func makeContentManifest(
        selectedTabID: String?,
        tabs: [ShellContentWorkspaceTabRecord]
    ) -> ShellContentWorkspaceManifest {
        ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: selectedTabID,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: tabs
                )
            ]
        )
    }

    private static func makeTab(
        tabID: String,
        title: String,
        isPinned: Bool,
        pinCwd: String?,
        liveCwd: String?,
        lastActivatedAt: Date,
        lastActivityAt: Date,
        activeTask: ShellTabActiveTaskState
    ) -> ShellWorkspaceTabRecord {
        ShellWorkspaceTabRecord(
            tabID: tabID,
            title: title,
            kind: .terminal,
            createdAt: referenceDate,
            lastActivatedAt: lastActivatedAt,
            lastActivityAt: lastActivityAt,
            isPinned: isPinned,
            pinSnapshot: pinCwd.map { makeSnapshot(tabID: tabID, cwd: $0) },
            liveSnapshot: liveCwd.map { makeSnapshot(tabID: tabID, cwd: $0) },
            activeTask: activeTask
        )
    }

    private static func makeContentTab(
        tabID: String,
        title: String,
        isPinned: Bool,
        pinCwd: String?,
        liveCwd: String?,
        lastActivatedAt: Date,
        lastActivityAt: Date,
        activeTask: ShellTabActiveTaskState
    ) -> ShellContentWorkspaceTabRecord {
        ShellContentWorkspaceTabRecord(
            tabID: tabID,
            title: title,
            kind: .terminal,
            createdAt: referenceDate,
            lastActivatedAt: lastActivatedAt,
            lastActivityAt: lastActivityAt,
            isPinned: isPinned,
            pinSnapshot: pinCwd.map { makeContentSnapshot(tabID: tabID, cwd: $0) },
            liveSnapshot: liveCwd.map { makeContentSnapshot(tabID: tabID, cwd: $0) },
            activeTask: activeTask
        )
    }

    private static func makeSnapshot(tabID: String, cwd: String?) -> ShellTabRestoreSnapshot {
        let paneID = "pane_\(tabID)"
        return ShellTabRestoreSnapshot(
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            ),
            panes: [
                ShellPaneRestoreRecord(
                    paneID: paneID,
                    launchTarget: .shell,
                    cwd: cwd,
                    title: nil
                )
            ]
        )
    }

    private static func makeContentSnapshot(
        tabID: String,
        cwd: String?
    ) -> ShellContentTabRestoreSnapshot {
        let paneSlotID = "pane_\(tabID)"
        let contentID = "content_\(paneSlotID)"
        return ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode(
                nodeID: "node_\(paneSlotID)",
                kind: .pane,
                direction: nil,
                paneSlotID: paneSlotID,
                children: nil
            ),
            paneSlots: [
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlotID,
                    contentID: contentID
                )
            ],
            contents: [
                ShellContentRestoreRecord(
                    contentID: contentID,
                    kind: .terminal,
                    title: "Shell",
                    payload: .terminal(
                        ShellTerminalContentPayload(
                            launchTarget: .shell,
                            cwd: cwd,
                            title: "Shell"
                        )
                    )
                )
            ]
        )
    }

    private static func makeSplitSnapshot(tabID: String) -> ShellTabRestoreSnapshot {
        let leftPaneID = "pane_\(tabID)_left"
        let rightPaneID = "pane_\(tabID)_right"
        return ShellTabRestoreSnapshot(
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(tabID)_split",
                kind: .split,
                direction: .vertical,
                ratio: 0.5,
                paneID: nil,
                children: [
                    ShellPaneTreeNode(
                        nodeID: "node_\(leftPaneID)",
                        kind: .pane,
                        direction: nil,
                        paneID: leftPaneID,
                        children: nil
                    ),
                    ShellPaneTreeNode(
                        nodeID: "node_\(rightPaneID)",
                        kind: .pane,
                        direction: nil,
                        paneID: rightPaneID,
                        children: nil
                    ),
                ]
            ),
            panes: [
                ShellPaneRestoreRecord(
                    paneID: leftPaneID,
                    launchTarget: .shell,
                    cwd: "/pinned/left",
                    title: nil
                ),
                ShellPaneRestoreRecord(
                    paneID: rightPaneID,
                    launchTarget: .shell,
                    cwd: "/pinned/right",
                    title: nil
                ),
            ]
        )
    }

    private static func makeContentSplitSnapshot(tabID: String) -> ShellContentTabRestoreSnapshot {
        makeSplitSnapshot(tabID: tabID).migratingTerminalPanesToContentContainers()
    }

    private static func findContentTab(
        _ tabID: String,
        in manifest: ShellContentWorkspaceManifest
    ) -> ShellContentWorkspaceTabRecord? {
        manifest.spaces.flatMap(\.tabs).first { $0.tabID == tabID }
    }

    private static func requireOnlySpace(
        in manifest: ShellContentWorkspaceManifest
    ) throws -> ShellContentWorkspaceSpaceRecord {
        guard manifest.spaces.count == 1, let space = manifest.spaces.first else {
            throw TestFailure("expected exactly one content space")
        }
        return space
    }

    private static func requireOnlyContentTab(
        in manifest: ShellContentWorkspaceManifest
    ) throws -> ShellContentWorkspaceTabRecord {
        let tabs = manifest.spaces.flatMap(\.tabs)
        guard tabs.count == 1, let tab = tabs.first else {
            throw TestFailure("expected exactly one content tab")
        }
        return tab
    }

    private static func requireContentTab(
        _ tabID: String,
        in manifest: ShellContentWorkspaceManifest
    ) throws -> ShellContentWorkspaceTabRecord {
        guard let tab = manifest.spaces.flatMap(\.tabs).first(where: { $0.tabID == tabID }) else {
            throw TestFailure("missing content tab \(tabID)")
        }
        return tab
    }

    private static func requireSnapshot(
        _ snapshot: ShellContentTabRestoreSnapshot?
    ) throws -> ShellContentTabRestoreSnapshot {
        guard let snapshot else {
            throw TestFailure("expected content restore snapshot")
        }
        return snapshot
    }

    private static func requireOnlyTerminalContent(
        in snapshot: ShellContentTabRestoreSnapshot?
    ) throws -> ShellContentRestoreRecord {
        let snapshot = try requireSnapshot(snapshot)
        guard snapshot.contents.count == 1,
              let content = snapshot.contents.first,
              content.kind == .terminal,
              content.payload.terminal != nil
        else {
            throw TestFailure("expected exactly one terminal content restore record")
        }
        return content
    }

    private static func requirePane(_ paneID: String, in state: ShellStateSnapshot) throws -> ShellPane {
        guard let pane = state.pane(paneID: paneID) else {
            throw TestFailure("missing pane \(paneID)")
        }
        return pane
    }

    private static func requireTab(_ tabID: String, in state: ShellStateSnapshot) throws -> ShellTab {
        guard let tab = state.tab(tabID: tabID) else {
            throw TestFailure("missing tab \(tabID)")
        }
        return tab
    }

    private static func makeTempDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("alan-shell-workspace-manifest-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: String
    ) {
        guard condition() else {
            fputs("error: \(message)\n", stderr)
            exit(1)
        }
    }
}

private enum ShellWorkspaceManifestFixtureExporter {
    private static let referenceDate = Date(timeIntervalSince1970: 1_800_000_000)
    private static let referenceDateString = "2027-01-15T08:00:00Z"
    private static let twelveHours: TimeInterval = 12 * 60 * 60

    static func exportIfRequested() throws {
        guard let rootPath = ProcessInfo.processInfo.environment["ALAN_SHELL_MANIFEST_FIXTURE_DIR"],
              !rootPath.isEmpty
        else {
            return
        }

        let rootURL = URL(fileURLWithPath: rootPath)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        for fixture in try fixtures() {
            let fixtureURL = rootURL
                .appendingPathComponent(fixture.id)
                .appendingPathExtension("json")
            try FileManager.default.createDirectory(
                at: fixtureURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try encoder.encode(fixture).write(to: fixtureURL, options: .atomic)
        }
        print("Shell workspace manifest fixtures exported to \(rootPath).")
    }

    private static func fixtures() throws -> [ShellCoreFixtureCase] {
        let defaultManifest = ShellContentWorkspaceManifest.defaultManifest(
            windowID: "window_main",
            defaultWorkingDirectory: "/repo/app",
            now: referenceDate
        )
        let defaultState = ShellWorkspaceMaterializer.materialize(
            manifest: defaultManifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )

        let emptySelectedManifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_empty",
            selectedTabID: nil,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_empty",
                    title: "Empty",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: []
                ),
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_other",
                    title: "Other",
                    order: 1,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_other",
                    tabs: [
                        makeContentTab(
                            tabID: "tab_other",
                            title: "Other",
                            cwd: "/other"
                        ),
                    ],
                    presentationIconSystemName: "rectangle.stack.fill"
                ),
            ]
        )
        let emptySelectedState = ShellWorkspaceMaterializer.materialize(
            manifest: emptySelectedManifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )

        let expiredAt = referenceDate.addingTimeInterval(-(twelveHours + 60))
        let recentAt = referenceDate.addingTimeInterval(-60)
        let pruneInput = makeContentManifest(
            selectedTabID: "tab_expired",
            tabs: [
                makeContentTab(
                    tabID: "tab_expired",
                    title: "Expired",
                    cwd: "/expired",
                    lastActivatedAt: expiredAt,
                    lastActivityAt: expiredAt,
                    activeTask: .inactive
                ),
                makeContentTab(
                    tabID: "tab_active",
                    title: "Active",
                    cwd: "/active",
                    lastActivatedAt: expiredAt,
                    lastActivityAt: expiredAt,
                    activeTask: .foregroundCommand
                ),
                makeContentTab(
                    tabID: "tab_recent",
                    title: "Recent",
                    cwd: "/recent",
                    lastActivatedAt: recentAt,
                    lastActivityAt: recentAt,
                    activeTask: .inactive
                ),
            ]
        )
        let pruned = pruneInput.pruningExpiredTabs(now: referenceDate, ttl: twelveHours)
        let pinnedManifest = makeContentManifest(
            selectedTabID: "tab_pinned",
            tabs: [
                makeContentTab(
                    tabID: "tab_pinned",
                    title: "Pinned",
                    cwd: "/live/project",
                    isPinned: true,
                    pinCwd: "/pinned/project"
                ),
            ]
        )
        let pinnedState = ShellWorkspaceMaterializer.materialize(
            manifest: pinnedManifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )
        let legacyTerminalManifest = makeLegacyTerminalManifest()
        let migratedLegacyManifest =
            legacyTerminalManifest.migratingTerminalRestoreSnapshotsToContentContainers()
        var quickTerminalManifest = makeContentManifest(
            selectedTabID: "tab_main",
            tabs: [
                makeContentTab(
                    tabID: "tab_main",
                    title: "Main",
                    cwd: "/main"
                ),
            ]
        )
        quickTerminalManifest.quickTerminal = ShellQuickTerminalRestoreRecord(
            paneID: ShellQuickTerminalSlot.globalPaneID,
            presentation: .visible,
            lastWorkingDirectory: nil,
            liveSnapshot: makeContentSnapshot(
                paneSlotID: ShellQuickTerminalSlot.globalPaneID,
                title: "python server",
                cwd: "/repo/quick"
            ),
            activeTask: .foregroundCommand
        )
        let quickTerminalState = ShellWorkspaceMaterializer.materialize(
            manifest: quickTerminalManifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )
        let missingProfileManifest = makeContentManifest(
            selectedTabID: "tab_profile",
            terminalProfileID: "profile-missing",
            tabs: [
                makeContentTab(
                    tabID: "tab_profile",
                    title: "Profile Shell",
                    cwd: nil,
                    terminalProfileID: "profile-missing"
                ),
            ]
        )
        let missingProfileState = ShellWorkspaceMaterializer.materialize(
            manifest: missingProfileManifest,
            defaultWorkingDirectory: "/fallback",
            now: referenceDate
        )
        let corruptInput = "{ this is not valid json"
        let malformedManifestInput = """
        {
          "schema_version": 1,
          "content_contract_version": "0.2",
          "window_id": "window_main",
          "selected_space_id": "space_main",
          "spaces": [
            {
              "space_id": "space_main",
              "title": "Main",
              "order": "not-an-integer",
              "created_at": "2027-01-15T08:00:00Z",
              "updated_at": "2027-01-15T08:00:00Z",
              "tabs": []
            }
          ]
        }
        """

        return [
            ShellCoreFixtureCase(
                id: "manifest/default-manifest-materialize",
                kind: "manifest",
                description: "Default content manifest materializes one selected terminal workspace.",
                input: EmptyFixtureInput(),
                operation: DefaultManifestOperation(
                    windowID: "window_main",
                    defaultWorkingDirectory: "/repo/app",
                    now: referenceDateString,
                    materializeDefaultWorkingDirectory: "/fallback"
                ),
                expected: ManifestAndStateExpectation(
                    manifest: defaultManifest,
                    state: PortableWorkspaceState(defaultState)
                )
            ),
            ShellCoreFixtureCase(
                id: "manifest/materialize-empty-selected-space",
                kind: "manifest",
                description: "Materialization preserves an empty selected Space while restoring inactive Space tabs.",
                input: emptySelectedManifest,
                operation: MaterializeManifestOperation(
                    defaultWorkingDirectory: "/fallback",
                    now: referenceDateString
                ),
                expected: StateExpectation(state: PortableWorkspaceState(emptySelectedState))
            ),
            ShellCoreFixtureCase(
                id: "manifest/pruning-expired-tabs",
                kind: "manifest",
                description: "TTL pruning removes expired inactive tabs while retaining active and recent tabs.",
                input: pruneInput,
                operation: PruneExpiredTabsOperation(
                    now: referenceDateString,
                    ttlSeconds: Int(twelveHours)
                ),
                expected: ManifestExpectation(manifest: pruned)
            ),
            ShellCoreFixtureCase(
                id: "manifest/materialize-pinned-snapshot",
                kind: "manifest",
                description: "Pinned tabs restore from pin snapshot instead of newer live snapshot.",
                input: pinnedManifest,
                operation: MaterializeManifestOperation(
                    defaultWorkingDirectory: "/fallback",
                    now: referenceDateString
                ),
                expected: StateExpectation(state: PortableWorkspaceState(pinnedState))
            ),
            ShellCoreFixtureCase(
                id: "manifest/migrate-legacy-terminal-manifest",
                kind: "manifest",
                description: "Legacy terminal-only manifest migrates into content-container snapshot shape.",
                input: legacyTerminalManifest,
                operation: MigrateLegacyManifestOperation(),
                expected: ManifestExpectation(manifest: migratedLegacyManifest)
            ),
            ShellCoreFixtureCase(
                id: "manifest/materialize-quick-terminal",
                kind: "manifest",
                description: "Quick terminal restore materializes hidden runtime metadata outside normal tabs.",
                input: quickTerminalManifest,
                operation: MaterializeManifestOperation(
                    defaultWorkingDirectory: "/fallback",
                    now: referenceDateString
                ),
                expected: StateExpectation(state: PortableWorkspaceState(quickTerminalState))
            ),
            ShellCoreFixtureCase(
                id: "manifest/materialize-missing-profile-reference",
                kind: "manifest",
                description: "Materialization preserves missing Terminal Profile references without applying fallback cwd.",
                input: missingProfileManifest,
                operation: MaterializeManifestOperation(
                    defaultWorkingDirectory: "/fallback",
                    now: referenceDateString
                ),
                expected: StateExpectation(state: PortableWorkspaceState(missingProfileState))
            ),
            ShellCoreFixtureCase(
                id: "manifest/decode-corrupt-input",
                kind: "manifest",
                description: "Corrupt manifest JSON maps to a stable decode error.",
                input: corruptInput,
                operation: DecodeManifestJSONOperation(),
                expected: DecodeManifestExpectation(rawJSONString: corruptInput)
            ),
            ShellCoreFixtureCase(
                id: "manifest/decode-malformed-content-manifest",
                kind: "manifest",
                description: "Malformed content manifest JSON maps to a stable decode error.",
                input: malformedManifestInput,
                operation: DecodeManifestJSONOperation(),
                expected: DecodeManifestExpectation(rawJSONString: malformedManifestInput)
            ),
        ]
    }

    private static func makeLegacyTerminalManifest() -> ShellWorkspaceManifest {
        ShellWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: "tab_legacy",
            spaces: [
                ShellWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    selectedTabID: "tab_legacy",
                    tabs: [
                        ShellWorkspaceTabRecord(
                            tabID: "tab_legacy",
                            title: "Legacy",
                            kind: .terminal,
                            createdAt: referenceDate,
                            lastActivatedAt: referenceDate,
                            lastActivityAt: referenceDate,
                            isPinned: true,
                            isTitleUserLocked: true,
                            pinSnapshot: makeLegacySnapshot(
                                paneID: "pane_legacy",
                                title: "Legacy",
                                cwd: "/legacy/pinned",
                                terminalProfileID: "profile-main"
                            ),
                            liveSnapshot: nil,
                            activeTask: .inactive
                        ),
                    ],
                    terminalProfileID: "profile-main",
                    presentationIconSystemName: "rectangle.stack.fill"
                ),
            ]
        )
    }

    private static func makeContentManifest(
        selectedTabID: String?,
        terminalProfileID: String? = nil,
        tabs: [ShellContentWorkspaceTabRecord]
    ) -> ShellContentWorkspaceManifest {
        ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: "window_main",
            selectedSpaceID: "space_main",
            selectedTabID: selectedTabID,
            spaces: [
                ShellContentWorkspaceSpaceRecord(
                    spaceID: "space_main",
                    title: "Main",
                    order: 0,
                    createdAt: referenceDate,
                    updatedAt: referenceDate,
                    tabs: tabs,
                    terminalProfileID: terminalProfileID
                ),
            ]
        )
    }

    private static func makeContentTab(
        tabID: String,
        title: String,
        cwd: String?,
        isPinned: Bool = false,
        pinCwd: String? = nil,
        lastActivatedAt: Date? = nil,
        lastActivityAt: Date? = nil,
        activeTask: ShellTabActiveTaskState = .inactive,
        terminalProfileID: String? = nil
    ) -> ShellContentWorkspaceTabRecord {
        ShellContentWorkspaceTabRecord(
            tabID: tabID,
            title: title,
            kind: .terminal,
            createdAt: referenceDate,
            lastActivatedAt: lastActivatedAt ?? referenceDate,
            lastActivityAt: lastActivityAt ?? referenceDate,
            isPinned: isPinned,
            pinSnapshot: pinCwd.map {
                makeContentSnapshot(
                    tabID: tabID,
                    title: title,
                    cwd: $0,
                    terminalProfileID: terminalProfileID
                )
            },
            liveSnapshot: makeContentSnapshot(
                tabID: tabID,
                title: title,
                cwd: cwd,
                terminalProfileID: terminalProfileID
            ),
            activeTask: activeTask
        )
    }

    private static func makeContentSnapshot(
        tabID: String,
        title: String,
        cwd: String?,
        terminalProfileID: String? = nil
    ) -> ShellContentTabRestoreSnapshot {
        let paneSlotID = "pane_\(tabID)"
        let contentID = "content_\(paneSlotID)"
        return makeContentSnapshot(
            paneSlotID: paneSlotID,
            title: title,
            cwd: cwd,
            contentID: contentID,
            terminalProfileID: terminalProfileID
        )
    }

    private static func makeContentSnapshot(
        paneSlotID: String,
        title: String,
        cwd: String?,
        contentID: String? = nil,
        terminalProfileID: String? = nil
    ) -> ShellContentTabRestoreSnapshot {
        let contentID = contentID ?? "content_\(paneSlotID)"
        return ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode(
                nodeID: "node_\(paneSlotID)",
                kind: .pane,
                direction: nil,
                paneSlotID: paneSlotID,
                children: nil
            ),
            paneSlots: [
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlotID,
                    contentID: contentID
                ),
            ],
            contents: [
                ShellContentRestoreRecord(
                    contentID: contentID,
                    kind: .terminal,
                    title: title,
                    payload: .terminal(
                        ShellTerminalContentPayload(
                            launchTarget: .shell,
                            cwd: cwd,
                            title: title,
                            terminalProfileID: terminalProfileID
                        )
                    )
                ),
            ]
        )
    }

    private static func makeLegacySnapshot(
        paneID: String,
        title: String,
        cwd: String,
        terminalProfileID: String?
    ) -> ShellTabRestoreSnapshot {
        ShellTabRestoreSnapshot(
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            ),
            panes: [
                ShellPaneRestoreRecord(
                    paneID: paneID,
                    launchTarget: .shell,
                    cwd: cwd,
                    title: title,
                    terminalProfileID: terminalProfileID
                ),
            ]
        )
    }
}

private struct ShellCoreFixtureCase: Encodable {
    let id: String
    let kind: String
    let source = "swift"
    let description: String
    let input: AnyEncodable
    let operation: AnyEncodable
    let expected: AnyEncodable

    init<Input: Encodable, Operation: Encodable, Expected: Encodable>(
        id: String,
        kind: String,
        description: String,
        input: Input,
        operation: Operation,
        expected: Expected
    ) {
        self.id = id
        self.kind = kind
        self.description = description
        self.input = AnyEncodable(input)
        self.operation = AnyEncodable(operation)
        self.expected = AnyEncodable(expected)
    }
}

private struct AnyEncodable: Encodable {
    private let encodeValue: (Encoder) throws -> Void

    init<Value: Encodable>(_ value: Value) {
        encodeValue = value.encode(to:)
    }

    func encode(to encoder: Encoder) throws {
        try encodeValue(encoder)
    }
}

private struct EmptyFixtureInput: Encodable {}

private struct DefaultManifestOperation: Encodable {
    let type = "default_manifest"
    let windowID: String
    let defaultWorkingDirectory: String
    let now: String
    let materializeDefaultWorkingDirectory: String

    private enum CodingKeys: String, CodingKey {
        case type
        case windowID = "window_id"
        case defaultWorkingDirectory = "default_working_directory"
        case now
        case materializeDefaultWorkingDirectory = "materialize_default_working_directory"
    }
}

private struct MaterializeManifestOperation: Encodable {
    let type = "materialize"
    let defaultWorkingDirectory: String
    let now: String

    private enum CodingKeys: String, CodingKey {
        case type
        case defaultWorkingDirectory = "default_working_directory"
        case now
    }
}

private struct PruneExpiredTabsOperation: Encodable {
    let type = "pruning_expired_tabs"
    let now: String
    let ttlSeconds: Int

    private enum CodingKeys: String, CodingKey {
        case type
        case now
        case ttlSeconds = "ttl_seconds"
    }
}

private struct MigrateLegacyManifestOperation: Encodable {
    let type = "migrate_legacy_terminal_manifest"
}

private struct DecodeManifestJSONOperation: Encodable {
    let type = "decode_content_manifest_json"
}

private struct ManifestAndStateExpectation: Encodable {
    let status = "ok"
    let manifest: ShellContentWorkspaceManifest
    let state: PortableWorkspaceState
}

private struct StateExpectation: Encodable {
    let status = "ok"
    let state: PortableWorkspaceState
}

private struct ManifestExpectation: Encodable {
    let status = "ok"
    let manifest: ShellContentWorkspaceManifest
}

private struct DecodeManifestExpectation: Encodable {
    let status: String
    let manifest: ShellContentWorkspaceManifest?
    let errorCode: String?

    init(rawJSONString: String) {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        if let manifest = try? decoder.decode(
            ShellContentWorkspaceManifest.self,
            from: Data(rawJSONString.utf8)
        ) {
            status = "ok"
            self.manifest = manifest
            errorCode = nil
        } else {
            status = "error"
            manifest = nil
            errorCode = "decode_error"
        }
    }

    private enum CodingKeys: String, CodingKey {
        case status
        case manifest
        case errorCode = "error_code"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(status, forKey: .status)
        try container.encodeIfPresent(manifest, forKey: .manifest)
        try container.encodeIfPresent(errorCode, forKey: .errorCode)
    }
}

private struct PortableWorkspaceState: Encodable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [PortableSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [PortableContentInstance]
    let quickTerminal: PortableQuickTerminalState?

    init(_ state: ShellStateSnapshot) {
        let contentState = state.contentStateProjection()
        contractVersion = contentState.contractVersion
        windowID = contentState.windowID
        focusedSpaceID = state.focusedSpaceID
        focusedTabID = state.focusedTabID
        focusedPaneID = state.focusedPaneID
        spaces = contentState.spaces.map(PortableSpace.init)
        paneSlots = contentState.paneSlots
        contents = contentState.contents.map(PortableContentInstance.init)
        quickTerminal = PortableQuickTerminalState(state)
    }

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
        case quickTerminal = "quick_terminal"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(contractVersion, forKey: .contractVersion)
        try container.encode(windowID, forKey: .windowID)
        try container.encode(focusedSpaceID, forKey: .focusedSpaceID)
        try container.encode(focusedTabID, forKey: .focusedTabID)
        try container.encode(focusedPaneID, forKey: .focusedPaneID)
        try container.encode(spaces, forKey: .spaces)
        try container.encode(paneSlots, forKey: .paneSlots)
        try container.encode(contents, forKey: .contents)
        try container.encodeIfPresent(quickTerminal, forKey: .quickTerminal)
    }
}

private struct PortableQuickTerminalState: Encodable {
    let paneID: String
    let presentation: ShellQuickTerminalPresentation
    let lastWorkingDirectory: String?
    let contentID: String
    let terminalMetadata: PortableTerminalRuntimeMetadata?
    let attention: ShellAttentionState

    init?(_ state: ShellStateSnapshot) {
        guard let slot = state.quickTerminal,
              let pane = state.pane(paneID: slot.paneID)
        else {
            return nil
        }

        let contentID = ShellContentInstance.terminalContentID(forPaneID: slot.paneID)
        let content = state.contents?.first { $0.contentID == contentID }
        paneID = slot.paneID
        presentation = slot.presentation
        lastWorkingDirectory = slot.lastWorkingDirectory
        self.contentID = contentID
        terminalMetadata = content?.payload.terminal.map(PortableTerminalRuntimeMetadata.init)
            ?? PortableTerminalRuntimeMetadata(title: pane.viewport?.title, cwd: pane.cwd)
        attention = pane.attention
    }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
        case contentID = "content_id"
        case terminalMetadata = "terminal_metadata"
        case attention
    }
}

private struct PortableSpace: Encodable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [PortableTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIcon: String?

    init(_ space: ShellContentSpace) {
        spaceID = space.spaceID
        title = space.title
        attention = space.attention
        tabs = space.tabs.map(PortableTab.init)
        selectedTabID = space.selectedTabID
        terminalProfileID = space.terminalProfileID
        presentationIcon = space.presentationIconSystemName
    }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIcon = "presentation_icon"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(spaceID, forKey: .spaceID)
        try container.encode(title, forKey: .title)
        try container.encode(attention, forKey: .attention)
        try container.encode(tabs, forKey: .tabs)
        try container.encode(selectedTabID, forKey: .selectedTabID)
        try container.encode(terminalProfileID, forKey: .terminalProfileID)
        try container.encode(presentationIcon, forKey: .presentationIcon)
    }
}

private struct PortableTab: Encodable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: PortablePaneTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    init(_ tab: ShellContentTab) {
        tabID = tab.tabID
        kind = tab.kind
        title = tab.title
        paneTree = PortablePaneTreeNode(tab.paneTree)
        isPinned = tab.isPinned
        isTitleUserLocked = tab.isTitleUserLocked
    }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(tabID, forKey: .tabID)
        try container.encode(kind, forKey: .kind)
        try container.encode(title, forKey: .title)
        try container.encode(paneTree, forKey: .paneTree)
        try container.encode(isPinned, forKey: .isPinned)
        try container.encode(isTitleUserLocked, forKey: .isTitleUserLocked)
    }
}

private struct PortablePaneTreeNode: Encodable {
    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneID: String?
    let children: [PortablePaneTreeNode]?

    init(_ node: ShellPaneSlotTreeNode) {
        nodeID = node.nodeID
        kind = node.kind
        direction = node.direction
        ratio = node.ratio
        paneID = node.paneSlotID
        children = node.children?.map(PortablePaneTreeNode.init)
    }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneID = "pane_id"
        case children
    }
}

private struct PortableContentInstance: Encodable {
    let contentID: String
    let kind: ShellContentKind
    let title: String
    let iconName: String?
    let capabilities: [ShellContentCapability]
    let terminalMetadata: PortableTerminalRuntimeMetadata?
    let lifecycle: ShellContentLifecycleState

    init(_ content: ShellContentInstance) {
        contentID = content.contentID
        kind = content.kind
        title = content.title
        iconName = content.iconName
        capabilities = content.capabilities
        terminalMetadata = content.payload.terminal.map(PortableTerminalRuntimeMetadata.init)
        lifecycle = content.lifecycle
    }

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case iconName = "icon_name"
        case capabilities
        case terminalMetadata = "terminal_metadata"
        case lifecycle
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(contentID, forKey: .contentID)
        try container.encode(kind, forKey: .kind)
        try container.encode(title, forKey: .title)
        try container.encode(iconName, forKey: .iconName)
        try container.encode(capabilities, forKey: .capabilities)
        try container.encode(terminalMetadata, forKey: .terminalMetadata)
        try container.encode(lifecycle, forKey: .lifecycle)
    }
}

private struct PortableTerminalRuntimeMetadata: Encodable {
    let title: String?
    let cwd: String?
    let activeTaskState: ShellTabActiveTaskState = .inactive
    let activity: TerminalActivitySnapshot? = nil

    init(_ payload: ShellTerminalContentPayload) {
        title = payload.title
        cwd = payload.cwd
    }

    init(title: String?, cwd: String?) {
        self.title = title
        self.cwd = cwd
    }

    private enum CodingKeys: String, CodingKey {
        case title
        case cwd
        case activeTaskState = "active_task_state"
        case activity
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(title, forKey: .title)
        try container.encode(cwd, forKey: .cwd)
        try container.encode(activeTaskState, forKey: .activeTaskState)
        try container.encode(activity, forKey: .activity)
    }
}

private struct TestFailure: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
