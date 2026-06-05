import CoreGraphics
import Foundation

#if os(macOS)
struct ShellSidebarSpaceSliderLayout: Equatable {
    static let maximumVisibleSpaces = 9
    static let spacing: CGFloat = 7
    static let minimumItemWidth: CGFloat = 8

    enum Density: Equatable {
        case low
        case medium
        case high
    }

    enum DisplayMode: Equatable {
        case fullTitle
        case shortTitle
        case indicator
    }

    struct Item: Equatable {
        let index: Int
        let mode: DisplayMode
        let width: CGFloat
        let visualScale: CGFloat
        let opacity: CGFloat
        let isSelected: Bool
        let isFocused: Bool
    }

    let density: Density
    let items: [Item]
    let contentWidth: CGFloat

    static func make(
        spaceCount rawSpaceCount: Int,
        selectedIndex rawSelectedIndex: Int?,
        hoveredIndex rawHoveredIndex: Int? = nil,
        scrubFocusIndex rawScrubFocusIndex: Int? = nil,
        availableWidth: CGFloat,
        reduceMotion: Bool
    ) -> ShellSidebarSpaceSliderLayout {
        let spaceCount = min(max(rawSpaceCount, 0), maximumVisibleSpaces)
        let density = density(for: spaceCount)
        guard spaceCount > 0 else {
            return ShellSidebarSpaceSliderLayout(density: density, items: [], contentWidth: 0)
        }

        let selectedIndex = clamped(rawSelectedIndex, count: spaceCount)
        let hoveredIndex = clamped(rawHoveredIndex, count: spaceCount)
        let scrubFocusIndex = clamped(rawScrubFocusIndex, count: spaceCount)
        let focusIndex = scrubFocusIndex ?? hoveredIndex
        let width = max(availableWidth, 0)
        let spacingWidth = CGFloat(max(spaceCount - 1, 0)) * spacing
        let itemBudget = max(width - spacingWidth, CGFloat(spaceCount) * minimumItemWidth)
        let modes = displayModes(
            density: density,
            spaceCount: spaceCount,
            selectedIndex: selectedIndex,
            hoveredIndex: hoveredIndex,
            scrubFocusIndex: scrubFocusIndex
        )
        let itemWidths = widths(
            modes: modes,
            selectedIndex: selectedIndex,
            focusIndex: focusIndex,
            itemBudget: itemBudget
        )

        let items = modes.enumerated().map { index, mode in
            let isFocused = focusIndex == index
            let distance = focusIndex.map { abs($0 - index) } ?? 0
            return Item(
                index: index,
                mode: mode,
                width: itemWidths[index],
                visualScale: visualScale(
                    distanceFromFocus: distance,
                    isFocused: isFocused,
                    hasFocus: focusIndex != nil,
                    reduceMotion: reduceMotion
                ),
                opacity: opacity(
                    distanceFromFocus: distance,
                    hasFocus: focusIndex != nil
                ),
                isSelected: selectedIndex == index,
                isFocused: isFocused
            )
        }

        let contentWidth = itemWidths.reduce(0, +) + spacingWidth
        return ShellSidebarSpaceSliderLayout(density: density, items: items, contentWidth: contentWidth)
    }

    static func density(for spaceCount: Int) -> Density {
        switch min(max(spaceCount, 0), maximumVisibleSpaces) {
        case 0...3:
            return .low
        case 4...6:
            return .medium
        default:
            return .high
        }
    }

    private static func displayModes(
        density: Density,
        spaceCount: Int,
        selectedIndex: Int?,
        hoveredIndex: Int?,
        scrubFocusIndex: Int?
    ) -> [DisplayMode] {
        (0..<spaceCount).map { index in
            switch density {
            case .low:
                return .fullTitle
            case .medium:
                return selectedIndex == index ? .fullTitle : .shortTitle
            case .high:
                if selectedIndex == index || scrubFocusIndex == index {
                    return .fullTitle
                }
                if hoveredIndex == index {
                    return .shortTitle
                }
                return .indicator
            }
        }
    }

    private static func widths(
        modes: [DisplayMode],
        selectedIndex: Int?,
        focusIndex: Int?,
        itemBudget: CGFloat
    ) -> [CGFloat] {
        let weights = modes.enumerated().map { index, mode -> CGFloat in
            switch mode {
            case .fullTitle:
                if selectedIndex == index {
                    return 3.4
                }
                if focusIndex == index {
                    return 3.1
                }
                return 2.6
            case .shortTitle:
                return focusIndex == index ? 2.1 : 1.65
            case .indicator:
                return focusIndex == index ? 1.0 : 0.72
            }
        }
        let totalWeight = max(weights.reduce(0, +), 1)
        return weights.map { max(minimumItemWidth, itemBudget * ($0 / totalWeight)) }
    }

    private static func visualScale(
        distanceFromFocus distance: Int,
        isFocused: Bool,
        hasFocus: Bool,
        reduceMotion: Bool
    ) -> CGFloat {
        guard hasFocus, !reduceMotion else { return 1 }
        if isFocused {
            return 1.035
        }
        if distance == 1 {
            return 1.012
        }
        return 1
    }

    private static func opacity(distanceFromFocus distance: Int, hasFocus: Bool) -> CGFloat {
        guard hasFocus else { return 1 }
        if distance <= 1 {
            return 1
        }
        if distance == 2 {
            return 0.82
        }
        return 0.68
    }

    private static func clamped(_ index: Int?, count: Int) -> Int? {
        guard let index, count > 0 else { return nil }
        return min(max(index, 0), count - 1)
    }

    func frame(for index: Int) -> CGRect? {
        var cursor: CGFloat = 0
        for item in items {
            let frame = CGRect(x: cursor, y: 0, width: item.width, height: 22)
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
                reset()
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
