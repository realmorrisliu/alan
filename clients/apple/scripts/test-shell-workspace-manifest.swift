import Foundation

@main
struct ShellWorkspaceManifestTestRunner {
    static func main() throws {
        try ShellWorkspaceManifestTests.run()
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

private struct TestFailure: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
