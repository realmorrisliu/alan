import CoreGraphics
import Foundation

#if os(macOS)
struct ShellSidebarSpaceSliderLayout: Equatable {
    static let spacing: CGFloat = 4
    static let trackHeight: CGFloat = 28
    static let itemHeight: CGFloat = 24
    static let fullTitleMinimumWidth: CGFloat = 92
    static let truncatedTitleMinimumWidth: CGFloat = 56
    static let minimumItemWidth: CGFloat = 28

    enum DisplayMode: Equatable {
        case fullTitle
        case truncatedTitle
        case iconOnly
    }

    struct Item: Equatable {
        let index: Int
        let mode: DisplayMode
        let width: CGFloat
        let isSelected: Bool
        let isFocused: Bool
    }

    let items: [Item]
    let contentWidth: CGFloat
    let availableWidth: CGFloat

    var isHorizontallyScrollable: Bool {
        contentWidth > availableWidth
    }

    static func make(
        spaceCount rawSpaceCount: Int,
        selectedIndex rawSelectedIndex: Int?,
        hoveredIndex rawHoveredIndex: Int? = nil,
        scrubFocusIndex rawScrubFocusIndex: Int? = nil,
        availableWidth: CGFloat,
        reduceMotion: Bool
    ) -> ShellSidebarSpaceSliderLayout {
        _ = reduceMotion
        let spaceCount = max(rawSpaceCount, 0)
        let availableWidth = max(availableWidth, 0)
        guard spaceCount > 0 else {
            return ShellSidebarSpaceSliderLayout(
                items: [],
                contentWidth: 0,
                availableWidth: availableWidth
            )
        }

        let selectedIndex = clamped(rawSelectedIndex, count: spaceCount)
        let hoveredIndex = clamped(rawHoveredIndex, count: spaceCount)
        let scrubFocusIndex = clamped(rawScrubFocusIndex, count: spaceCount)
        let focusIndex = scrubFocusIndex ?? hoveredIndex
        let itemWidth = distributedItemWidth(spaceCount: spaceCount, availableWidth: availableWidth)
        let mode = displayMode(itemWidth: itemWidth)

        let items = (0..<spaceCount).map { index in
            let isFocused = focusIndex == index
            return Item(
                index: index,
                mode: mode,
                width: itemWidth,
                isSelected: selectedIndex == index,
                isFocused: isFocused
            )
        }

        let contentWidth = CGFloat(spaceCount) * itemWidth
            + CGFloat(max(spaceCount - 1, 0)) * spacing
        return ShellSidebarSpaceSliderLayout(
            items: items,
            contentWidth: contentWidth,
            availableWidth: availableWidth
        )
    }

    private static func displayMode(itemWidth: CGFloat) -> DisplayMode {
        if itemWidth >= fullTitleMinimumWidth {
            return .fullTitle
        }
        if itemWidth >= truncatedTitleMinimumWidth {
            return .truncatedTitle
        }
        return .iconOnly
    }

    private static func distributedItemWidth(
        spaceCount: Int,
        availableWidth: CGFloat
    ) -> CGFloat {
        guard spaceCount > 0 else { return 0 }
        let spacingWidth = CGFloat(max(spaceCount - 1, 0)) * spacing
        let distributedWidth = (availableWidth - spacingWidth) / CGFloat(spaceCount)
        return distributedWidth >= minimumItemWidth ? distributedWidth : minimumItemWidth
    }

    private static func clamped(_ index: Int?, count: Int) -> Int? {
        guard let index, count > 0 else { return nil }
        return min(max(index, 0), count - 1)
    }

    func frame(for index: Int) -> CGRect? {
        var cursor: CGFloat = 0
        for item in items {
            let frame = CGRect(x: cursor, y: 0, width: item.width, height: Self.itemHeight)
            if item.index == index {
                return frame
            }
            cursor += item.width + Self.spacing
        }
        return nil
    }

    func midpoint(for index: Int) -> CGFloat? {
        frame(for: index).map(\.midX)
    }

    func targetIndex(at locationX: CGFloat) -> Int? {
        guard !items.isEmpty else { return nil }
        var cursor: CGFloat = 0
        for item in items {
            let minimumX = cursor - (Self.spacing * 0.5)
            let maximumX = cursor + item.width + (Self.spacing * 0.5)
            if locationX >= minimumX && locationX <= maximumX {
                return item.index
            }
            cursor += item.width + Self.spacing
        }
        return nil
    }

    func clampedTargetIndex(at locationX: CGFloat) -> Int? {
        guard let first = items.first?.index,
              let last = items.last?.index
        else {
            return nil
        }
        if let target = targetIndex(at: locationX) {
            return target
        }
        return locationX < 0 ? first : last
    }
}

enum ShellSidebarSpaceSliderScrubSource: Equatable {
    case drag
    case wheel
    case keyboard
}

struct ShellSidebarSpaceSliderScrubState: Equatable {
    static let dragThreshold: CGFloat = 8
    static let wheelStepWidth: CGFloat = 24
    static let edgeResistanceLimit: CGFloat = 7
    static let wheelCommitDelay: TimeInterval = 0.16

    let source: ShellSidebarSpaceSliderScrubSource
    let sourceIndex: Int
    var focusIndex: Int
    var accumulatedWheelDeltaX: CGFloat = 0
    var edgeResistanceOffset: CGFloat = 0

    init?(source: ShellSidebarSpaceSliderScrubSource, selectedIndex: Int?, spaceCount: Int) {
        guard let selectedIndex,
              spaceCount > 0
        else {
            return nil
        }
        let clampedIndex = Self.clampedIndex(selectedIndex, spaceCount: spaceCount)
        self.source = source
        sourceIndex = clampedIndex
        focusIndex = clampedIndex
    }

    var hasPreviewTarget: Bool {
        focusIndex != sourceIndex
    }

    var commitIndex: Int {
        focusIndex
    }

    var cancelIndex: Int {
        sourceIndex
    }

    mutating func updateDrag(
        locationX: CGFloat,
        translationX: CGFloat,
        layout: ShellSidebarSpaceSliderLayout
    ) {
        guard abs(translationX) >= Self.dragThreshold,
              let targetIndex = layout.clampedTargetIndex(at: locationX)
        else {
            focusIndex = sourceIndex
            edgeResistanceOffset = 0
            return
        }

        focusIndex = targetIndex
        edgeResistanceOffset = Self.edgeResistanceOffset(
            locationX: locationX,
            contentWidth: layout.contentWidth
        )
    }

    mutating func updateWheel(deltaX: CGFloat, itemSpan: CGFloat, spaceCount: Int) {
        accumulatedWheelDeltaX += deltaX
        let stepWidth = max(itemSpan, Self.wheelStepWidth)
        let rawStep = Int((accumulatedWheelDeltaX / stepWidth).rounded(.towardZero))
        let nextIndex = Self.clampedIndex(sourceIndex + rawStep, spaceCount: spaceCount)
        focusIndex = nextIndex
        edgeResistanceOffset = nextIndex == sourceIndex + rawStep ? 0 : Self.edgeResistanceLimit * CGFloat(rawStep.signum())
    }

    mutating func moveFocus(by delta: Int, spaceCount: Int) {
        focusIndex = Self.clampedIndex(focusIndex + delta, spaceCount: spaceCount)
        edgeResistanceOffset = 0
    }

    private static func edgeResistanceOffset(locationX: CGFloat, contentWidth: CGFloat) -> CGFloat {
        if locationX < 0 {
            let distance = abs(locationX)
            return -edgeResistanceLimit * distance / (distance + edgeResistanceLimit)
        }
        if locationX > contentWidth {
            let distance = locationX - contentWidth
            return edgeResistanceLimit * distance / (distance + edgeResistanceLimit)
        }
        return 0
    }

    private static func clampedIndex(_ index: Int, spaceCount: Int) -> Int {
        min(max(index, 0), max(spaceCount - 1, 0))
    }
}

struct ShellSidebarSpaceSliderWheelIntentState: Equatable {
    enum Route: Equatable {
        case passThrough
        case scrub(deltaX: CGFloat)
    }

    private enum Intent: Equatable {
        case undecided
        case horizontal
        case vertical
    }

    static let intentLockDistance: CGFloat = 5
    static let horizontalIntentBias: CGFloat = 1.18
    static let verticalIntentBias: CGFloat = 1.12

    private var accumulatedX: CGFloat = 0
    private var accumulatedY: CGFloat = 0
    private var intent = Intent.undecided

    mutating func route(deltaX: CGFloat, deltaY: CGFloat) -> Route {
        guard abs(deltaX) > 0 || abs(deltaY) > 0 else {
            return .passThrough
        }

        switch intent {
        case .horizontal:
            accumulatedX += deltaX
            accumulatedY += deltaY
            return .scrub(deltaX: deltaX)
        case .vertical:
            return .passThrough
        case .undecided:
            accumulatedX += deltaX
            accumulatedY += deltaY

            if abs(accumulatedY) >= Self.intentLockDistance,
               abs(accumulatedY) >= abs(accumulatedX) * Self.verticalIntentBias
            {
                intent = .vertical
                return .passThrough
            }

            guard abs(accumulatedX) >= Self.intentLockDistance,
                  abs(accumulatedX) > abs(accumulatedY) * Self.horizontalIntentBias
            else {
                return .passThrough
            }

            intent = .horizontal
            return .scrub(deltaX: accumulatedX)
        }
    }

    mutating func reset() {
        accumulatedX = 0
        accumulatedY = 0
        intent = .undecided
    }
}

enum ShellSidebarSpaceSliderClickSelection {
    static func targetIndex(selectedIndex: Int?, clickedIndex: Int, spaceCount: Int) -> Int? {
        guard clickedIndex >= 0,
              clickedIndex < spaceCount,
              clickedIndex != selectedIndex
        else {
            return nil
        }
        return clickedIndex
    }
}
#endif
