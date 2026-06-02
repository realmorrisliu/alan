import Foundation

@main
struct ShellTabOrganizationTestRunner {
    static func main() throws {
        try ShellTabOrganizationTests.run()
    }
}

private enum ShellTabOrganizationTests {
    static func run() throws {
        try verifiesSameSectionReorderPreservesTabAndPaneIdentity()
        try verifiesCrossSectionPinAndUnpinPreservePaneIdentity()
        try verifiesMoveCurrentTabToSpaceFollowsSelection()
        try verifiesMoveNonCurrentTabToSpaceDoesNotFollowSelection()
        try verifiesMoveCurrentTabToSpaceRepairsSpaceLocalSelection()
        try verifiesMoveNonCurrentTabToSpacePreservesDestinationSelection()
        try verifiesClosingRememberedBackgroundTabRepairsSpaceSelection()
        try verifiesMissingMoveTargetIsRejectedWithoutStateChange()
        print("Shell tab organization tests passed.")
    }

    private static func verifiesSameSectionReorderPreservesTabAndPaneIdentity() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        state = try state.openingTerminalTab(in: "space_main", title: "Three", workingDirectory: "/tmp").state

        let movedTabID = try requireFocusedTabID(in: state)
        let movedPaneIDs = try requireTab(movedTabID, in: state).paneTree.paneIDs
        let reordered = try state.movingTab(movedTabID, sectionOffset: -1).state

        expect(
            reordered.space(spaceID: "space_main")?.unpinnedTabs.map(\.tabID) == ["tab_main", movedTabID, "tab_2"],
            "same-section reorder must update only section order"
        )
        let reorderedPaneIDs = try requireTab(movedTabID, in: reordered).paneTree.paneIDs
        expect(
            reorderedPaneIDs == movedPaneIDs,
            "same-section reorder must preserve pane identity"
        )
        expect(
            reordered.panes(in: movedTabID).map(\.paneID) == movedPaneIDs,
            "same-section reorder must keep panes attached to the moved tab"
        )
    }

    private static func verifiesCrossSectionPinAndUnpinPreservePaneIdentity() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let tabID = try requireFocusedTabID(in: state)
        let paneIDs = try requireTab(tabID, in: state).paneTree.paneIDs

        let pinned = try state.pinningTab(tabID).state
        expect(
            pinned.tabOrganizationLocation(tabID: tabID)
                == ShellTabOrganizationLocation(spaceID: "space_main", section: .pinned, index: 0),
            "pinning must move the tab into the pinned section"
        )
        let pinnedPaneIDs = try requireTab(tabID, in: pinned).paneTree.paneIDs
        expect(pinnedPaneIDs == paneIDs, "pinning must preserve pane IDs")

        let unpinned = try pinned.unpinningTab(tabID).state
        expect(
            unpinned.tabOrganizationLocation(tabID: tabID)?.section == .unpinned,
            "unpinning must move the tab into the unpinned section"
        )
        let unpinnedPaneIDs = try requireTab(tabID, in: unpinned).paneTree.paneIDs
        expect(unpinnedPaneIDs == paneIDs, "unpinning must preserve pane IDs")
    }

    private static func verifiesMoveCurrentTabToSpaceFollowsSelection() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let movedTabID = try requireFocusedTabID(in: state)
        let focusedPaneID = try requireFocusedPaneID(in: state)
        state = state.creatingTerminalSpace(title: "Target", workingDirectory: "/tmp").state

        let moved = try state
            .focusingPane(focusedPaneID).state
            .movingTabToSpace(tabID: movedTabID, targetSpaceID: "space_2").state

        expect(moved.focusedSpaceID == "space_2", "moving the current tab must follow the target space")
        expect(moved.focusedTabID == movedTabID, "moving the current tab must keep it selected")
        expect(moved.focusedPaneID == focusedPaneID, "moving the current tab must keep preferred pane focus")
        expect(
            moved.panes(in: movedTabID).allSatisfy { $0.spaceID == "space_2" },
            "moving a tab to another space must update pane space ownership"
        )
    }

    private static func verifiesMoveNonCurrentTabToSpaceDoesNotFollowSelection() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let nonCurrentTabID = try requireFocusedTabID(in: state)
        state = state.creatingTerminalSpace(title: "Target", workingDirectory: "/tmp").state
        state = try state.focusingPane("pane_1").state

        let focusedBefore = (
            spaceID: state.focusedSpaceID,
            tabID: state.focusedTabID,
            paneID: state.focusedPaneID
        )
        let moved = try state.movingTabToSpace(tabID: nonCurrentTabID, targetSpaceID: "space_2").state

        expect(moved.focusedSpaceID == focusedBefore.spaceID, "moving a non-current tab must keep current space")
        expect(moved.focusedTabID == focusedBefore.tabID, "moving a non-current tab must keep current tab")
        expect(moved.focusedPaneID == focusedBefore.paneID, "moving a non-current tab must keep current pane")
    }

    private static func verifiesMoveCurrentTabToSpaceRepairsSpaceLocalSelection() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let movedTabID = try requireFocusedTabID(in: state)
        let focusedPaneID = try requireFocusedPaneID(in: state)
        state = state.creatingTerminalSpace(title: "Target", workingDirectory: "/tmp").state

        let moved = try state
            .focusingPane(focusedPaneID).state
            .movingTabToSpace(tabID: movedTabID, targetSpaceID: "space_2").state

        expect(
            moved.space(spaceID: "space_main")?.selectedTabID == "tab_main",
            "moving the remembered selected tab away must repair the source space selection"
        )
        expect(
            moved.space(spaceID: "space_2")?.selectedTabID == movedTabID,
            "moving the current tab must make the destination space remember it"
        )
    }

    private static func verifiesMoveNonCurrentTabToSpacePreservesDestinationSelection() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let movedTabID = try requireFocusedTabID(in: state)
        state = state.creatingTerminalSpace(title: "Target", workingDirectory: "/tmp").state
        state = try state.focusingPane("pane_1").state

        let moved = try state.movingTabToSpace(tabID: movedTabID, targetSpaceID: "space_2").state

        expect(
            moved.space(spaceID: "space_main")?.selectedTabID == "tab_main",
            "source space selection must stay repaired to the focused tab after moving a background tab"
        )
        expect(
            moved.space(spaceID: "space_2")?.selectedTabID == "tab_3",
            "moving a non-current tab must preserve the destination space remembered tab"
        )
    }

    private static func verifiesClosingRememberedBackgroundTabRepairsSpaceSelection() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTerminalTab(in: "space_main", title: "Two", workingDirectory: "/tmp").state
        let closedTabID = try requireFocusedTabID(in: state)
        state = state.creatingTerminalSpace(title: "Target", workingDirectory: "/tmp").state

        let closed = try state.closingTab(closedTabID).state

        expect(
            closed.space(spaceID: "space_main")?.selectedTabID == "tab_main",
            "closing an inactive space's remembered tab must repair to the first retained tab"
        )
        expect(
            closed.focusedSpaceID == "space_2" && closed.focusedTabID == "tab_3",
            "closing a background remembered tab must preserve current focus"
        )
    }

    private static func verifiesMissingMoveTargetIsRejectedWithoutStateChange() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        do {
            _ = try state.movingTabToSpace(tabID: "tab_main", targetSpaceID: "missing_space")
            fail("missing target space must be rejected")
        } catch ShellStateMutationError.spaceNotFound {
            expect(state == ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp"), "failed move must leave state unchanged")
        }
    }

    private static func requireFocusedTabID(in state: ShellStateSnapshot) throws -> String {
        guard let tabID = state.focusedTabID else {
            throw TestFailure("missing focused tab")
        }
        return tabID
    }

    private static func requireFocusedPaneID(in state: ShellStateSnapshot) throws -> String {
        guard let paneID = state.focusedPaneID else {
            throw TestFailure("missing focused pane")
        }
        return paneID
    }

    private static func requireTab(_ tabID: String, in state: ShellStateSnapshot) throws -> ShellTab {
        guard let tab = state.tab(tabID: tabID) else {
            throw TestFailure("missing tab \(tabID)")
        }
        return tab
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
