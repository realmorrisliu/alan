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
        try verifiesOneSpaceUsesFullTitleTrackTarget()
        try verifiesSeveralSpacesUseReadableTrackTargets()
        try verifiesReadableSpacesDistributeAcrossTheFullTrack()
        try verifiesTruncatedTitlesWhenPartialWidthFits()
        try verifiesTruncatedSpacesDistributeAcrossTheFullTrack()
        try verifiesIconOnlySpacesDistributeUntilMinimumWidth()
        try verifiesMoreThanNineSpacesParticipate()
        try verifiesIconOnlyCollapseAndOverflowSizing()
        try verifiesHoverDoesNotChangeGeometry()
        try verifiesScrubFocusIsDistinctFromSelectedSpace()
        try verifiesReducedMotionPreservesStableGeometry()
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

    private static func verifiesOneSpaceUsesFullTitleTrackTarget() throws {
        let layout = make(spaceCount: 1, selectedIndex: 0, availableWidth: 240)

        expect(layout.items.count == 1, "single-Space layout must include the Space")
        expect(layout.items[0].mode == .fullTitle, "single-Space target must show icon and full title")
        expect(layout.items[0].isSelected, "selected item must be preserved")
        expect(!layout.isHorizontallyScrollable, "single-Space track must not require overflow scrolling")
        expect(layout.contentWidth <= 240, "single-Space content must fit inside the track")
    }

    private static func verifiesSeveralSpacesUseReadableTrackTargets() throws {
        let layout = make(spaceCount: 3, selectedIndex: 1, availableWidth: 300)

        expect(
            layout.items.map(\.mode) == [.fullTitle, .fullTitle, .fullTitle],
            "readable track width must show icon and full title for every Space"
        )
        expect(layout.contentWidth <= 300, "readable targets must fit without overflow")
    }

    private static func verifiesReadableSpacesDistributeAcrossTheFullTrack() throws {
        let layout = make(spaceCount: 2, selectedIndex: 0, availableWidth: 300)
        let expectedItemWidth = (300 - ShellSidebarSpaceSliderLayout.spacing) / 2

        expect(
            layout.items.map(\.mode) == [.fullTitle, .fullTitle],
            "wide readable Spaces must keep full title labels"
        )
        expect(
            layout.items.allSatisfy { rounded($0.width) == rounded(expectedItemWidth) },
            "readable Space targets must distribute across the full track instead of using a fixed maximum width"
        )
        expect(
            rounded(layout.contentWidth) == rounded(300),
            "readable Space targets must fill the whole track before minimum-width collapse"
        )
    }

    private static func verifiesTruncatedTitlesWhenPartialWidthFits() throws {
        let layout = make(spaceCount: 3, selectedIndex: 1, availableWidth: 180)

        expect(
            layout.items.map(\.mode) == [.truncatedTitle, .truncatedTitle, .truncatedTitle],
            "partial track width must keep icon and truncated title targets before icon-only collapse"
        )
        expect(layout.contentWidth <= 180, "truncated targets must fit inside the available track")
        expect(!layout.isHorizontallyScrollable, "truncated targets must not scroll until minimums overflow")
    }

    private static func verifiesTruncatedSpacesDistributeAcrossTheFullTrack() throws {
        let layout = make(spaceCount: 4, selectedIndex: 1, availableWidth: 240)
        let expectedItemWidth = (240 - ShellSidebarSpaceSliderLayout.spacing * 3) / 4

        expect(
            layout.items.map(\.mode) == [
                .truncatedTitle,
                .truncatedTitle,
                .truncatedTitle,
                .truncatedTitle,
            ],
            "partial-width Spaces must keep truncated title labels before icon-only collapse"
        )
        expect(
            layout.items.allSatisfy { rounded($0.width) == rounded(expectedItemWidth) },
            "truncated Space targets must distribute across the full track instead of using a fixed width"
        )
        expect(
            rounded(layout.contentWidth) == rounded(240),
            "truncated Space targets must fill the whole track before minimum-width collapse"
        )
    }

    private static func verifiesIconOnlySpacesDistributeUntilMinimumWidth() throws {
        let layout = make(spaceCount: 7, selectedIndex: 3, availableWidth: 240)
        let expectedItemWidth = (240 - ShellSidebarSpaceSliderLayout.spacing * 6) / 7

        expect(
            layout.items.allSatisfy { $0.mode == .iconOnly },
            "narrow Spaces may collapse to icon-only before minimum overflow"
        )
        expect(
            layout.items.allSatisfy { rounded($0.width) == rounded(expectedItemWidth) },
            "icon-only Space targets must still distribute across the full track until the minimum width is reached"
        )
        expect(
            rounded(layout.contentWidth) == rounded(240),
            "icon-only Space targets above the minimum width must fill the whole track without scrolling"
        )
        expect(!layout.isHorizontallyScrollable, "icon-only distribution above minimum width must not scroll")
    }

    private static func verifiesMoreThanNineSpacesParticipate() throws {
        let layout = make(spaceCount: 12, selectedIndex: 10, availableWidth: 360)

        expect(layout.items.count == 12, "layout must include every Space instead of capping at nine")
        expect(layout.items[10].isSelected, "selected Space beyond the old cap must remain selectable")
        expect(layout.items.map(\.index) == Array(0..<12), "item indices must preserve every Space target")
    }

    private static func verifiesIconOnlyCollapseAndOverflowSizing() throws {
        let layout = make(spaceCount: 14, selectedIndex: 12, availableWidth: 240)

        expect(
            layout.items.allSatisfy { $0.mode == .iconOnly },
            "overflow track must collapse every Space to icon-only minimum targets"
        )
        expect(layout.items.count == 14, "icon-only overflow must still include every Space")
        expect(layout.contentWidth > 240, "minimum icon-only targets must report overflow content width")
        expect(layout.isHorizontallyScrollable, "minimum overflow must request horizontal track scrolling")
    }

    private static func verifiesHoverDoesNotChangeGeometry() throws {
        let base = make(spaceCount: 9, selectedIndex: 0, availableWidth: 240)
        let hovered = make(spaceCount: 9, selectedIndex: 0, hoveredIndex: 5, availableWidth: 240)

        expect(geometrySignature(base) == geometrySignature(hovered), "hover must not change Space frames")
        expect(hovered.items[5].isFocused, "hovered item must still be identified as focused")
    }

    private static func verifiesScrubFocusIsDistinctFromSelectedSpace() throws {
        let base = make(spaceCount: 9, selectedIndex: 0, availableWidth: 240)
        let focused = make(spaceCount: 9, selectedIndex: 0, scrubFocusIndex: 6, availableWidth: 240)

        expect(focused.items[0].isSelected, "selected Space marker must remain on the active Space")
        expect(focused.items[6].isFocused, "scrub focus must mark the preview Space")
        expect(geometrySignature(base) == geometrySignature(focused), "scrub focus must not shift target frames")
    }

    private static func verifiesReducedMotionPreservesStableGeometry() throws {
        let animated = ShellSidebarSpaceSliderLayout.make(
            spaceCount: 9,
            selectedIndex: 0,
            scrubFocusIndex: 4,
            availableWidth: 240,
            reduceMotion: false
        )
        let reduced = ShellSidebarSpaceSliderLayout.make(
            spaceCount: 9,
            selectedIndex: 0,
            scrubFocusIndex: 4,
            availableWidth: 240,
            reduceMotion: true
        )

        expect(
            geometrySignature(animated) == geometrySignature(reduced),
            "reduced motion must preserve the same stable target geometry"
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
        let layout = make(spaceCount: 12, selectedIndex: 2, availableWidth: 360)
        var scrub = try makeScrub(source: .drag, selectedIndex: 2, spaceCount: 12)
        let targetX = try expectValue(layout.midpoint(for: 6), "target midpoint must exist")

        scrub.updateDrag(locationX: targetX, translationX: 3, layout: layout)
        expect(scrub.focusIndex == 2, "drag below threshold must not move preview focus")

        scrub.updateDrag(locationX: targetX, translationX: 48, layout: layout)
        expect(scrub.focusIndex == 6, "drag scrub must preview the target under the pointer")
        expect(scrub.hasPreviewTarget, "drag scrub must distinguish preview from selected Space")
        expect(scrub.commitIndex == 6, "drag release must commit the focused Space")
    }

    private static func verifiesDragScrubUsesEdgeResistanceAtBounds() throws {
        let layout = make(spaceCount: 12, selectedIndex: 4, availableWidth: 360)
        var leadingScrub = try makeScrub(source: .drag, selectedIndex: 4, spaceCount: 12)
        var trailingScrub = try makeScrub(source: .drag, selectedIndex: 4, spaceCount: 12)

        leadingScrub.updateDrag(locationX: -40, translationX: -80, layout: layout)
        trailingScrub.updateDrag(locationX: layout.contentWidth + 40, translationX: 80, layout: layout)

        expect(leadingScrub.focusIndex == 0, "leading edge drag must clamp to the first Space")
        expect(
            leadingScrub.edgeResistanceOffset < 0,
            "leading edge drag must expose a resisted offset"
        )
        expect(trailingScrub.focusIndex == 11, "trailing edge drag must clamp to the last Space")
        expect(
            trailingScrub.edgeResistanceOffset > 0,
            "trailing edge drag must expose a resisted offset"
        )
    }

    private static func verifiesWheelScrubPreviewAndCommitTarget() throws {
        var scrub = try makeScrub(source: .wheel, selectedIndex: 3, spaceCount: 12)

        scrub.updateWheel(deltaX: 10, itemSpan: 24, spaceCount: 12)
        expect(scrub.focusIndex == 3, "small wheel scrub must enter preview without moving focus")

        scrub.updateWheel(deltaX: 58, itemSpan: 24, spaceCount: 12)
        expect(scrub.focusIndex == 5, "wheel scrub must move preview focus after enough horizontal input")
        expect(scrub.hasPreviewTarget, "wheel scrub must preview before commit")
        expect(scrub.commitIndex == 5, "wheel dwell must commit the focused Space")
    }

    private static func verifiesScrubCancelRestoresTheSelectedSource() throws {
        let layout = make(spaceCount: 12, selectedIndex: 2, availableWidth: 360)
        var scrub = try makeScrub(source: .drag, selectedIndex: 2, spaceCount: 12)
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
        scrubFocusIndex: Int? = nil,
        availableWidth: CGFloat = 240
    ) -> ShellSidebarSpaceSliderLayout {
        ShellSidebarSpaceSliderLayout.make(
            spaceCount: spaceCount,
            selectedIndex: selectedIndex,
            hoveredIndex: hoveredIndex,
            scrubFocusIndex: scrubFocusIndex,
            availableWidth: availableWidth,
            reduceMotion: false
        )
    }

    private static func geometrySignature(
        _ layout: ShellSidebarSpaceSliderLayout
    ) -> [String] {
        layout.items.map { item in
            "\(item.index):\(item.mode):\(rounded(item.width))"
        } + ["content:\(rounded(layout.contentWidth))"]
    }

    private static func rounded(_ value: CGFloat) -> Int {
        Int((value * 100).rounded())
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
