import SwiftUI

#if os(macOS)
private struct ShellSidebarSpaceSliderScrollOffsetKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

/// Live descriptor for the in-progress Space being created. When present the
/// slider appends one trailing, selected, display-only target reflecting the
/// typed name/icon — no real Space exists until Create.
struct ShellSpaceSliderDraft: Equatable {
    let name: String
    let iconSystemName: String?
}

struct ShellSidebarSpaceSlider: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ObservedObject var host: ShellHostController
    let displaySpaceID: String?
    let previewedSpaceID: String?
    let activityFreshnessNow: Date
    let onForwardVerticalWheel: (ShellSidebarSpaceSliderWheelEvent) -> Bool
    var creationDraft: ShellSpaceSliderDraft? = nil
    static let draftTargetID = "space-creation-draft"
    @FocusState private var isKeyboardFocused: Bool
    @State private var hoveredSpaceID: String?
    @State private var scrubState: ShellSidebarSpaceSliderScrubState?
    @State private var wheelIntentState = ShellSidebarSpaceSliderWheelIntentState()
    @State private var wheelCommitToken = 0
    @State private var trackScrollOffsetX: CGFloat = 0
    @StateObject private var spaceCreationProfileOptions = ShellSpaceCreationProfileOptionStore()

    var body: some View {
        GeometryReader { proxy in
            let spaces = visibleSpaces
            let layout = sliderLayout(availableWidth: proxy.size.width)
            let trackWidth = max(proxy.size.width - (ShellSidebarMetrics.edgeInset * 2), 0)

            ScrollViewReader { scrollProxy in
                ZStack(alignment: .leading) {
                    RoundedRectangle(
                        cornerRadius: ShellSidebarSpaceSliderLayout.trackHeight * 0.5,
                        style: .continuous
                    )
                    .fill(ShellPalette.sidebarSpaceSliderTrack)
                    .overlay {
                        RoundedRectangle(
                            cornerRadius: ShellSidebarSpaceSliderLayout.trackHeight * 0.5,
                            style: .continuous
                        )
                        .stroke(ShellPalette.line.opacity(0.22), lineWidth: 0.6)
                    }

                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: ShellSidebarSpaceSliderLayout.spacing) {
                            ForEach(Array(spaces.enumerated()), id: \.element.spaceID) { index, space in
                                if let item = layout.items.first(where: { $0.index == index }) {
                                    spaceControl(for: space, item: item)
                                        .id(space.spaceID)
                                }
                            }
                            if let creationDraft,
                               let draftItem = layout.items.first(where: { $0.index == spaces.count })
                            {
                                draftControl(for: creationDraft, item: draftItem)
                                    .id(Self.draftTargetID)
                            }
                        }
                        .padding(.horizontal, 2)
                        .frame(
                            width: max(layout.contentWidth + 4, trackWidth),
                            height: ShellSidebarSpaceSliderLayout.trackHeight,
                            alignment: .leading
                        )
                        .background {
                            GeometryReader { contentProxy in
                                Color.clear.preference(
                                    key: ShellSidebarSpaceSliderScrollOffsetKey.self,
                                    value: max(
                                        0,
                                        -contentProxy.frame(in: .named("spaceSliderTrack")).minX
                                    )
                                )
                            }
                        }
                    }
                    .frame(width: trackWidth, height: ShellSidebarSpaceSliderLayout.trackHeight)
                    .clipShape(
                        RoundedRectangle(
                            cornerRadius: ShellSidebarSpaceSliderLayout.trackHeight * 0.5,
                            style: .continuous
                        )
                    )
                }
                .frame(width: trackWidth, height: ShellSidebarSpaceSliderLayout.trackHeight)
                .coordinateSpace(name: "spaceSliderTrack")
                .padding(.horizontal, ShellSidebarMetrics.edgeInset)
                .padding(.vertical, 2)
                .contentShape(Rectangle())
                .simultaneousGesture(dragScrubGesture(layout: layout))
                .background {
                    ShellSidebarSpaceSliderWheelMonitor(
                        onScroll: { event, deltaX, deltaY in
                            handleWheel(
                                event: event,
                                deltaX: deltaX,
                                deltaY: deltaY,
                                availableWidth: proxy.size.width
                            )
                        },
                        onReset: resetWheelIntent,
                        onContextMenuIntent: cancelScrubPreview
                    )
                }
                .onPreferenceChange(ShellSidebarSpaceSliderScrollOffsetKey.self) { offset in
                    trackScrollOffsetX = offset
                }
                .onChange(of: autoScrollSpaceID) { _, spaceID in
                    scrollSpaceIntoView(spaceID, scrollProxy: scrollProxy)
                }
                .onAppear {
                    scrollSpaceIntoView(autoScrollSpaceID, scrollProxy: scrollProxy, animated: false)
                }
            }
            .offset(x: reduceMotion ? 0 : scrubState?.edgeResistanceOffset ?? 0)
            .focusable()
            .focused($isKeyboardFocused)
            .focusEffectDisabled()
            .onMoveCommand(perform: handleMoveCommand)
            .onExitCommand {
                cancelScrubPreview()
            }
            .onKeyPress(.return) {
                commitKeyboardScrub()
                return .handled
            }
            .onKeyPress(.space) {
                commitKeyboardScrub()
                return .handled
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onChange(of: visibleSpaceIDs) { _, _ in
            cancelInvalidScrubIfNeeded()
        }
        .onChange(of: resolvedDisplaySpaceID) { _, _ in
            cancelScrubPreview()
        }
        .task {
            spaceCreationProfileOptions.refresh()
        }
        .onChange(of: creationDraft != nil) { _, creating in
            // Entering the creation form must cancel any pending scrub and
            // invalidate the scheduled wheel-commit so it cannot fire and move
            // the underlying Space behind the draft.
            if creating {
                cancelScrubPreview()
                spaceCreationProfileOptions.refresh()
            }
        }
    }

    private var visibleSpaces: [ShellSpace] {
        host.spaces
    }

    private func sliderLayout(availableWidth: CGFloat) -> ShellSidebarSpaceSliderLayout {
        // While creating a Space, append one trailing draft target that is
        // always the selected one; real-target hover/scrub focus is suppressed.
        if creationDraft != nil {
            return ShellSidebarSpaceSliderLayout.make(
                spaceCount: visibleSpaces.count + 1,
                selectedIndex: visibleSpaces.count,
                hoveredIndex: nil,
                scrubFocusIndex: nil,
                availableWidth: availableWidth - (ShellSidebarMetrics.edgeInset * 2),
                reduceMotion: reduceMotion
            )
        }

        let selectedIndex = visibleSpaces.firstIndex { $0.spaceID == resolvedDisplaySpaceID }
        let hoveredIndex = scrubState == nil
            ? visibleSpaces.firstIndex { $0.spaceID == hoveredSpaceID }
            : nil
        let previewedIndex = visibleSpaces.firstIndex { $0.spaceID == scrubFocusSpaceID }
        return ShellSidebarSpaceSliderLayout.make(
            spaceCount: visibleSpaces.count,
            selectedIndex: selectedIndex,
            hoveredIndex: hoveredIndex,
            scrubFocusIndex: previewedIndex,
            availableWidth: availableWidth - (ShellSidebarMetrics.edgeInset * 2),
            reduceMotion: reduceMotion
        )
    }

    private func spaceControl(for space: ShellSpace, item: ShellSidebarSpaceSliderLayout.Item) -> some View {
        Button {
            // Selection is frozen on the draft target during Space creation.
            guard creationDraft == nil else { return }
            guard let targetIndex = ShellSidebarSpaceSliderClickSelection.targetIndex(
                selectedIndex: selectedSpaceIndex,
                clickedIndex: item.index,
                spaceCount: visibleSpaces.count
            ) else {
                return
            }
            cancelScrubPreview()
            host.select(spaceID: visibleSpaces[targetIndex].spaceID)
        } label: {
            ShellSidebarSpaceTrackTarget(
                title: space.title,
                icon: ShellSpacePresentationIcon.resolve(
                    systemName: space.presentationIconSystemName,
                    title: space.title
                ),
                mode: item.mode,
                attention: strongestAttention(for: space),
                isSelected: item.isSelected,
                isFocused: item.isFocused,
                isHovered: hoveredSpaceID == space.spaceID
            )
        }
        .buttonStyle(.plain)
        .frame(width: item.width, height: ShellSidebarSpaceSliderLayout.itemHeight)
        .contentShape(Rectangle())
        .contextMenu {
            spaceContextMenu(for: space)
        }
        .help(space.title)
        .accessibilityLabel(spaceAccessibilityLabel(for: space, isSelected: item.isSelected))
        .accessibilityAddTraits(item.isSelected ? .isSelected : [])
        .onHover { isHovering in
            hoveredSpaceID = isHovering ? space.spaceID : nil
        }
    }

    /// Display-only trailing target for the in-progress draft Space. Mirrors the
    /// real `spaceControl` track target but attaches no tap/scrub/context
    /// affordances — selection cannot move to it and it commits nothing.
    private func draftControl(
        for draft: ShellSpaceSliderDraft,
        item: ShellSidebarSpaceSliderLayout.Item
    ) -> some View {
        ShellSidebarSpaceTrackTarget(
            title: draft.name.trimmingCharacters(in: .whitespacesAndNewlines),
            icon: ShellSpacePresentationIcon.resolve(
                systemName: draft.iconSystemName,
                title: draft.name
            ),
            mode: item.mode,
            attention: .idle,
            isSelected: item.isSelected,
            isFocused: item.isFocused,
            isHovered: false
        )
        .frame(width: item.width, height: ShellSidebarSpaceSliderLayout.itemHeight)
        .contentShape(Rectangle())
        .allowsHitTesting(false)
        .accessibilityLabel("New Space draft")
        .accessibilityAddTraits(.isSelected)
    }

    @ViewBuilder
    private func spaceContextMenu(for space: ShellSpace) -> some View {
        Menu("Terminal Profile") {
            Button {
                cancelScrubPreview()
                _ = host.setTerminalProfile(nil, forSpaceID: space.spaceID)
            } label: {
                Label("Login shell", systemImage: space.terminalProfileID == nil ? "checkmark" : "terminal")
            }

            ForEach(spaceCreationProfileOptions.options) { option in
                Button {
                    cancelScrubPreview()
                    _ = host.setTerminalProfile(option.id, forSpaceID: space.spaceID)
                } label: {
                    Label(
                        option.name,
                        systemImage: option.id == space.terminalProfileID
                            ? "checkmark"
                            : option.systemName
                    )
                }
                .disabled(!option.isEnabled)
                .help(option.guidance ?? option.name)
            }
        }

        Menu("Space Icon") {
            Button {
                cancelScrubPreview()
                _ = host.setPresentationIcon(nil, forSpaceID: space.spaceID)
            } label: {
                Label(
                    "Default (Initial)",
                    systemImage: space.presentationIconSystemName == nil ? "checkmark" : "textformat"
                )
            }

            Divider()

            ForEach(ShellSpaceIconCatalog.curatedSymbols, id: \.self) { symbol in
                Button {
                    cancelScrubPreview()
                    _ = host.setPresentationIcon(symbol, forSpaceID: space.spaceID)
                } label: {
                    Label(
                        iconMenuTitle(symbol),
                        systemImage: space.presentationIconSystemName == symbol ? "checkmark" : symbol
                    )
                }
            }
        }
    }

    private var resolvedDisplaySpaceID: String? {
        displaySpaceID ?? host.selectedSpace?.spaceID
    }

    private var selectedSpaceIndex: Int? {
        visibleSpaces.firstIndex { $0.spaceID == resolvedDisplaySpaceID }
    }

    private var scrubFocusSpaceID: String? {
        if let scrubState,
           visibleSpaces.indices.contains(scrubState.focusIndex)
        {
            return visibleSpaces[scrubState.focusIndex].spaceID
        }
        return previewedSpaceID
    }

    private var autoScrollSpaceID: String? {
        // While creating, keep the trailing draft target on-screen so its live
        // identity preview stays visible even when the slider is scrollable.
        if creationDraft != nil { return Self.draftTargetID }
        return scrubFocusSpaceID ?? resolvedDisplaySpaceID
    }

    private var visibleSpaceIDs: [String] {
        visibleSpaces.map(\.spaceID)
    }

    private func dragScrubGesture(layout: ShellSidebarSpaceSliderLayout) -> some Gesture {
        DragGesture(
            minimumDistance: ShellSidebarSpaceSliderScrubState.dragThreshold,
            coordinateSpace: .local
        )
        .onChanged { value in
            updateDragScrub(value, layout: layout)
        }
        .onEnded { value in
            updateDragScrub(value, layout: layout)
            commitScrubSelection()
        }
    }

    private func updateDragScrub(
        _ value: DragGesture.Value,
        layout: ShellSidebarSpaceSliderLayout
    ) {
        guard var nextState = scrubStateForUpdating(source: .drag) else { return }
        nextState.updateDrag(
            locationX: value.location.x - ShellSidebarMetrics.edgeInset + trackScrollOffsetX,
            translationX: value.translation.width,
            layout: layout
        )
        scrubState = nextState
    }

    private func handleWheel(
        event: ShellSidebarSpaceSliderWheelEvent,
        deltaX: CGFloat,
        deltaY: CGFloat,
        availableWidth: CGFloat
    ) -> Bool {
        var nextIntent = wheelIntentState
        let route = nextIntent.route(deltaX: deltaX, deltaY: deltaY)
        wheelIntentState = nextIntent

        switch route {
        case .passThrough:
            guard ShellSidebarSpaceSliderWheelForwarding.shouldForwardPassThroughToTabList(
                deltaX: deltaX,
                deltaY: deltaY
            ) else {
                return false
            }
            return onForwardVerticalWheel(event)
        case .scrub(let routedDeltaX):
            updateWheelScrub(deltaX: routedDeltaX, availableWidth: availableWidth)
            scheduleWheelCommit()
            return true
        }
    }

    private func updateWheelScrub(deltaX: CGFloat, availableWidth: CGFloat) {
        guard var nextState = scrubStateForUpdating(source: .wheel) else { return }
        let itemSpan = max(
            (availableWidth - ShellSidebarMetrics.edgeInset * 2)
                / CGFloat(max(visibleSpaces.count, 1)),
            ShellSidebarSpaceSliderScrubState.wheelStepWidth
        )
        nextState.updateWheel(
            deltaX: deltaX,
            itemSpan: itemSpan,
            spaceCount: visibleSpaces.count
        )
        scrubState = nextState
    }

    private func handleMoveCommand(_ direction: MoveCommandDirection) {
        let delta: Int
        switch direction {
        case .left:
            delta = -1
        case .right:
            delta = 1
        default:
            return
        }

        guard var nextState = scrubStateForUpdating(source: .keyboard) else { return }
        nextState.moveFocus(by: delta, spaceCount: visibleSpaces.count)
        scrubState = nextState
    }

    private func scrubStateForUpdating(
        source: ShellSidebarSpaceSliderScrubSource
    ) -> ShellSidebarSpaceSliderScrubState? {
        // No scrub/keyboard/wheel selection while a draft target owns selection.
        guard creationDraft == nil else { return nil }
        if let scrubState, scrubState.source == source {
            return scrubState
        }
        return ShellSidebarSpaceSliderScrubState(
            source: source,
            selectedIndex: selectedSpaceIndex,
            spaceCount: visibleSpaces.count
        )
    }

    private func scheduleWheelCommit() {
        wheelCommitToken += 1
        let token = wheelCommitToken
        DispatchQueue.main.asyncAfter(
            deadline: .now() + ShellSidebarSpaceSliderScrubState.wheelCommitDelay
        ) {
            guard wheelCommitToken == token,
                  scrubState?.source == .wheel
            else {
                return
            }
            commitScrubSelection()
            resetWheelIntent()
        }
    }

    private func scrollSpaceIntoView(
        _ spaceID: String?,
        scrollProxy: ScrollViewProxy,
        animated: Bool = true
    ) {
        guard let spaceID else { return }
        let action = {
            scrollProxy.scrollTo(spaceID, anchor: .center)
        }
        if animated, !reduceMotion {
            withAnimation(.easeOut(duration: 0.14), action)
        } else {
            action()
        }
    }

    private func commitKeyboardScrub() {
        guard scrubState?.source == .keyboard else { return }
        commitScrubSelection()
    }

    private func commitScrubSelection() {
        // Chokepoint for every scrub commit (wheel timer, drag-end, keyboard).
        // While a draft owns selection, a stale in-flight commit must not move
        // the underlying Space — drop it and clear any lingering scrub state.
        guard creationDraft == nil else {
            cancelScrubPreview()
            return
        }
        guard let scrubState,
              visibleSpaces.indices.contains(scrubState.commitIndex)
        else {
            cancelScrubPreview()
            return
        }

        let targetSpace = visibleSpaces[scrubState.commitIndex]
        cancelScrubPreview()
        if targetSpace.spaceID != resolvedDisplaySpaceID {
            host.select(spaceID: targetSpace.spaceID)
        }
    }

    private func cancelScrubPreview() {
        scrubState = nil
        wheelCommitToken += 1
        resetWheelIntent()
    }

    private func resetWheelIntent() {
        wheelIntentState.reset()
    }

    private func cancelInvalidScrubIfNeeded() {
        guard let scrubState else { return }
        guard visibleSpaces.indices.contains(scrubState.focusIndex),
              visibleSpaces.indices.contains(scrubState.sourceIndex)
        else {
            cancelScrubPreview()
            return
        }
    }

    /// Readable label for a curated Space-icon SF Symbol. Falls back to a
    /// title-cased transform of the raw symbol name for any unmapped symbol.
    private func iconMenuTitle(_ symbol: String) -> String {
        switch symbol {
        case "terminal": return "Terminal"
        case "chevron.left.forwardslash.chevron.right": return "Code"
        case "hammer": return "Build"
        case "wrench.and.screwdriver": return "Tools"
        case "ant": return "Debug"
        case "flask": return "Experiment"
        case "cube.box": return "Package"
        case "shippingbox": return "Release"
        case "server.rack": return "Server"
        case "externaldrive": return "Storage"
        case "doc.text": return "Document"
        case "book": return "Docs"
        case "paintbrush": return "Design"
        case "paintpalette": return "Palette"
        case "globe": return "Web"
        case "network": return "Network"
        case "lock": return "Secure"
        case "key": return "Keys"
        case "leaf": return "Nature"
        case "bolt": return "Power"
        case "sparkles": return "Sparkle"
        case "star": return "Star"
        case "flag": return "Flag"
        case "folder": return "Folder"
        default:
            return symbol
                .split(separator: ".")
                .map(\.capitalized)
                .joined(separator: " ")
        }
    }

    private func strongestAttention(for space: ShellSpace) -> ShellAttentionState {
        host.shellState.panes
            .filter { $0.spaceID == space.spaceID }
            .map { shellEffectiveAttention(for: $0, now: activityFreshnessNow) }
            .filter(\.requiresUserAction)
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .awaitingUser:
            return 3
        case .notable:
            return 2
        case .active:
            return 1
        case .idle:
            return 0
        }
    }

    private func spaceAccessibilityLabel(for space: ShellSpace, isSelected: Bool) -> String {
        var parts = [space.title, space.tabs.count == 1 ? "1 tab" : "\(space.tabs.count) tabs"]
        if isSelected {
            parts.append("selected")
        }
        if previewedSpaceID == space.spaceID {
            parts.append("preview")
        }
        if strongestAttention(for: space) != .idle {
            parts.append("needs attention")
        }
        return parts.joined(separator: ", ")
    }
}

private struct ShellSidebarSpaceTrackTarget: View {
    let title: String
    let icon: ShellSpacePresentationIcon.Resolved
    let mode: ShellSidebarSpaceSliderLayout.DisplayMode
    let attention: ShellAttentionState
    let isSelected: Bool
    let isFocused: Bool
    let isHovered: Bool

    var body: some View {
        HStack(spacing: textSpacing) {
            spaceIcon
                .frame(width: 15, height: 15)

            if mode != .iconOnly {
                Text(title)
                    .font(font)
                    .foregroundStyle(textForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, horizontalPadding)
        .frame(
            maxWidth: .infinity,
            minHeight: ShellSidebarSpaceSliderLayout.itemHeight,
            alignment: .center
        )
        .background {
            if isSelected {
                ShellLiquidGlassSurface(
                    shape: Capsule(),
                    tint: ShellPalette.sidebarSelection,
                    tintOpacity: 0.30,
                    strokeOpacity: 0.24,
                    usesSystemGlassInLightMode: true
                )
            } else if isFocused || isHovered {
                Capsule()
                    .strokeBorder(focusStroke, lineWidth: 0.7)
            }
        }
    }

    @ViewBuilder
    private var spaceIcon: some View {
        switch icon {
        case .symbol(let name), .fallbackSymbol(let name):
            Image(systemName: name)
                .font(.system(size: iconSize, weight: .semibold))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(iconForeground)
                .accessibilityHidden(true)
        case .monogram(let text):
            Text(text)
                .font(ShellType.pro(iconSize, weight: .semibold))
                .foregroundStyle(iconForeground)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
                .accessibilityHidden(true)
        }
    }

    private var font: Font {
        switch mode {
        case .fullTitle:
            return .system(size: 12, weight: isSelected ? .semibold : .medium)
        case .truncatedTitle:
            return .system(size: 11.6, weight: isSelected || isFocused ? .semibold : .medium)
        case .iconOnly:
            return .system(size: 11.4, weight: .medium)
        }
    }

    private var horizontalPadding: CGFloat {
        switch mode {
        case .fullTitle:
            return 8
        case .truncatedTitle:
            return 6
        case .iconOnly:
            return 0
        }
    }

    private var textSpacing: CGFloat {
        switch mode {
        case .fullTitle:
            return 5
        case .truncatedTitle:
            return 4
        case .iconOnly:
            return 0
        }
    }

    private var iconSize: CGFloat {
        switch mode {
        case .fullTitle, .truncatedTitle:
            return 11.4
        case .iconOnly:
            return 12.4
        }
    }

    private var textForeground: Color {
        if isSelected {
            return ShellPalette.sidebarInk.opacity(0.92)
        }
        if isFocused || isHovered {
            return ShellPalette.sidebarInk.opacity(0.82)
        }
        return ShellPalette.sidebarMutedInk.opacity(0.72)
    }

    private var iconForeground: Color {
        if attention.requiresUserAction {
            return ShellSignal.action.opacity(isSelected ? 0.94 : 0.86)
        }
        if isSelected {
            return ShellPalette.sidebarInk.opacity(0.92)
        }
        if isFocused {
            return ShellPalette.accent.opacity(0.82)
        }
        if isHovered {
            return ShellPalette.sidebarInk.opacity(0.78)
        }
        return ShellPalette.sidebarMutedInk.opacity(0.62)
    }

    private var focusStroke: Color {
        if isFocused {
            return ShellPalette.accent.opacity(0.34)
        }
        return ShellPalette.sidebarInk.opacity(0.14)
    }
}
#endif
