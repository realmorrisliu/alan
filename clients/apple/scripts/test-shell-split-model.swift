import Foundation

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {}

@main
struct ShellSplitModelTestRunner {
    static func main() throws {
        try ShellSplitModelTests.run()
        try ShellSplitModelFixtureExporter.exportIfRequested()
    }
}

private enum ShellSplitModelTests {
    static func run() throws {
        try verifiesNewSplitsStoreEqualRatio()
        try verifiesDirectionalSplitsPlaceNewPaneOnRequestedSide()
        try verifiesSplitRatiosClampWhenResized()
        try verifiesEqualizeRestoresEverySplitRatio()
        try verifiesZoomProjectionUsesLeafWithoutMutatingSplitTree()
        try verifiesInTabPaneMovementPreservesPaneIdentityAndRepairsTree()
        try verifiesInvalidInTabPaneMovementLeavesStateUnchanged()
        try verifiesSameDirectionAttachKeepsBinarySplitTree()
        try verifiesSidebarSplitTopologyProjection()
        try verifiesSpatialFocusFollowsSplitTree()
        try verifiesSpatialFocusPreservesPerpendicularPosition()
        try verifiesPaneScopedCloseRemovesSelectedPane()
        try verifiesPaneScopedCloseKeepsInactivePaneTargeting()
        try verifiesPaneScopedCloseClosesSinglePaneTab()
        try verifiesPaneScopedCloseLeavesFinalSpaceEmpty()
        try verifiesClosingBackgroundSpaceTabPreservesCurrentFocus()
        try verifiesDeletingBackgroundSpacePreservesCurrentFocus()
        try verifiesPaneAllocationSkipsReservedRuntimeIDs()
        try verifiesSplitDecodeRequiresPersistedRatio()
        print("Shell split model tests passed.")
    }

    private static func verifiesNewSplitsStoreEqualRatio() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let result = try state.splittingPane("pane_1", direction: .vertical)
        let tree = try requireFocusedTabTree(result.state)

        expect(tree.kind == .split, "splitting a pane must create a split branch")
        expect(tree.ratio == 0.5, "new split branches must persist an equal divider ratio")
        expect(tree.children?.count == 2, "a split branch must keep two structural children")
    }

    private static func verifiesDirectionalSplitsPlaceNewPaneOnRequestedSide() throws {
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        let rightTree = try requireFocusedTabTree(try base.splittingPane("pane_1", placement: .right).state)
        expect(rightTree.direction == .vertical, "split right must create a vertical split branch")
        expect(rightTree.paneIDs == ["pane_1", "pane_2"], "split right must place the new pane after the focused pane")

        let leftTree = try requireFocusedTabTree(try base.splittingPane("pane_1", placement: .left).state)
        expect(leftTree.direction == .vertical, "split left must create a vertical split branch")
        expect(leftTree.paneIDs == ["pane_2", "pane_1"], "split left must place the new pane before the focused pane")

        let downTree = try requireFocusedTabTree(try base.splittingPane("pane_1", placement: .down).state)
        expect(downTree.direction == .horizontal, "split down must create a horizontal split branch")
        expect(downTree.paneIDs == ["pane_1", "pane_2"], "split down must place the new pane after the focused pane")

        let upTree = try requireFocusedTabTree(try base.splittingPane("pane_1", placement: .up).state)
        expect(upTree.direction == .horizontal, "split up must create a horizontal split branch")
        expect(upTree.paneIDs == ["pane_2", "pane_1"], "split up must place the new pane before the focused pane")
    }

    private static func verifiesSplitRatiosClampWhenResized() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let split = try state.splittingPane("pane_1", direction: .vertical).state
        let splitID = try requireFocusedTabTree(split).nodeID

        let tooSmall = try split.resizingSplit(splitID, ratio: 0.01).state
        let tooSmallRatio = try requireFocusedTabTree(tooSmall).ratio
        expect(
            tooSmallRatio == ShellPaneTreeNode.minimumSplitRatio,
            "resize must clamp tiny split ratios to the minimum usable ratio"
        )

        let tooLarge = try split.resizingSplit(splitID, ratio: 0.99).state
        let tooLargeRatio = try requireFocusedTabTree(tooLarge).ratio
        expect(
            tooLargeRatio == ShellPaneTreeNode.maximumSplitRatio,
            "resize must clamp large split ratios to the maximum usable ratio"
        )
    }

    private static func verifiesEqualizeRestoresEverySplitRatio() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", direction: .vertical).state
        let rootID = try requireFocusedTabTree(state).nodeID
        state = try state.resizingSplit(rootID, ratio: 0.72).state
        state = try state.equalizingSplits(in: state.focusedTabID).state
        let equalizedRatio = try requireFocusedTabTree(state).ratio

        expect(
            equalizedRatio == 0.5,
            "equalize must restore the tab's root split ratio"
        )
    }

    private static func verifiesZoomProjectionUsesLeafWithoutMutatingSplitTree() throws {
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let split = try base.splittingPane("pane_1", placement: .right).state
        let tree = try requireFocusedTabTree(split)
        let zoomedLeaf = try require(tree.leafNode(containingPaneID: "pane_2"), "zoom leaf missing")

        expect(zoomedLeaf.kind == .pane, "zoom projection must display a single pane leaf")
        expect(zoomedLeaf.paneID == "pane_2", "zoom projection must display the requested pane")
        let treeAfterProjection = try requireFocusedTabTree(split)
        expect(
            treeAfterProjection == tree,
            "zoom projection must not mutate the canonical split tree"
        )
    }

    private static func verifiesInTabPaneMovementPreservesPaneIdentityAndRepairsTree() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state
        let movedPaneBefore = try require(state.pane(paneID: "pane_2"), "moved pane missing")

        let result = try state.movingPaneWithinTab("pane_2", placement: .left)
        let movedTree = try requireFocusedTabTree(result.state)

        expect(
            movedTree.paneIDs == ["pane_2", "pane_1"],
            "in-tab movement must repair the tree by placing the moved pane before the target"
        )
        expect(
            result.state.pane(paneID: "pane_2") == movedPaneBefore,
            "in-tab movement must preserve the moved PaneSlot and mounted content identity"
        )
        expect(
            result.state.focusedPaneID == "pane_2",
            "in-tab movement must keep focus on the moved pane"
        )
        expect(
            result.state.pane(paneID: "pane_2")?.tabID == "tab_main",
            "in-tab movement must keep the pane in the same tab"
        )
    }

    private static func verifiesInvalidInTabPaneMovementLeavesStateUnchanged() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state
        let original = state

        do {
            _ = try state.movingPaneWithinTab("pane_1", placement: .left)
            expect(false, "moving a pane without an adjacent destination must be rejected")
        } catch ShellStateMutationError.invalidMoveTarget {
            expect(state == original, "failed movement must leave the original state unchanged")
        }

        do {
            _ = try ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
                .movingPaneWithinTab("pane_1", placement: .right)
            expect(false, "moving a single-pane tab must be rejected")
        } catch ShellStateMutationError.invalidMoveTarget {
            // Expected.
        }
    }

    private static func verifiesSameDirectionAttachKeepsBinarySplitTree() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let split = try state.splittingPane("pane_1", direction: .vertical).state
        let attached = try requireFocusedTabTree(split).attachingPane(
            "pane_3",
            direction: .vertical,
            splitNodeID: "node_nested_split",
            newLeafNodeID: "node_pane_3"
        )

        expect(
            attached.children?.count == 2,
            "same-direction pane attachment must keep split branches binary"
        )
        guard let nestedSplit = attached.children?.last else {
            throw TestFailure("nested split missing")
        }
        expect(nestedSplit.kind == .split, "same-direction attachment must nest the final child")
        expect(nestedSplit.direction == .vertical, "nested split must keep the requested direction")
        expect(nestedSplit.children?.count == 2, "nested split must own exactly two children")
        expect(
            attached.paneIDs == ["pane_1", "pane_2", "pane_3"],
            "same-direction attachment must preserve pane ordering"
        )
    }

    private static func verifiesSidebarSplitTopologyProjection() throws {
        let threeColumns = split(
            .vertical,
            leaf("pane_1"),
            split(.vertical, leaf("pane_2"), leaf("pane_3"))
        )
        let threeColumnSummary = summary(for: threeColumns, focusedPaneID: "pane_2")
        expect(
            threeColumnSummary.topology.kind == .columns(count: 3),
            "same-axis vertical chains must classify as three columns"
        )
        expect(
            threeColumnSummary.paneIDs == ["pane_1", "pane_2", "pane_3"],
            "three-column topology must preserve visible pane order"
        )
        expect(
            threeColumnSummary.focusedPaneID == "pane_2",
            "topology summary must preserve focused pane when it is visible"
        )

        let threeRows = split(
            .horizontal,
            leaf("pane_1"),
            split(.horizontal, leaf("pane_2"), leaf("pane_3"))
        )
        expect(
            summary(for: threeRows).topology.kind == .rows(count: 3),
            "same-axis horizontal chains must classify as three rows"
        )

        let mainLeft = split(
            .vertical,
            leaf("pane_1"),
            split(.horizontal, leaf("pane_2"), leaf("pane_3"))
        )
        expect(
            summary(for: mainLeft).topology.kind == .mainLeftWithRightStack,
            "left main with right stack must classify as a main-stack topology"
        )

        let mainRight = split(
            .vertical,
            split(.horizontal, leaf("pane_1"), leaf("pane_2")),
            leaf("pane_3")
        )
        expect(
            summary(for: mainRight).topology.kind == .mainRightWithLeftStack,
            "right main with left stack must classify as a main-stack topology"
        )

        let mainTop = split(
            .horizontal,
            leaf("pane_1"),
            split(.vertical, leaf("pane_2"), leaf("pane_3"))
        )
        expect(
            summary(for: mainTop).topology.kind == .mainTopWithBottomSplit,
            "top main with bottom split must classify as a main-stack topology"
        )

        let mainBottom = split(
            .horizontal,
            split(.vertical, leaf("pane_1"), leaf("pane_2")),
            leaf("pane_3")
        )
        expect(
            summary(for: mainBottom).topology.kind == .mainBottomWithTopSplit,
            "bottom main with top split must classify as a main-stack topology"
        )

        let fourColumns = split(
            .vertical,
            leaf("pane_1"),
            split(
                .vertical,
                leaf("pane_2"),
                split(.vertical, leaf("pane_3"), leaf("pane_4"))
            )
        )
        expect(
            summary(for: fourColumns).topology.kind == .columns(count: 4),
            "same-axis four-pane chains must stay recognizable when legible"
        )

        let grid = split(
            .vertical,
            split(.horizontal, leaf("pane_1"), leaf("pane_2")),
            split(.horizontal, leaf("pane_3"), leaf("pane_4"))
        )
        expect(
            summary(for: grid).topology.kind == .grid2x2(rootDirection: .vertical),
            "balanced opposite-axis four-pane layouts must classify as a 2 by 2 grid"
        )

        let fiveColumns = split(
            .vertical,
            leaf("pane_1"),
            split(
                .vertical,
                leaf("pane_2"),
                split(
                    .vertical,
                    leaf("pane_3"),
                    split(.vertical, leaf("pane_4"), leaf("pane_5"))
                )
            )
        )
        if case .complex(let count) = summary(for: fiveColumns).topology.kind {
            expect(count == 5, "high-count same-axis layouts must fall back to complex count")
        } else {
            expect(false, "high-count same-axis layouts must classify as complex")
        }
    }

    private static func verifiesSpatialFocusFollowsSplitTree() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state
        let pane2 = state.focusedPaneID ?? "pane_2"
        let pane1Focused = try state.focusingPane("pane_1").state

        let rightResult = try pane1Focused.focusingAdjacentPane(.right)
        expect(rightResult.paneID == pane2, "focus right must move to the right sibling pane")

        let leftResult = try rightResult.state.focusingAdjacentPane(.left)
        expect(leftResult.paneID == "pane_1", "focus left must return to the left sibling pane")

        do {
            _ = try pane1Focused.focusingAdjacentPane(.left)
            expect(false, "focus left without a neighbor must throw")
        } catch ShellStateMutationError.spatialFocusTargetNotFound {
            // Expected.
        }
    }

    private static func verifiesSpatialFocusPreservesPerpendicularPosition() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state
        state = try state.splittingPane("pane_1", placement: .down).state
        state = try state.splittingPane("pane_2", placement: .down).state

        let lowerLeftFocused = try state.focusingPane("pane_3").state
        let rightResult = try lowerLeftFocused.focusingAdjacentPane(.right)
        expect(
            rightResult.paneID == "pane_4",
            "focus right from the lower-left pane must land on the lower-right pane"
        )

        let leftResult = try rightResult.state.focusingAdjacentPane(.left)
        expect(
            leftResult.paneID == "pane_3",
            "focus left from the lower-right pane must return to the lower-left pane"
        )

        var rowState = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        rowState = try rowState.splittingPane("pane_1", placement: .down).state
        rowState = try rowState.splittingPane("pane_1", placement: .right).state
        rowState = try rowState.splittingPane("pane_2", placement: .right).state

        let upperRightFocused = try rowState.focusingPane("pane_3").state
        let downResult = try upperRightFocused.focusingAdjacentPane(.down)
        expect(
            downResult.paneID == "pane_4",
            "focus down from the upper-right pane must land on the lower-right pane"
        )

        let upResult = try downResult.state.focusingAdjacentPane(.up)
        expect(
            upResult.paneID == "pane_3",
            "focus up from the lower-right pane must return to the upper-right pane"
        )
    }

    private static func verifiesPaneScopedCloseKeepsInactivePaneTargeting() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state
        state = try state.focusingPane("pane_1").state

        let result = try state.closingPane("pane_2")
        let tree = try requireFocusedTabTree(result.state)

        expect(result.state.pane(paneID: "pane_2") == nil, "targeted close must remove the requested pane")
        expect(result.state.pane(paneID: "pane_1") != nil, "targeted close must preserve the selected sibling")
        expect(result.state.focusedPaneID == "pane_1", "closing an inactive pane must not move focus")
        expect(tree.paneIDs == ["pane_1"], "split tree must repair after closing the inactive pane")
    }

    private static func verifiesPaneScopedCloseRemovesSelectedPane() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state

        let result = try state.closingPane("pane_2")
        let tree = try requireFocusedTabTree(result.state)

        expect(result.state.pane(paneID: "pane_2") == nil, "selected pane close must remove the selected pane")
        expect(result.state.focusedPaneID == "pane_1", "selected pane close must focus the remaining sibling")
        expect(tree.paneIDs == ["pane_1"], "selected pane close must repair the split tree")
    }

    private static func verifiesPaneScopedCloseClosesSinglePaneTab() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(
            in: state.focusedSpaceID,
            title: "Second",
            workingDirectory: "/tmp"
        ).state

        let result = try state.closingPane("pane_2")

        expect(result.state.pane(paneID: "pane_2") == nil, "single-pane tab close must remove that pane")
        expect(result.state.tab(tabID: "tab_2") == nil, "single-pane tab close must reuse tab close semantics")
        expect(result.state.pane(paneID: "pane_1") != nil, "single-pane tab close must preserve remaining tab panes")
        expect(result.state.focusedPaneID == "pane_1", "single-pane tab close must focus a remaining pane")
    }

    private static func verifiesPaneScopedCloseLeavesFinalSpaceEmpty() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        let result = try state.closingPane("pane_1")

        expect(result.state.spaces.count == 1, "closing the final pane must keep the space")
        expect(result.state.spaces.first?.spaceID == "space_main", "closing the final pane must keep space identity")
        expect(result.state.spaces.first?.tabs.isEmpty == true, "closing the final pane must leave the space empty")
        expect(result.state.panes.isEmpty, "closing the final pane must remove the pane")
        expect(result.state.focusedSpaceID == "space_main", "closing the final pane must keep the empty space focused")
        expect(result.state.focusedTabID == nil, "closing the final pane must clear tab focus")
        expect(result.state.focusedPaneID == nil, "closing the final pane must clear pane focus")
    }

    private static func verifiesClosingBackgroundSpaceTabPreservesCurrentFocus() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp/main")
        state = state.creatingTerminalSpace(
            title: "Background",
            workingDirectory: "/tmp/background"
        ).state
        state = try state.openingTerminalTab(
            in: "space_main",
            title: "Active second tab",
            workingDirectory: "/tmp/active"
        ).state

        expect(state.focusedSpaceID == "space_main", "test setup must focus the main space")
        expect(state.focusedTabID == "tab_3", "test setup must focus the active tab")
        expect(state.focusedPaneID == "pane_3", "test setup must focus the active pane")

        let result = try state.closingTab("tab_2")

        expect(
            result.state.tab(tabID: "tab_2") == nil,
            "background tab close must remove the target tab"
        )
        expect(
            result.state.pane(paneID: "pane_2") == nil,
            "background tab close must remove its pane"
        )
        expect(
            result.state.focusedSpaceID == "space_main",
            "background tab close must preserve focused space"
        )
        expect(
            result.state.focusedTabID == "tab_3",
            "background tab close must preserve focused tab"
        )
        expect(
            result.state.focusedPaneID == "pane_3",
            "background tab close must preserve focused pane"
        )
    }

    private static func verifiesDeletingBackgroundSpacePreservesCurrentFocus() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp/main")
        state = state.creatingTerminalSpace(
            title: "Background",
            workingDirectory: "/tmp/background"
        ).state
        state = try state.openingTerminalTab(
            in: "space_main",
            title: "Active second tab",
            workingDirectory: "/tmp/active"
        ).state

        expect(state.focusedSpaceID == "space_main", "test setup must focus the main space")
        expect(state.focusedTabID == "tab_3", "test setup must focus the active tab")
        expect(state.focusedPaneID == "pane_3", "test setup must focus the active pane")

        let result = try state.deletingSpace("space_2")

        expect(
            result.state.space(spaceID: "space_2") == nil,
            "background space delete must remove target space"
        )
        expect(
            result.state.pane(paneID: "pane_2") == nil,
            "background space delete must remove target panes"
        )
        expect(
            result.state.focusedSpaceID == "space_main",
            "background space delete must preserve focused space"
        )
        expect(
            result.state.focusedTabID == "tab_3",
            "background space delete must preserve focused tab"
        )
        expect(
            result.state.focusedPaneID == "pane_3",
            "background space delete must preserve focused pane"
        )
    }

    private static func verifiesPaneAllocationSkipsReservedRuntimeIDs() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        let openedTab = try state.openingTerminalTab(
            in: nil,
            title: nil,
            workingDirectory: nil,
            reservedPaneIDs: ["pane_2"]
        )
        expect(
            openedTab.paneID == "pane_3",
            "opening a tab must not reuse a pane ID reserved by a live runtime"
        )

        let splitPane = try state.splittingPane(
            "pane_1",
            direction: .vertical,
            reservedPaneIDs: ["pane_2"]
        )
        expect(
            splitPane.paneID == "pane_3",
            "splitting a pane must not reuse a pane ID reserved by a live runtime"
        )

        let newSpace = state.creatingTerminalSpace(
            title: nil,
            workingDirectory: nil,
            reservedPaneIDs: ["pane_2"]
        )
        expect(
            newSpace.paneID == "pane_3",
            "creating a space must not reuse a pane ID reserved by a live runtime"
        )
    }

    private static func verifiesSplitDecodeRequiresPersistedRatio() throws {
        let missingRatioJSON = """
        {
          "contract_version": "0.1",
          "window_id": "window_test",
          "focused_space_id": "space_main",
          "focused_tab_id": "tab_main",
          "focused_pane_id": "pane_1",
          "spaces": [
            {
              "space_id": "space_main",
              "title": "Terminal",
              "attention": "active",
              "tabs": [
                {
                  "tab_id": "tab_main",
                  "kind": "terminal",
                  "title": "Shell",
                  "pane_tree": {
                    "node_id": "node_split",
                    "kind": "split",
                    "direction": "vertical",
                    "children": [
                      {"node_id": "node_pane_1", "kind": "pane", "pane_id": "pane_1"},
                      {"node_id": "node_pane_2", "kind": "pane", "pane_id": "pane_2"}
                    ]
                  }
                }
              ]
            }
          ],
          "panes": [
            {"pane_id": "pane_1", "tab_id": "tab_main", "space_id": "space_main", "launch_target": "shell", "attention": "active"},
            {"pane_id": "pane_2", "tab_id": "tab_main", "space_id": "space_main", "launch_target": "shell", "attention": "idle"}
          ]
        }
        """
        do {
            _ = try JSONDecoder().decode(ShellStateSnapshot.self, from: Data(missingRatioJSON.utf8))
            expect(false, "split trees without persisted ratio must fail to decode")
        } catch DecodingError.keyNotFound(_, _) {
            // Expected.
        }
    }

    private static func requireFocusedTabTree(_ state: ShellStateSnapshot) throws -> ShellPaneTreeNode {
        guard let tabID = state.focusedTabID,
              let tab = state.tab(tabID: tabID)
        else {
            throw TestFailure("focused tab missing")
        }
        return tab.paneTree
    }

    private static func require<T>(_ value: T?, _ message: String) throws -> T {
        guard let value else {
            throw TestFailure(message)
        }
        return value
    }

    private static func summary(
        for paneTree: ShellPaneTreeNode,
        focusedPaneID: String? = nil
    ) -> ShellTabPaneSummary {
        ShellTabPaneSummary(
            paneTree: paneTree,
            visiblePaneIDs: paneTree.paneIDs,
            focusedPaneID: focusedPaneID
        )
    }

    private static func leaf(_ paneID: String) -> ShellPaneTreeNode {
        ShellPaneTreeNode(
            nodeID: "node_\(paneID)",
            kind: .pane,
            direction: nil,
            paneID: paneID,
            children: nil
        )
    }

    private static func split(
        _ direction: ShellSplitDirection,
        _ children: ShellPaneTreeNode...
    ) -> ShellPaneTreeNode {
        let paneIDSlug = children.flatMap(\.paneIDs).joined(separator: "_")
        return ShellPaneTreeNode(
            nodeID: "node_split_\(direction.rawValue)_\(paneIDSlug)",
            kind: .split,
            direction: direction,
            paneID: nil,
            children: children
        )
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

private enum ShellSplitModelFixtureExporter {
    static func exportIfRequested() throws {
        guard let rootPath = ProcessInfo.processInfo.environment["ALAN_SHELL_SPLIT_FIXTURE_DIR"],
              !rootPath.isEmpty
        else {
            return
        }

        try export(to: URL(fileURLWithPath: rootPath))
        print("Shell split model fixtures exported to \(rootPath).")
    }

    private static func export(to rootURL: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        for fixture in try splitTreeFixtures() + reducerFixtures() + controlCommandFixtures()
            + terminalProfileFixtures()
        {
            let fixtureURL = rootURL
                .appendingPathComponent(fixture.id)
                .appendingPathExtension("json")
            try FileManager.default.createDirectory(
                at: fixtureURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try encoder.encode(fixture).write(to: fixtureURL, options: .atomic)
        }
    }

    private static func splitTreeFixtures() throws -> [ShellCoreFixtureCase] {
        let singlePane = leaf("pane_1")
        let rightSplit = singlePane.splittingPane(
            "pane_1",
            placement: .right,
            splitNodeID: "node_split",
            newLeafNodeID: "node_pane_2",
            newPaneID: "pane_2"
        )
        let leftSplit = singlePane.splittingPane(
            "pane_1",
            placement: .left,
            splitNodeID: "node_split",
            newLeafNodeID: "node_pane_2",
            newPaneID: "pane_2"
        )

        let resizeInput = split(
            id: "node_split",
            .vertical,
            leaf("pane_1"),
            leaf("pane_2")
        )
        let minimumResize = resizeInput.resizingSplit("node_split", ratio: 0.01)
        let maximumResize = resizeInput.resizingSplit("node_split", ratio: 0.99)

        let unevenNestedTree = split(
            id: "node_root_split",
            .vertical,
            ratio: 0.72,
            leaf("pane_1"),
            split(
                id: "node_nested_split",
                .horizontal,
                ratio: 0.2,
                leaf("pane_2"),
                leaf("pane_3")
            )
        )
        let equalizedTree = unevenNestedTree.equalizedSplits()

        let rowFocusTree = split(
            id: "node_root_split",
            .vertical,
            split(id: "node_left_stack", .horizontal, leaf("pane_1"), leaf("pane_3")),
            split(id: "node_right_stack", .horizontal, leaf("pane_2"), leaf("pane_4"))
        )
        let columnFocusTree = split(
            id: "node_root_split",
            .horizontal,
            split(id: "node_top_row", .vertical, leaf("pane_1"), leaf("pane_3")),
            split(id: "node_bottom_row", .vertical, leaf("pane_2"), leaf("pane_4"))
        )

        return [
            ShellCoreFixtureCase(
                id: "split-tree/split-pane-placement-right",
                description: "Split right creates a vertical branch and places the new pane after the target.",
                input: SplitTreeInput(tree: singlePane),
                operation: SplitPaneOperation(
                    targetPaneID: "pane_1",
                    placement: .right,
                    splitNodeID: "node_split",
                    newLeafNodeID: "node_pane_2",
                    newPaneID: "pane_2"
                ),
                expected: TreeWithPaneIDsExpectation(tree: rightSplit, paneIDs: rightSplit.paneIDs)
            ),
            ShellCoreFixtureCase(
                id: "split-tree/split-pane-placement-left",
                description: "Split left creates a vertical branch and places the new pane before the target.",
                input: SplitTreeInput(tree: singlePane),
                operation: SplitPaneOperation(
                    targetPaneID: "pane_1",
                    placement: .left,
                    splitNodeID: "node_split",
                    newLeafNodeID: "node_pane_2",
                    newPaneID: "pane_2"
                ),
                expected: TreeWithPaneIDsExpectation(tree: leftSplit, paneIDs: leftSplit.paneIDs)
            ),
            ShellCoreFixtureCase(
                id: "split-tree/resize-clamps-to-minimum",
                description: "Resize clamps ratios below the minimum usable divider ratio.",
                input: SplitTreeInput(tree: resizeInput),
                operation: ResizeSplitOperation(splitNodeID: "node_split", ratio: 0.01),
                expected: ResizeSplitExpectation(
                    tree: minimumResize.node,
                    outcome: minimumResize.changed ? "changed" : "unchanged"
                )
            ),
            ShellCoreFixtureCase(
                id: "split-tree/resize-clamps-to-maximum",
                description: "Resize clamps ratios above the maximum usable divider ratio.",
                input: SplitTreeInput(tree: resizeInput),
                operation: ResizeSplitOperation(splitNodeID: "node_split", ratio: 0.99),
                expected: ResizeSplitExpectation(
                    tree: maximumResize.node,
                    outcome: maximumResize.changed ? "changed" : "unchanged"
                )
            ),
            ShellCoreFixtureCase(
                id: "split-tree/equalize-restores-nested-ratios",
                description: "Equalize restores every split ratio in a nested tree to 0.5.",
                input: SplitTreeInput(tree: unevenNestedTree),
                operation: EqualizeSplitsOperation(),
                expected: EqualizeSplitsExpectation(
                    tree: equalizedTree,
                    ratiosByNodeID: equalizedTree.splitRatiosByNodeID
                )
            ),
            ShellCoreFixtureCase(
                id: "split-tree/zoom-leaf-preserves-canonical-tree",
                description: "Zoom projection returns the requested leaf without mutating the canonical tree.",
                input: SplitTreeInput(tree: rightSplit),
                operation: ZoomLeafOperation(paneID: "pane_2"),
                expected: ZoomLeafExpectation(
                    tree: try require(rightSplit.leafNode(containingPaneID: "pane_2"), "zoom leaf missing"),
                    canonicalTree: rightSplit
                )
            ),
            ShellCoreFixtureCase(
                id: "split-tree/spatial-focus-right-preserves-row",
                description: "Focus right from the lower-left pane lands on the lower-right pane.",
                input: SplitTreeInput(tree: rowFocusTree),
                operation: AdjacentPaneOperation(targetPaneID: "pane_3", direction: .right),
                expected: AdjacentPaneExpectation(
                    paneID: rowFocusTree.adjacentPaneID(from: "pane_3", direction: .right)
                )
            ),
            ShellCoreFixtureCase(
                id: "split-tree/spatial-focus-down-preserves-column",
                description: "Focus down from the upper-right pane lands on the lower-right pane.",
                input: SplitTreeInput(tree: columnFocusTree),
                operation: AdjacentPaneOperation(targetPaneID: "pane_3", direction: .down),
                expected: AdjacentPaneExpectation(
                    paneID: columnFocusTree.adjacentPaneID(from: "pane_3", direction: .down)
                )
            ),
        ]
    }

    private static func reducerFixtures() throws -> [ShellCoreFixtureCase] {
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let openedTab = try base.openingTerminalTab(
            in: "space_main",
            title: "Worker",
            workingDirectory: "/tmp/project"
        ).state
        let createdSpace = base.creatingTerminalSpace(
            title: "Other",
            workingDirectory: "/tmp/other"
        ).state
        let splitRight = try base.splittingPane("pane_1", placement: .right).state
        let leftFocusedSplit = try splitRight.focusingPane("pane_1").state
        let focusAdjacentRight = try leftFocusedSplit.focusingAdjacentPane(.right).state
        let closedSelectedPane = try splitRight.closingPane("pane_2").state
        let renamedTab = try base.renamingTab("tab_main", title: "  Focused   Work  ").state
        let pinnedTab = try base.pinningTab("tab_main").state
        let unpinnedTab = try pinnedTab.unpinningTab("tab_main").state
        let closeTabInput = try base.openingTerminalTab(
            in: "space_main",
            title: "Second",
            workingDirectory: "/tmp/second"
        ).state
        let closedTab = try closeTabInput.closingTab("tab_2").state
        let attentionSet = try base.settingAttention(.awaitingUser, for: "pane_1").state
        let metadataUpdated = try base.settingAutomaticTabTitle(
            "tab_main",
            title: "cargo test"
        ).state
        let metadataFixture = PortableTerminalRuntimeMetadata(
            title: "cargo test",
            cwd: "/repo/app",
            activeTaskState: .foregroundCommand,
            activity: nil
        )
        let agentActivity = codexRunningActivity()
        let agentActivityMetadata = PortableTerminalRuntimeMetadata(
            title: nil,
            cwd: "/repo/app",
            activeTaskState: .inactive,
            activity: agentActivity
        )
        let duplicateInput = try base.openingTerminalTab(
            in: "space_main",
            title: "Second",
            workingDirectory: "/tmp/second"
        ).state
        let duplicatedTab = try duplicateInput.duplicatingTab("tab_2").state
        var moveTabToSpaceInput = duplicateInput
        moveTabToSpaceInput = moveTabToSpaceInput
            .creatingTerminalSpace(title: "Other", workingDirectory: "/tmp/other")
            .state
        let movedTabToSpace = try moveTabToSpaceInput.movingTabToSpace(
            tabID: "tab_2",
            targetSpaceID: "space_2"
        ).state
        let moveTabWithinInput = try duplicateInput.openingTerminalTab(
            in: "space_main",
            title: "Third",
            workingDirectory: "/tmp/third"
        ).state
        let movedTabWithinSection = try moveTabWithinInput.movingTab("tab_3", sectionOffset: -1).state

        var clearInput = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        clearInput = try clearInput.openingTerminalTab(
            in: "space_main",
            title: "Clearable",
            workingDirectory: "/tmp"
        ).state
        clearInput = try clearInput.openingTerminalTab(
            in: "space_main",
            title: "Protected",
            workingDirectory: "/tmp"
        ).state
        let protectedTabID = try require(clearInput.focusedTabID, "protected tab missing")
        clearInput = try clearInput.openingTerminalTab(
            in: "space_main",
            title: "Pinned",
            workingDirectory: "/tmp"
        ).state
        let pinnedTabID = try require(clearInput.focusedTabID, "pinned tab missing")
        clearInput = try clearInput.pinningTab(pinnedTabID).state
        clearInput = clearInput.creatingTerminalSpace(title: "Other", workingDirectory: "/tmp").state
        clearInput = try clearInput.focusingPane("pane_1").state
        let clearedInactiveTabs = try clearInput.clearingInactiveTemporaryTabs(
            in: "space_main",
            activeTaskByTabID: [protectedTabID: .foregroundCommand]
        ).state
        let activeTaskMetadata = PortableTerminalRuntimeMetadata(
            title: nil,
            cwd: nil,
            activeTaskState: .foregroundCommand,
            activity: nil
        )

        let movedPaneWithinTab = try splitRight.movingPaneWithinTab(
            "pane_2",
            placement: .left
        ).state
        let movedPaneToNewTab = try splitRight.movingPaneToNewTab(
            "pane_2",
            title: "Lifted"
        ).state
        let crossTabInput = try movedPaneToNewTab.openingTerminalTab(
            in: "space_main",
            title: "Target",
            workingDirectory: nil
        ).state
        let movedPaneToTab = try crossTabInput.movingPane(
            "pane_2",
            toTab: "tab_3",
            direction: .vertical
        ).state
        let zoomedPaneByTabID = ["tab_main": "pane_2"]
        let zoomedPaneState = try splitRight.focusingPane("pane_2").state
        let unzoomedPaneState = zoomedPaneState
        let closedZoomedPane = try zoomedPaneState.closingPane("pane_2").state
        let movePaneToNewTabLastPaneErrorCode: String
        do {
            _ = try base.movingPaneToNewTab("pane_1", title: nil)
            throw TestFailure("last-pane move to new tab unexpectedly succeeded")
        } catch let error as ShellStateMutationError {
            movePaneToNewTabLastPaneErrorCode = error.rawValue
        }
        let movePaneWithinInvalidTargetErrorCode: String
        do {
            _ = try base.movingPaneWithinTab("pane_1", placement: .left)
            throw TestFailure("single-pane in-tab move unexpectedly succeeded")
        } catch let error as ShellStateMutationError {
            movePaneWithinInvalidTargetErrorCode = error.rawValue
        }
        let splitMissingErrorCode: String
        do {
            _ = try base.splittingPane("missing", placement: .right)
            throw TestFailure("missing pane split unexpectedly succeeded")
        } catch let error as ShellStateMutationError {
            splitMissingErrorCode = error.rawValue
        }

        return [
            ShellCoreFixtureCase(
                id: "reducer/open-terminal-tab",
                kind: "reducer",
                description: "Opening a terminal tab appends a tab, pane slot, and terminal content in the focused Space.",
                input: PortableWorkspaceState(base),
                operation: OpenTerminalTabOperation(
                    spaceID: "space_main",
                    title: "Worker",
                    workingDirectory: "/tmp/project",
                    terminalProfileID: nil
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(openedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/create-terminal-space",
                kind: "reducer",
                description: "Creating a terminal Space appends a Space with one selected terminal tab.",
                input: PortableWorkspaceState(base),
                operation: CreateTerminalSpaceOperation(
                    title: "Other",
                    tabTitle: nil,
                    workingDirectory: "/tmp/other",
                    terminalProfileID: nil
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(createdSpace))
            ),
            ShellCoreFixtureCase(
                id: "reducer/split-pane-right",
                kind: "reducer",
                description: "Splitting a terminal pane to the right creates a second pane slot in the same tab.",
                input: PortableWorkspaceState(base),
                operation: ReducerSplitPaneOperation(
                    paneSlotID: "pane_1",
                    placement: .right,
                    title: nil,
                    workingDirectory: nil,
                    terminalProfileID: nil
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(splitRight))
            ),
            ShellCoreFixtureCase(
                id: "reducer/focus-adjacent-right",
                kind: "reducer",
                description: "Focusing right from the left split pane moves focus to the right sibling.",
                input: PortableWorkspaceState(leftFocusedSplit),
                operation: FocusAdjacentPaneOperation(direction: .right),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(focusAdjacentRight))
            ),
            ShellCoreFixtureCase(
                id: "reducer/close-selected-pane",
                kind: "reducer",
                description: "Closing the selected split pane removes its content and returns focus to the remaining pane.",
                input: PortableWorkspaceState(splitRight),
                operation: ClosePaneOperation(paneSlotID: "pane_2"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(closedSelectedPane))
            ),
            ShellCoreFixtureCase(
                id: "reducer/close-tab",
                kind: "reducer",
                description: "Closing the selected tab removes its pane and repairs focus to a remaining tab.",
                input: PortableWorkspaceState(closeTabInput),
                operation: CloseTabOperation(tabID: "tab_2"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(closedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/rename-tab",
                kind: "reducer",
                description: "Renaming a tab collapses whitespace and locks the user title.",
                input: PortableWorkspaceState(base),
                operation: RenameTabOperation(
                    tabID: "tab_main",
                    title: "  Focused   Work  "
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(renamedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/pin-tab",
                kind: "reducer",
                description: "Pinning a tab marks it pinned without changing focus.",
                input: PortableWorkspaceState(base),
                operation: PinTabOperation(tabID: "tab_main"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(pinnedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/unpin-tab",
                kind: "reducer",
                description: "Unpinning a pinned tab returns it to the unpinned section.",
                input: PortableWorkspaceState(pinnedTab),
                operation: UnpinTabOperation(tabID: "tab_main"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(unpinnedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/set-attention",
                kind: "reducer",
                description: "Setting pane attention updates the pane slot and containing Space attention.",
                input: PortableWorkspaceState(base),
                operation: SetAttentionOperation(
                    paneSlotID: "pane_1",
                    attention: .awaitingUser
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(attentionSet))
            ),
            ShellCoreFixtureCase(
                id: "reducer/update-terminal-metadata",
                kind: "reducer",
                description: "Terminal metadata updates portable runtime fields and automatic tab title.",
                input: PortableWorkspaceState(base),
                operation: UpdateTerminalMetadataOperation(
                    paneSlotID: "pane_1",
                    title: "cargo test",
                    cwd: "/repo/app",
                    activeTaskState: .foregroundCommand,
                    activity: nil
                ),
                expected: ReducerSuccessExpectation(
                    state: PortableWorkspaceState(
                        metadataUpdated,
                        terminalMetadataByContentID: ["content_pane_1": metadataFixture]
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/apply-agent-activity",
                kind: "reducer",
                description: "Agent activity updates terminal runtime metadata and working directory.",
                input: PortableWorkspaceState(base),
                operation: ApplyAgentActivityOperation(
                    paneSlotID: "pane_1",
                    activity: agentActivity,
                    workingDirectory: "/repo/app"
                ),
                expected: ReducerSuccessExpectation(
                    state: PortableWorkspaceState(
                        base,
                        terminalMetadataByContentID: ["content_pane_1": agentActivityMetadata]
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/duplicate-tab",
                kind: "reducer",
                description: "Duplicating a terminal tab inserts a new terminal tab next to the source.",
                input: PortableWorkspaceState(duplicateInput),
                operation: DuplicateTabOperation(tabID: "tab_2"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(duplicatedTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-tab-to-space",
                kind: "reducer",
                description: "Moving a tab to another Space updates pane-slot Space ownership.",
                input: PortableWorkspaceState(moveTabToSpaceInput),
                operation: MoveTabToSpaceOperation(
                    tabID: "tab_2",
                    targetSpaceID: "space_2"
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(movedTabToSpace))
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-tab-within-section",
                kind: "reducer",
                description: "Moving a tab within its section reorders unpinned tabs.",
                input: PortableWorkspaceState(moveTabWithinInput),
                operation: MoveTabOperation(tabID: "tab_3", sectionOffset: -1),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(movedTabWithinSection))
            ),
            ShellCoreFixtureCase(
                id: "reducer/clear-inactive-temporary-tabs",
                kind: "reducer",
                description: "Clearing inactive temporary tabs preserves selected, pinned, protected, and other-Space tabs.",
                input: PortableWorkspaceState(clearInput),
                operation: ClearInactiveTemporaryTabsOperation(
                    spaceID: "space_main",
                    protectedTabIDs: [protectedTabID]
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(clearedInactiveTabs))
            ),
            ShellCoreFixtureCase(
                id: "reducer/clear-inactive-temporary-tabs-active-task-metadata",
                kind: "reducer",
                description: "Clearing inactive temporary tabs preserves tabs protected by runtime active-task metadata.",
                input: PortableWorkspaceState(
                    clearInput,
                    terminalMetadataByContentID: ["content_pane_3": activeTaskMetadata]
                ),
                operation: ClearInactiveTemporaryTabsOperation(
                    spaceID: "space_main",
                    protectedTabIDs: []
                ),
                expected: ReducerSuccessExpectation(
                    state: PortableWorkspaceState(
                        clearedInactiveTabs,
                        terminalMetadataByContentID: ["content_pane_3": activeTaskMetadata]
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-pane-within-tab",
                kind: "reducer",
                description: "Moving a pane within a tab preserves pane and content identity while repairing the split tree.",
                input: PortableWorkspaceState(splitRight),
                operation: MovePaneWithinTabOperation(
                    paneSlotID: "pane_2",
                    placement: .left
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(movedPaneWithinTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-pane-to-new-tab",
                kind: "reducer",
                description: "Moving a pane to a new tab lifts it out of the split tree.",
                input: PortableWorkspaceState(splitRight),
                operation: MovePaneToNewTabOperation(
                    paneSlotID: "pane_2",
                    title: "Lifted"
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(movedPaneToNewTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-pane-to-tab",
                kind: "reducer",
                description: "Moving a pane into another tab attaches it as a split and removes an empty source tab.",
                input: PortableWorkspaceState(crossTabInput),
                operation: MovePaneToTabOperation(
                    paneSlotID: "pane_2",
                    targetTabID: "tab_3",
                    direction: .vertical
                ),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(movedPaneToTab))
            ),
            ShellCoreFixtureCase(
                id: "reducer/zoom-pane",
                kind: "reducer",
                description: "Zooming a split pane stores tab-scoped zoom state and focuses the pane.",
                input: PortableWorkspaceState(splitRight),
                operation: ZoomPaneOperation(paneSlotID: "pane_2"),
                expected: ReducerSuccessExpectation(
                    state: PortableWorkspaceState(
                        zoomedPaneState,
                        zoomedPaneIDByTabID: zoomedPaneByTabID
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/unzoom-tab",
                kind: "reducer",
                description: "Unzooming a tab clears tab-scoped zoom state without changing focus.",
                input: PortableWorkspaceState(
                    zoomedPaneState,
                    zoomedPaneIDByTabID: zoomedPaneByTabID
                ),
                operation: UnzoomTabOperation(tabID: "tab_main"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(unzoomedPaneState))
            ),
            ShellCoreFixtureCase(
                id: "reducer/close-zoomed-pane-prunes-zoom",
                kind: "reducer",
                description: "Closing a zoomed pane removes invalid tab zoom state.",
                input: PortableWorkspaceState(
                    zoomedPaneState,
                    zoomedPaneIDByTabID: zoomedPaneByTabID
                ),
                operation: ClosePaneOperation(paneSlotID: "pane_2"),
                expected: ReducerSuccessExpectation(state: PortableWorkspaceState(closedZoomedPane))
            ),
            ShellCoreFixtureCase(
                id: "reducer/zoom-single-pane-error",
                kind: "reducer",
                description: "Zooming a single-pane tab is rejected.",
                input: PortableWorkspaceState(base),
                operation: ZoomPaneOperation(paneSlotID: "pane_1"),
                expected: ReducerErrorExpectation(
                    errorCode: "invalid_move_target",
                    state: PortableWorkspaceState(base)
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/unzoom-unzoomed-tab-error",
                kind: "reducer",
                description: "Unzooming a tab without zoom state is rejected.",
                input: PortableWorkspaceState(base),
                operation: UnzoomTabOperation(tabID: "tab_main"),
                expected: ReducerErrorExpectation(
                    errorCode: "invalid_move_target",
                    state: PortableWorkspaceState(base)
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-pane-to-new-tab-last-pane-error",
                kind: "reducer",
                description: "Moving the only pane in a tab to a new tab is rejected.",
                input: PortableWorkspaceState(base),
                operation: MovePaneToNewTabOperation(
                    paneSlotID: "pane_1",
                    title: nil
                ),
                expected: ReducerErrorExpectation(
                    errorCode: movePaneToNewTabLastPaneErrorCode,
                    state: PortableWorkspaceState(base)
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/move-pane-within-tab-invalid-target-error",
                kind: "reducer",
                description: "Moving a pane within a single-pane tab is rejected.",
                input: PortableWorkspaceState(base),
                operation: MovePaneWithinTabOperation(
                    paneSlotID: "pane_1",
                    placement: .left
                ),
                expected: ReducerErrorExpectation(
                    errorCode: movePaneWithinInvalidTargetErrorCode,
                    state: PortableWorkspaceState(base)
                )
            ),
            ShellCoreFixtureCase(
                id: "reducer/split-missing-pane-error",
                kind: "reducer",
                description: "Splitting a missing pane returns a stable error and leaves state unchanged.",
                input: PortableWorkspaceState(base),
                operation: ReducerSplitPaneOperation(
                    paneSlotID: "missing",
                    placement: .right,
                    title: nil,
                    workingDirectory: nil,
                    terminalProfileID: nil
                ),
                expected: ReducerErrorExpectation(
                    errorCode: splitMissingErrorCode,
                    state: PortableWorkspaceState(base)
                )
            ),
        ]
    }

    private static func controlCommandFixtures() throws -> [ShellCoreFixtureCase] {
        let base = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let split = try base.splittingPane("pane_1", placement: .right).state

        return [
            try controlCommandFixture(
                id: "control-command/state",
                description: "The state control command projects the portable workspace snapshot.",
                state: base,
                command: decodeControlCommand(
                    """
                    {
                      "request_id": "control-state",
                      "command": "state"
                    }
                    """
                )
            ),
            try controlCommandFixture(
                id: "control-command/tab-open",
                description: "Opening a tab through the control command applies local state changes.",
                state: base,
                command: decodeControlCommand(
                    """
                    {
                      "request_id": "control-tab-open",
                      "command": "tab.open",
                      "space_id": "space_main",
                      "title": "Worker",
                      "cwd": "/tmp/project",
                      "terminal_profile_id": "profile-main"
                    }
                    """
                )
            ),
            try controlCommandFixture(
                id: "control-command/pane-split",
                description: "Splitting a pane through the control command returns a state snapshot.",
                state: base,
                command: decodeControlCommand(
                    """
                    {
                      "request_id": "control-pane-split",
                      "command": "pane.split",
                      "pane_id": "pane_1",
                      "direction": "vertical",
                      "title": "Worker",
                      "cwd": "/tmp/project",
                      "terminal_profile_id": "profile-main"
                    }
                    """
                )
            ),
            try controlCommandFixture(
                id: "control-command/pane-split-missing-direction",
                description: "pane.split without direction returns the stable direction_required error.",
                state: base,
                command: decodeControlCommand(
                    """
                    {
                      "request_id": "control-pane-split-missing-direction",
                      "command": "pane.split",
                      "pane_id": "pane_1"
                    }
                    """
                )
            ),
            try controlCommandFixture(
                id: "control-command/pane-focus",
                description: "Focusing a pane through the control command updates focused ids.",
                state: split,
                command: decodeControlCommand(
                    """
                    {
                      "request_id": "control-pane-focus",
                      "command": "pane.focus",
                      "pane_id": "pane_1"
                    }
                    """
                )
            ),
        ]
    }

    private static func controlCommandFixture(
        id: String,
        description: String,
        state: ShellStateSnapshot,
        command: AlanShellControlCommand
    ) throws -> ShellCoreFixtureCase {
        guard let result = AlanShellLocalCommandExecutor.execute(command: command, state: state)
        else {
            throw ShellControlCommandFixtureExportError.unhandledCommand(id)
        }

        return ShellCoreFixtureCase(
            id: id,
            kind: "control_command",
            description: description,
            input: PortableWorkspaceState(state),
            operation: command,
            expected: ControlCommandExpectation(result)
        )
    }

    private static func decodeControlCommand(_ json: String) throws -> AlanShellControlCommand {
        try JSONDecoder().decode(AlanShellControlCommand.self, from: Data(json.utf8))
    }

    private static func terminalProfileFixtures() -> [ShellCoreFixtureCase] {
        let invalid = TerminalProfileDefinition(
            id: "bad",
            title: "",
            launch: .sudoUser(unixUser: ""),
            defaultWorkingDirectory: nil,
            presentation: nil
        )
        let validationDocument = TerminalProfileDocument(
            defaultProfileID: "missing-default",
            profiles: [invalid]
        )
        let draft = TerminalProfileFixtureEditorDraft(
            id: " alan ",
            title: " Alan ",
            launchKind: .sudoUser,
            unixUser: "alan",
            customCommand: "",
            defaultWorkingDirectory: " /Users/alan ",
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: "green"
            ),
            managedTerminalAccountID: " alan "
        )
        let editorResult = TerminalProfileEditor.makeDefinition(from: draft.editorDraft)

        return [
            ShellCoreFixtureCase(
                id: "terminal-profile/validation-errors",
                kind: "terminal_profile",
                description: "Terminal Profile validation reports stable document errors.",
                input: validationDocument,
                operation: TerminalProfileValidateOperation(),
                expected: TerminalProfileValidationExpectation(
                    TerminalProfileValidator.validate(validationDocument)
                )
            ),
            ShellCoreFixtureCase(
                id: "terminal-profile/editor-make-definition",
                kind: "terminal_profile",
                description: "Terminal Profile editor trims drafts and builds definitions.",
                input: EmptyFixtureInput(),
                operation: TerminalProfileMakeDefinitionOperation(draft: draft),
                expected: TerminalProfileEditorExpectation(editorResult)
            ),
        ]
    }

    private static func leaf(_ paneID: String) -> ShellPaneTreeNode {
        ShellPaneTreeNode(
            nodeID: "node_\(paneID)",
            kind: .pane,
            direction: nil,
            paneID: paneID,
            children: nil
        )
    }

    private static func split(
        id: String,
        _ direction: ShellSplitDirection,
        ratio: Double = 0.5,
        _ children: ShellPaneTreeNode...
    ) -> ShellPaneTreeNode {
        ShellPaneTreeNode(
            nodeID: id,
            kind: .split,
            direction: direction,
            ratio: ratio,
            paneID: nil,
            children: children
        )
    }

    private static func codexRunningActivity() -> TerminalActivitySnapshot {
        TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: .codex, label: "Codex"),
            status: .running,
            priority: .active,
            progress: nil,
            command: nil,
            agent: TerminalActivityAgentMetadata(
                kind: .codex,
                safeSessionLabel: nil,
                projectLabel: "alan",
                workingDirectory: "/repo/app"
            ),
            display: TerminalActivityDisplay(
                sourceLabel: "Codex",
                stateLabel: "Running",
                detailLabel: nil,
                paneHint: nil
            ),
            freshness: TerminalActivityFreshness(
                updatedAt: "2026-05-17T09:00:00Z",
                staleAt: "2026-05-17T09:01:30Z",
                expiresAt: nil
            )
        )
    }

    private static func require<T>(_ value: T?, _ message: String) throws -> T {
        guard let value else {
            throw TestFailure(message)
        }
        return value
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
        kind: String = "split_tree",
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

private struct SplitTreeInput: Encodable {
    let tree: ShellPaneTreeNode
}

private struct SplitPaneOperation: Encodable {
    let type = "split_pane"
    let targetPaneID: String
    let placement: ShellPaneSplitDirection
    let splitNodeID: String
    let newLeafNodeID: String
    let newPaneID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case targetPaneID = "target_pane_id"
        case placement
        case splitNodeID = "split_node_id"
        case newLeafNodeID = "new_leaf_node_id"
        case newPaneID = "new_pane_id"
    }
}

private struct ResizeSplitOperation: Encodable {
    let type = "resize_split"
    let splitNodeID: String
    let ratio: Double

    private enum CodingKeys: String, CodingKey {
        case type
        case splitNodeID = "split_node_id"
        case ratio
    }
}

private struct EqualizeSplitsOperation: Encodable {
    let type = "equalize_splits"
}

private struct ZoomLeafOperation: Encodable {
    let type = "zoom_leaf"
    let paneID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case paneID = "pane_id"
    }
}

private struct AdjacentPaneOperation: Encodable {
    let type = "adjacent_pane"
    let targetPaneID: String
    let direction: ShellSpatialFocusDirection

    private enum CodingKeys: String, CodingKey {
        case type
        case targetPaneID = "target_pane_id"
        case direction
    }
}

private struct TreeWithPaneIDsExpectation: Encodable {
    let tree: ShellPaneTreeNode
    let paneIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case tree
        case paneIDs = "pane_ids"
    }
}

private struct ResizeSplitExpectation: Encodable {
    let tree: ShellPaneTreeNode
    let outcome: String
}

private struct EqualizeSplitsExpectation: Encodable {
    let tree: ShellPaneTreeNode
    let ratiosByNodeID: [String: Double]

    private enum CodingKeys: String, CodingKey {
        case tree
        case ratiosByNodeID = "ratios_by_node_id"
    }
}

private struct ZoomLeafExpectation: Encodable {
    let tree: ShellPaneTreeNode
    let canonicalTree: ShellPaneTreeNode

    private enum CodingKeys: String, CodingKey {
        case tree
        case canonicalTree = "canonical_tree"
    }
}

private struct AdjacentPaneExpectation: Encodable {
    let paneID: String?

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
    }
}

private struct OpenTerminalTabOperation: Encodable {
    let type = "open_terminal_tab"
    let spaceID: String?
    let title: String?
    let workingDirectory: String?
    let terminalProfileID: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case spaceID = "space_id"
        case title
        case workingDirectory = "working_directory"
        case terminalProfileID = "terminal_profile_id"
    }
}

private struct CreateTerminalSpaceOperation: Encodable {
    let type = "create_terminal_space"
    let title: String?
    let tabTitle: String?
    let workingDirectory: String?
    let terminalProfileID: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case title
        case tabTitle = "tab_title"
        case workingDirectory = "working_directory"
        case terminalProfileID = "terminal_profile_id"
    }
}

private struct ReducerSplitPaneOperation: Encodable {
    let type = "split_pane"
    let paneSlotID: String
    let placement: ShellPaneSplitDirection
    let title: String?
    let workingDirectory: String?
    let terminalProfileID: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case placement
        case title
        case workingDirectory = "working_directory"
        case terminalProfileID = "terminal_profile_id"
    }
}

private struct FocusAdjacentPaneOperation: Encodable {
    let type = "focus_adjacent_pane"
    let direction: ShellSpatialFocusDirection
}

private struct ClosePaneOperation: Encodable {
    let type = "close_pane"
    let paneSlotID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
    }
}

private struct CloseTabOperation: Encodable {
    let type = "close_tab"
    let tabID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
    }
}

private struct RenameTabOperation: Encodable {
    let type = "rename_tab"
    let tabID: String
    let title: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case title
    }
}

private struct PinTabOperation: Encodable {
    let type = "pin_tab"
    let tabID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
    }
}

private struct UnpinTabOperation: Encodable {
    let type = "unpin_tab"
    let tabID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
    }
}

private struct DuplicateTabOperation: Encodable {
    let type = "duplicate_tab"
    let tabID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
    }
}

private struct MoveTabOperation: Encodable {
    let type = "move_tab"
    let tabID: String
    let sectionOffset: Int

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case sectionOffset = "section_offset"
    }
}

private struct MoveTabToSpaceOperation: Encodable {
    let type = "move_tab_to_space"
    let tabID: String
    let targetSpaceID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case targetSpaceID = "target_space_id"
    }
}

private struct ClearInactiveTemporaryTabsOperation: Encodable {
    let type = "clear_inactive_temporary_tabs"
    let spaceID: String
    let protectedTabIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case type
        case spaceID = "space_id"
        case protectedTabIDs = "protected_tab_ids"
    }
}

private struct MovePaneToNewTabOperation: Encodable {
    let type = "move_pane_to_new_tab"
    let paneSlotID: String
    let title: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case title
    }
}

private struct MovePaneToTabOperation: Encodable {
    let type = "move_pane_to_tab"
    let paneSlotID: String
    let targetTabID: String
    let direction: ShellSplitDirection

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case targetTabID = "target_tab_id"
        case direction
    }
}

private struct MovePaneWithinTabOperation: Encodable {
    let type = "move_pane_within_tab"
    let paneSlotID: String
    let placement: ShellPaneSplitDirection

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case placement
    }
}

private struct ZoomPaneOperation: Encodable {
    let type = "zoom_pane"
    let paneSlotID: String

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
    }
}

private struct UnzoomTabOperation: Encodable {
    let type = "unzoom_tab"
    let tabID: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
    }
}

private struct SetAttentionOperation: Encodable {
    let type = "set_attention"
    let paneSlotID: String
    let attention: ShellAttentionState

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case attention
    }
}

private struct UpdateTerminalMetadataOperation: Encodable {
    let type = "update_terminal_metadata"
    let paneSlotID: String
    let title: String?
    let cwd: String?
    let activeTaskState: ShellTabActiveTaskState?
    let activity: TerminalActivitySnapshot?

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case title
        case cwd
        case activeTaskState = "active_task_state"
        case activity
    }
}

private struct ApplyAgentActivityOperation: Encodable {
    let type = "apply_agent_activity"
    let paneSlotID: String
    let activity: TerminalActivitySnapshot
    let workingDirectory: String?

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case activity
        case workingDirectory = "working_directory"
    }
}

private struct ReducerSuccessExpectation: Encodable {
    let status = "ok"
    let state: PortableWorkspaceState
}

private struct ReducerErrorExpectation: Encodable {
    let status = "error"
    let errorCode: String
    let state: PortableWorkspaceState

    private enum CodingKeys: String, CodingKey {
        case status
        case errorCode = "error_code"
        case state
    }
}

private enum ShellControlCommandFixtureExportError: Error {
    case unhandledCommand(String)
}

private struct ControlCommandExpectation: Encodable {
    let status = "handled"
    let response: ControlCommandResponseExpectation
    let updatedState: PortableWorkspaceState?
    let sideEffect: ControlCommandSideEffectExpectation?

    init(_ result: AlanShellLocalCommandResult) {
        response = ControlCommandResponseExpectation(result.response)
        updatedState = result.updatedState.map { PortableWorkspaceState($0) }
        sideEffect = result.sideEffect.map(ControlCommandSideEffectExpectation.init)
    }

    private enum CodingKeys: String, CodingKey {
        case status
        case response
        case updatedState = "updated_state"
        case sideEffect = "side_effect"
    }
}

private struct ControlCommandResponseExpectation: Encodable {
    let requestID: String
    let contractVersion: String
    let applied: Bool?
    let stateSnapshot: PortableWorkspaceState?
    let focusedPaneSlotID: String?
    let spaceID: String?
    let tabID: String?
    let paneSlotID: String?
    let contentID: String?
    let contentKind: ShellContentKind?
    let errorCode: String?
    let errorMessage: String?

    init(_ response: AlanShellControlResponse) {
        requestID = response.requestID
        contractVersion = response.contractVersion
        applied = response.applied
        stateSnapshot = response.state.map { PortableWorkspaceState($0) }
        focusedPaneSlotID = response.focusedPaneSlotID ?? response.focusedPaneID
        spaceID = response.spaceID
        tabID = response.tabID
        paneSlotID = response.paneSlotID ?? response.paneID
        contentID = response.contentID
        contentKind = response.contentKind
        errorCode = response.errorCode
        errorMessage = response.errorMessage
    }

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case contractVersion = "contract_version"
        case applied
        case stateSnapshot = "state_snapshot"
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case contentKind = "content_kind"
        case errorCode = "error_code"
        case errorMessage = "error_message"
    }
}

private struct ControlCommandSideEffectExpectation: Encodable {
    let type: String
    let paneSlotID: String?
    let text: String?

    init(_ sideEffect: AlanShellLocalCommandSideEffect) {
        switch sideEffect {
        case let .sendText(paneID, text):
            type = "send_terminal_text"
            paneSlotID = paneID
            self.text = text
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case text
    }
}

private struct TerminalProfileValidateOperation: Encodable {
    let type = "validate"
}

private struct TerminalProfileMakeDefinitionOperation: Encodable {
    let type = "make_definition"
    let draft: TerminalProfileFixtureEditorDraft
}

private struct TerminalProfileFixtureEditorDraft: Encodable {
    let id: String
    let title: String
    let launchKind: TerminalProfileLaunchKind
    let unixUser: String
    let customCommand: String
    let defaultWorkingDirectory: String?
    let presentation: TerminalProfilePresentation?
    let managedTerminalAccountID: String?

    var editorDraft: TerminalProfileEditorDraft {
        TerminalProfileEditorDraft(
            id: id,
            title: title,
            launchKind: launchKind,
            unixUser: unixUser,
            customCommand: customCommand,
            defaultWorkingDirectory: defaultWorkingDirectory,
            presentation: presentation,
            managedTerminalAccountID: managedTerminalAccountID
        )
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case launchKind = "launch_kind"
        case unixUser = "unix_user"
        case customCommand = "custom_command"
        case defaultWorkingDirectory = "default_working_directory"
        case presentation
        case managedTerminalAccountID = "managed_terminal_account_id"
    }
}

private struct TerminalProfileValidationExpectation: Encodable {
    let isValid: Bool
    let errors: [TerminalProfileValidationErrorFixture]

    init(_ result: TerminalProfileValidationResult) {
        isValid = result.isValid
        errors = result.errors.map(TerminalProfileValidationErrorFixture.init)
    }

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case errors
    }
}

private struct TerminalProfileEditorExpectation: Encodable {
    let isValid: Bool
    let definition: TerminalProfileDefinition?
    let errors: [TerminalProfileValidationErrorFixture]

    init(_ result: TerminalProfileEditorResult) {
        isValid = result.isValid
        definition = result.definition
        errors = result.errors.map(TerminalProfileValidationErrorFixture.init)
    }

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case definition
        case errors
    }
}

private struct TerminalProfileValidationErrorFixture: Encodable {
    let type: String
    let id: String?
    let profileID: String?
    let path: String?

    init(_ error: TerminalProfileValidationError) {
        switch error {
        case .missingID:
            type = "missing_id"
            id = nil
            profileID = nil
            path = nil
        case let .duplicateID(value):
            type = "duplicate_id"
            id = value
            profileID = nil
            path = nil
        case let .missingTitle(value):
            type = "missing_title"
            id = value
            profileID = nil
            path = nil
        case let .missingUnixUser(value):
            type = "missing_unix_user"
            id = value
            profileID = nil
            path = nil
        case let .missingCustomCommand(value):
            type = "missing_custom_command"
            id = value
            profileID = nil
            path = nil
        case let .missingDefaultProfile(value):
            type = "missing_default_profile"
            id = value
            profileID = nil
            path = nil
        case let .unavailableExecutable(profileID, path):
            type = "unavailable_executable"
            id = nil
            self.profileID = profileID
            self.path = path
        case .coreUnavailable:
            type = "core_unavailable"
            id = nil
            profileID = nil
            path = nil
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case id
        case profileID = "profile_id"
        case path
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

    init(
        _ state: ShellStateSnapshot,
        zoomedPaneIDByTabID: [String: String] = [:],
        terminalMetadataByContentID: [String: PortableTerminalRuntimeMetadata] = [:]
    ) {
        let contentState = state.contentStateProjection()
        contractVersion = contentState.contractVersion
        windowID = contentState.windowID
        focusedSpaceID = contentState.focusedSpaceID
        focusedTabID = contentState.focusedTabID
        focusedPaneID = contentState.focusedPaneSlotID
        spaces = contentState.spaces.map {
            PortableSpace($0, zoomedPaneIDByTabID: zoomedPaneIDByTabID)
        }
        paneSlots = contentState.paneSlots
        contents = contentState.contents.map {
            PortableContentInstance(
                $0,
                terminalMetadata: terminalMetadataByContentID[$0.contentID]
            )
        }
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

    init(_ space: ShellContentSpace, zoomedPaneIDByTabID: [String: String]) {
        spaceID = space.spaceID
        title = space.title
        attention = space.attention
        tabs = space.tabs.map {
            PortableTab($0, zoomedPaneID: zoomedPaneIDByTabID[$0.tabID])
        }
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
    let zoomedPaneID: String?
    let isPinned: Bool
    let isTitleUserLocked: Bool

    init(_ tab: ShellContentTab, zoomedPaneID: String?) {
        tabID = tab.tabID
        kind = tab.kind
        title = tab.title
        paneTree = PortablePaneTreeNode(tab.paneTree)
        self.zoomedPaneID = zoomedPaneID
        isPinned = tab.isPinned
        isTitleUserLocked = tab.isTitleUserLocked
    }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case zoomedPaneID = "zoomed_pane_id"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(tabID, forKey: .tabID)
        try container.encode(kind, forKey: .kind)
        try container.encodeIfPresent(title, forKey: .title)
        try container.encode(paneTree, forKey: .paneTree)
        try container.encodeIfPresent(zoomedPaneID, forKey: .zoomedPaneID)
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

    init(_ content: ShellContentInstance, terminalMetadata: PortableTerminalRuntimeMetadata?) {
        contentID = content.contentID
        kind = content.kind
        title = content.title
        iconName = content.iconName
        capabilities = content.capabilities
        self.terminalMetadata = terminalMetadata
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
        try container.encodeIfPresent(terminalMetadata, forKey: .terminalMetadata)
        try container.encode(lifecycle, forKey: .lifecycle)
    }
}

private struct PortableTerminalRuntimeMetadata: Encodable {
    let title: String?
    let cwd: String?
    let activeTaskState: ShellTabActiveTaskState
    let activity: TerminalActivitySnapshot?

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
