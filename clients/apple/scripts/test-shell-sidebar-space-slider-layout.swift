import CoreGraphics
import Foundation

@main
struct ShellSidebarSpaceSliderLayoutTestRunner {
    static func main() throws {
        try ShellSidebarSpaceSliderLayoutTests.run()
    }
}

private enum ShellSidebarSpaceSliderLayoutTests {
    static func run() throws {
        try verifiesDensityTiers()
        try verifiesLowDensityUsesFullTitles()
        try verifiesMediumDensityUsesSelectedFullTitleAndShortTitles()
        try verifiesHighDensityUsesSelectedTitleAndIndicators()
        try verifiesHighDensityHoverExpandsTheHoveredIndicator()
        try verifiesScrubFocusIsDistinctFromSelectedSpace()
        try verifiesMaximumVisibleSpaceCap()
        try verifiesReducedMotionRemovesScaleEmphasis()
        try verifiesClickSelectionRules()
        try verifiesDragScrubPreviewAndCommitTarget()
        try verifiesDragScrubUsesEdgeResistanceAtBounds()
        try verifiesWheelScrubPreviewAndCommitTarget()
        try verifiesScrubCancelRestoresTheSelectedSource()
        try verifiesWheelIntentRoutingProtectsVerticalScroll()
        try verifiesPassThroughWheelForwardingDecision()
        try verifiesPhaseLessWheelResetSchedulerResetsAfterIdle()
        print("Shell sidebar Space slider layout tests passed.")
    }

    private static func verifiesDensityTiers() throws {
        expect(ShellSidebarSpaceSliderLayout.density(for: 1) == .low, "1 Space must be low density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 3) == .low, "3 Spaces must be low density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 4) == .medium, "4 Spaces must be medium density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 6) == .medium, "6 Spaces must be medium density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 7) == .high, "7 Spaces must be high density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 9) == .high, "9 Spaces must be high density")
        expect(ShellSidebarSpaceSliderLayout.density(for: 12) == .high, "Space count must cap into high density")
    }

    private static func verifiesLowDensityUsesFullTitles() throws {
        let layout = make(spaceCount: 3, selectedIndex: 1)

        expect(layout.density == .low, "3 Spaces must use low density")
        expect(
            layout.items.map(\.mode) == [.fullTitle, .fullTitle, .fullTitle],
            "low density must render every Space as a full title"
        )
        expect(layout.items[1].isSelected, "selected item must be preserved")
    }

    private static func verifiesMediumDensityUsesSelectedFullTitleAndShortTitles() throws {
        let layout = make(spaceCount: 6, selectedIndex: 2)

        expect(layout.density == .medium, "6 Spaces must use medium density")
        expect(layout.items[2].mode == .fullTitle, "selected medium-density Space must be full title")
        expect(
            layout.items.enumerated().allSatisfy { index, item in
                index == 2 || item.mode == .shortTitle
            },
            "inactive medium-density Spaces must be short titles"
        )
    }

    private static func verifiesHighDensityUsesSelectedTitleAndIndicators() throws {
        let layout = make(spaceCount: 9, selectedIndex: 4)

        expect(layout.density == .high, "9 Spaces must use high density")
        expect(layout.items[4].mode == .fullTitle, "selected high-density Space must be full title")
        expect(
            layout.items.enumerated().allSatisfy { index, item in
                index == 4 || item.mode == .indicator
            },
            "inactive high-density Spaces must be indicators"
        )
    }

    private static func verifiesHighDensityHoverExpandsTheHoveredIndicator() throws {
        let layout = make(spaceCount: 9, selectedIndex: 0, hoveredIndex: 5)

        expect(layout.items[5].mode == .shortTitle, "hovered high-density indicator must expand to a short title")
        expect(layout.items[5].isFocused, "hovered item must become focused for local preview")
        expect(layout.items[4].visualScale >= 1, "neighboring items must remain visually stable")
    }

    private static func verifiesScrubFocusIsDistinctFromSelectedSpace() throws {
        let layout = make(spaceCount: 9, selectedIndex: 0, scrubFocusIndex: 6)

        expect(layout.items[0].isSelected, "selected Space marker must remain on the active Space")
        expect(layout.items[6].isFocused, "scrub focus must mark the preview Space")
        expect(layout.items[6].mode == .fullTitle, "scrub-focused high-density Space must become a title")
    }

    private static func verifiesMaximumVisibleSpaceCap() throws {
        let layout = make(spaceCount: 12, selectedIndex: 10)

        expect(
            ShellSidebarSpaceSliderLayout.maximumVisibleSpaces == 9,
            "maximum visible Space count must be 9"
        )
        expect(layout.items.count == 9, "layout must cap rendered Spaces at 9")
        expect(layout.items[8].isSelected, "selected index must clamp into the visible range")
    }

    private static func verifiesReducedMotionRemovesScaleEmphasis() throws {
        let layout = ShellSidebarSpaceSliderLayout.make(
            spaceCount: 9,
            selectedIndex: 0,
            scrubFocusIndex: 4,
            availableWidth: 240,
            reduceMotion: true
        )

        expect(
            layout.items.allSatisfy { $0.visualScale == 1 },
            "reduced motion must disable scale emphasis"
        )
    }

    private static func verifiesClickSelectionRules() throws {
        expect(
            ShellSidebarSpaceSliderClickSelection.targetIndex(
                selectedIndex: 2,
                clickedIndex: 2,
                spaceCount: 6
            ) == nil,
            "clicking the selected Space must leave selection unchanged"
        )
        expect(
            ShellSidebarSpaceSliderClickSelection.targetIndex(
                selectedIndex: 2,
                clickedIndex: 4,
                spaceCount: 6
            ) == 4,
            "clicking a non-selected Space must produce an immediate switch target"
        )
        expect(
            ShellSidebarSpaceSliderClickSelection.targetIndex(
                selectedIndex: 2,
                clickedIndex: 9,
                spaceCount: 6
            ) == nil,
            "clicking outside visible Space bounds must not select"
        )
    }

    private static func verifiesDragScrubPreviewAndCommitTarget() throws {
        let layout = make(spaceCount: 9, selectedIndex: 2)
        var scrub = try makeScrub(source: .drag, selectedIndex: 2, spaceCount: 9)
        let targetX = try expectValue(layout.midpoint(for: 6), "target midpoint must exist")

        scrub.updateDrag(locationX: targetX, translationX: 3, layout: layout)
        expect(scrub.focusIndex == 2, "drag below threshold must not move preview focus")

        scrub.updateDrag(locationX: targetX, translationX: 48, layout: layout)
        expect(scrub.focusIndex == 6, "drag scrub must preview the target under the pointer")
        expect(scrub.hasPreviewTarget, "drag scrub must distinguish preview from selected Space")
        expect(scrub.commitIndex == 6, "drag release must commit the focused Space")
    }

    private static func verifiesDragScrubUsesEdgeResistanceAtBounds() throws {
        let layout = make(spaceCount: 9, selectedIndex: 4)
        var leadingScrub = try makeScrub(source: .drag, selectedIndex: 4, spaceCount: 9)
        var trailingScrub = try makeScrub(source: .drag, selectedIndex: 4, spaceCount: 9)

        leadingScrub.updateDrag(locationX: -40, translationX: -80, layout: layout)
        trailingScrub.updateDrag(locationX: layout.contentWidth + 40, translationX: 80, layout: layout)

        expect(leadingScrub.focusIndex == 0, "leading edge drag must clamp to the first Space")
        expect(
            leadingScrub.edgeResistanceOffset < 0,
            "leading edge drag must expose a resisted offset"
        )
        expect(trailingScrub.focusIndex == 8, "trailing edge drag must clamp to the last Space")
        expect(
            trailingScrub.edgeResistanceOffset > 0,
            "trailing edge drag must expose a resisted offset"
        )
    }

    private static func verifiesWheelScrubPreviewAndCommitTarget() throws {
        var scrub = try makeScrub(source: .wheel, selectedIndex: 3, spaceCount: 9)

        scrub.updateWheel(deltaX: 10, itemSpan: 24, spaceCount: 9)
        expect(scrub.focusIndex == 3, "small wheel scrub must enter preview without moving focus")

        scrub.updateWheel(deltaX: 58, itemSpan: 24, spaceCount: 9)
        expect(scrub.focusIndex == 5, "wheel scrub must move preview focus after enough horizontal input")
        expect(scrub.hasPreviewTarget, "wheel scrub must preview before commit")
        expect(scrub.commitIndex == 5, "wheel dwell must commit the focused Space")
    }

    private static func verifiesScrubCancelRestoresTheSelectedSource() throws {
        let layout = make(spaceCount: 9, selectedIndex: 2)
        var scrub = try makeScrub(source: .drag, selectedIndex: 2, spaceCount: 9)
        let targetX = try expectValue(layout.midpoint(for: 6), "target midpoint must exist")

        scrub.updateDrag(locationX: targetX, translationX: 48, layout: layout)

        expect(scrub.focusIndex == 6, "scrub preview must move before cancel")
        expect(scrub.cancelIndex == 2, "scrub cancel must restore the selected source Space")
    }

    private static func verifiesWheelIntentRoutingProtectsVerticalScroll() throws {
        var verticalState = ShellSidebarSpaceSliderWheelIntentState()
        let verticalRoute = verticalState.route(deltaX: 1, deltaY: 8)
        expect(
            verticalRoute == .passThrough,
            "vertical wheel input over the slider must pass through to tab-list scrolling"
        )
        let verticalJitterRoute = verticalState.route(deltaX: 12, deltaY: 1)
        expect(
            verticalJitterRoute == .passThrough,
            "vertical wheel intent must stay pass-through for the rest of the gesture"
        )

        verticalState.reset()
        let postResetHorizontalRoute = verticalState.route(deltaX: 8, deltaY: 1)
        expect(
            postResetHorizontalRoute == .scrub(deltaX: 8),
            "wheel intent reset must allow a later horizontal gesture to scrub Spaces"
        )

        var ambiguousState = ShellSidebarSpaceSliderWheelIntentState()
        let ambiguousRoute = ambiguousState.route(deltaX: 4, deltaY: 4)
        expect(
            ambiguousRoute == .passThrough,
            "ambiguous wheel input over the slider must not enter Space scrub"
        )

        var horizontalState = ShellSidebarSpaceSliderWheelIntentState()
        let horizontalRoute = horizontalState.route(deltaX: 8, deltaY: 1)
        expect(
            horizontalRoute == .scrub(deltaX: 8),
            "clear horizontal wheel input over the slider must enter Space scrub"
        )
    }

    private static func verifiesPassThroughWheelForwardingDecision() throws {
        expect(
            ShellSidebarSpaceSliderWheelForwarding
                .shouldForwardPassThroughToTabList(deltaX: 1, deltaY: 8),
            "vertical pass-through wheel input over the fixed slider must be forwarded to the tab list"
        )
        expect(
            ShellSidebarSpaceSliderWheelForwarding
                .shouldForwardPassThroughToTabList(deltaX: 4, deltaY: 4),
            "ambiguous pass-through wheel input with vertical delta must still reach tab-list scrolling"
        )
        expect(
            !ShellSidebarSpaceSliderWheelForwarding
                .shouldForwardPassThroughToTabList(deltaX: 8, deltaY: 0),
            "horizontal-only pass-through wheel input must not be forwarded as vertical tab-list scrolling"
        )
    }

    private static func verifiesPhaseLessWheelResetSchedulerResetsAfterIdle() throws {
        var resetCount = 0
        var scheduledWorkItems: [DispatchWorkItem] = []
        var scheduledDelays: [TimeInterval] = []
        let scheduler = ShellSidebarSpaceSliderWheelPhaseLessResetScheduler(
            onReset: {
                resetCount += 1
            },
            scheduleWorkItem: { workItem, delay in
                scheduledWorkItems.append(workItem)
                scheduledDelays.append(delay)
            }
        )

        scheduler.scheduleResetAfterIdle()
        expect(scheduledWorkItems.count == 1, "phase-less wheel input must schedule an idle reset")
        expect(
            scheduledDelays == [ShellSidebarSpaceSliderWheelPhaseLessResetScheduler.resetDelay],
            "phase-less wheel reset must use the shared idle delay"
        )

        let firstWorkItem = scheduledWorkItems[0]
        scheduler.scheduleResetAfterIdle()
        firstWorkItem.perform()
        expect(resetCount == 0, "a later phase-less event must cancel the prior idle reset")

        scheduledWorkItems[1].perform()
        expect(resetCount == 1, "phase-less idle reset must fire after the final scheduled event")

        scheduler.scheduleResetAfterIdle()
        let pendingWorkItem = scheduledWorkItems[2]
        scheduler.resetNow()
        pendingWorkItem.perform()
        expect(resetCount == 2, "explicit wheel boundaries must cancel pending phase-less reset")
    }

    private static func make(
        spaceCount: Int,
        selectedIndex: Int?,
        hoveredIndex: Int? = nil,
        scrubFocusIndex: Int? = nil
    ) -> ShellSidebarSpaceSliderLayout {
        ShellSidebarSpaceSliderLayout.make(
            spaceCount: spaceCount,
            selectedIndex: selectedIndex,
            hoveredIndex: hoveredIndex,
            scrubFocusIndex: scrubFocusIndex,
            availableWidth: 240,
            reduceMotion: false
        )
    }

    private static func makeScrub(
        source: ShellSidebarSpaceSliderScrubSource,
        selectedIndex: Int?,
        spaceCount: Int
    ) throws -> ShellSidebarSpaceSliderScrubState {
        try expectValue(
            ShellSidebarSpaceSliderScrubState(
                source: source,
                selectedIndex: selectedIndex,
                spaceCount: spaceCount
            ),
            "scrub state must be created"
        )
    }

    private static func expectValue<T>(_ value: T?, _ message: String) throws -> T {
        guard let value else {
            throw TestFailure(message)
        }
        return value
    }

    private static func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
        guard condition() else {
            fatalError(message)
        }
    }

    private struct TestFailure: Error {
        let message: String

        init(_ message: String) {
            self.message = message
        }
    }
}
