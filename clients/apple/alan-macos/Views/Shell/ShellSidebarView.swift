import SwiftUI
import UniformTypeIdentifiers

#if os(macOS)
struct ShellSidebarView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ObservedObject var host: ShellHostController
    let chromeMetrics: ShellWindowChromeMetrics
    let displaySpaceID: String?
    let isSwipeEnabled: Bool
    let openCommandTab: () -> Void
    @State private var spacePager: ShellSidebarSpaceContentPagerState?
    @State private var spacePagerToken = 0
    @State private var spacePagerPageWidth: CGFloat = 1
    @State private var hoveredTabID: String?
    @State private var hoveredSpaceID: String?
    @State private var isCommandLauncherHovered = false
    @State private var tabListScrollOffsetY: CGFloat = 0
    @State private var activityFreshnessNow = Date()
    @State private var activeTabDrag: ShellSidebarTabDragState?
    @State private var tabInsertionPreview: ShellSidebarTabInsertionTarget?

    init(
        host: ShellHostController,
        chromeMetrics: ShellWindowChromeMetrics,
        displaySpaceID: String?,
        previewedSpaceID: String? = nil,
        isSpaceSwipeGestureLocked: Bool = false,
        isSwipeEnabled: Bool,
        onSpaceSwipe: @escaping (ShellSidebarSwipeUpdate) -> Void = { _ in },
        openCommandTab: @escaping () -> Void
    ) {
        self.host = host
        self.chromeMetrics = chromeMetrics
        self.displaySpaceID = displaySpaceID
        self.isSwipeEnabled = isSwipeEnabled
        self.openCommandTab = openCommandTab
        _ = previewedSpaceID
        _ = isSpaceSwipeGestureLocked
        _ = onSpaceSwipe
    }

    var body: some View {
        sidebarContent
        .background {
            if isSwipeEnabled {
                ShellSidebarSwipeMonitor(onUpdate: handleSpaceSwipe)
            }
        }
        .scrollDisabled(isTabListScrollDisabled)
        .onChange(of: sourceSpaceID) { _, _ in
            tabListScrollOffsetY = 0
        }
        .task(id: activityFreshnessRefreshID) {
            await scheduleActivityFreshnessRefresh()
        }
    }

    private var sidebarContent: some View {
        VStack(alignment: .leading, spacing: 0) {
            commandLauncher
                .padding(.horizontal, ShellSidebarMetrics.edgeInset)
                .padding(.bottom, 10)
            spaceContentPager
            spaceDock
                .padding(.horizontal, ShellSidebarMetrics.edgeInset)
                .padding(.top, 10)
        }
        .padding(.top, chromeMetrics.commandLauncherTopInset)
        .padding(.bottom, ShellSidebarMetrics.spaceDockOuterBottomInset)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var spaceContentPager: some View {
        GeometryReader { proxy in
            let pageWidth = max(proxy.size.width, 1)
            ZStack(alignment: .leading) {
                ForEach(spacePageIndices, id: \.self) { index in
                    VStack(alignment: .leading, spacing: 0) {
                        spaceLabelRow(for: spaceID(forSpaceAt: index))
                            .padding(.bottom, 2)
                        tabSection(for: spaceID(forSpaceAt: index))
                    }
                    .frame(width: pageWidth, height: proxy.size.height, alignment: .topLeading)
                    .offset(x: spacePageOffset(for: index, pageWidth: pageWidth))
                    .allowsHitTesting(spacePager == nil && index == selectedSpaceIndex)
                }
            }
            .clipped()
            .onAppear {
                updateSpacePagerPageWidth(pageWidth)
            }
            .onChange(of: proxy.size.width) { _, width in
                updateSpacePagerPageWidth(max(width, 1))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func spaceLabelRow(for spaceID: String?) -> some View {
        ShellSidebarSpaceHeader(
            host: host,
            spaceID: spaceID
        )
        .frame(maxWidth: .infinity)
        .frame(height: 28)
    }

    private var commandLauncher: some View {
        Button(action: openCommandTab) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: ShellSidebarMetrics.iconPointSize, weight: .semibold))
                    .foregroundStyle(commandLauncherForeground)
                    .frame(width: ShellSidebarMetrics.iconColumnWidth)
                Text("Ask alan...")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(commandLauncherForeground)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ShellSidebarMetrics.rowInset)
            .frame(maxWidth: .infinity, minHeight: 34, alignment: .leading)
            .background {
                ShellLiquidGlassSurface(
                    shape: Capsule(),
                    tint: ShellPalette.commandGlassTint,
                    tintOpacity: isCommandLauncherHovered ? 0.22 : 0.18,
                    strokeOpacity: isCommandLauncherHovered ? 0.22 : 0.16
                )
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { isHovering in
            isCommandLauncherHovered = isHovering
        }
        .help("Ask alan, Command-P")
        .accessibilityLabel("Ask alan")
    }

    private var commandLauncherForeground: Color {
        ShellPalette.sidebarMutedInk.opacity(isCommandLauncherHovered ? 0.92 : 0.80)
    }

    private func tabSection(for spaceID: String?) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            tabListPage(for: spaceID)
                .overlay(alignment: .top) {
                    if spaceID == sourceSpaceID {
                        ShellSidebarScrollBoundary(progress: tabListBoundaryProgress)
                    }
                }
                .clipped()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func tabListPage(for spaceID: String?) -> some View {
        ScrollView(.vertical, showsIndicators: false) {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: ShellSidebarTabListOffsetPreferenceKey.self,
                    value: proxy.frame(in: .named(tabListCoordinateSpaceName(for: spaceID))).minY
                )
            }
            .frame(height: 0)

            VStack(alignment: .leading, spacing: 0) {
                if let space = space(for: spaceID) {
                    tabOrganizationSections(for: space)
                    ShellCompactEmptyAction(
                        title: "New Tab",
                        systemImage: "plus",
                        action: {
                            host.performShellAction(
                                .newTerminalTab,
                                target: .contextSpace(space.spaceID)
                            )
                        }
                    )
                    .help("Create a tab in this space")
                } else {
                    ShellCompactEmptyAction(
                        title: "New Space",
                        systemImage: "plus",
                        action: {
                            _ = host.createTerminalSpace()
                        }
                    )
                    .help("Create a space")
                }
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .padding(.top, 2)
            .padding(.horizontal, ShellSidebarMetrics.edgeInset)
        }
        .coordinateSpace(name: tabListCoordinateSpaceName(for: spaceID))
        .onPreferenceChange(ShellSidebarTabListOffsetPreferenceKey.self) { offsetY in
            guard spaceID == sourceSpaceID else { return }
            tabListScrollOffsetY = offsetY
        }
    }

    @ViewBuilder
    private func tabOrganizationSections(for space: ShellSpace) -> some View {
        let pinnedTabs = space.pinnedTabs
        let unpinnedTabs = space.unpinnedTabs

        if !pinnedTabs.isEmpty {
            tabRows(
                pinnedTabs,
                in: space,
                section: .pinned
            )
        }

        if !pinnedTabs.isEmpty && !unpinnedTabs.isEmpty {
            ShellSidebarTabSectionDivider()
                .padding(.vertical, 4)
        }

        tabRows(
            unpinnedTabs,
            in: space,
            section: .unpinned
        )
    }

    @ViewBuilder
    private func tabRows(
        _ tabs: [ShellTab],
        in space: ShellSpace,
        section: ShellTabOrganizationSection
    ) -> some View {
        ForEach(Array(tabs.enumerated()), id: \.element.id) { index, tab in
            insertionPreviewLine(
                spaceID: space.spaceID,
                section: section,
                index: index
            )
            tabListRow(for: tab, in: space, section: section, index: index)
        }

        insertionPreviewLine(
            spaceID: space.spaceID,
            section: section,
            index: tabs.count
        )
        .frame(height: tabs.isEmpty ? 8 : 4)
        .onDrop(
            of: [.plainText],
            delegate: ShellSidebarTabDropDelegate(
                target: ShellSidebarTabInsertionTarget(
                    spaceID: space.spaceID,
                    section: section,
                    index: tabs.count
                ),
                activeDrag: $activeTabDrag,
                preview: $tabInsertionPreview,
                host: host
            )
        )
    }

    @ViewBuilder
    private func insertionPreviewLine(
        spaceID: String,
        section: ShellTabOrganizationSection,
        index: Int
    ) -> some View {
        let target = ShellSidebarTabInsertionTarget(
            spaceID: spaceID,
            section: section,
            index: index
        )
        ShellSidebarTabInsertionLine(
            isVisible: tabInsertionPreview == target
        )
    }

    private var spaceDock: some View {
        HStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) {
                    ForEach(host.spaces) { space in
                        Button {
                            host.select(spaceID: space.spaceID)
                        } label: {
                            ShellSpaceSwitcherItem(
                                title: space.title,
                                symbolName: spaceSymbol(for: space),
                                attention: strongestAttention(for: space),
                                tabCount: space.tabs.count,
                                isSelected: host.selectedSpace?.spaceID == space.spaceID,
                                isPreviewed: previewedSpaceID == space.spaceID,
                                isHovered: hoveredSpaceID == space.spaceID
                            )
                        }
                        .buttonStyle(.plain)
                        .onHover { isHovering in
                            hoveredSpaceID = isHovering ? space.spaceID : nil
                        }
                    }
                }
                .padding(.vertical, ShellSidebarMetrics.spaceDockInternalVerticalPadding)
            }

            Button(action: createSpaceFromDock) {
                Image(systemName: "plus")
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(ShellPalette.sidebarInk.opacity(0.76))
                    .frame(width: 30, height: 30)
                    .background {
                        if hoveredSpaceID == "__new_space__" {
                            ShellMaterialShape(
                                role: .controlGlassHover,
                                shape: RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                            )
                        }
                    }
            }
            .buttonStyle(.plain)
            .help("Create a new space")
            .accessibilityLabel("Create space")
            .onHover { isHovering in
                hoveredSpaceID = isHovering ? "__new_space__" : nil
            }
        }
    }

    private func createSpaceFromDock() {
        _ = host.createTerminalSpace()
    }

    private func handleSpaceSwipe(_ update: ShellSidebarSwipeUpdate) {
        switch update.phase {
        case .began:
            guard spacePager?.isSettling != true else { return }
            beginSpacePager()
        case .changed:
            guard spacePager?.isSettling != true else { return }
            updateSpacePager(translationX: update.translationX)
        case .ended:
            finishSpacePager(velocityX: update.velocityX)
        case .cancelled:
            settleSpacePager(committing: false)
        }
    }

    private func beginSpacePager() {
        guard let sourceIndex = selectedSpaceIndex else { return }
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            spacePager = ShellSidebarSpaceContentPagerState(
                sourceIndex: sourceIndex,
                targetIndex: nil,
                dragOffset: 0,
                pageWidth: sidebarSwipePageWidth,
                settlementPhase: .dragging
            )
        }
    }

    private func updateSpacePager(translationX: CGFloat) {
        guard abs(translationX) > 0.5 else { return }
        guard let sourceIndex = spacePager?.sourceIndex ?? selectedSpaceIndex else { return }
        let clampedTranslationX = ShellSidebarSpaceContentPagerState.clampedDragOffset(
            for: translationX,
            pageWidth: sidebarSwipePageWidth
        )
        let direction = clampedTranslationX < 0 ? 1 : -1
        let targetIndex = adjacentSpaceIndex(from: sourceIndex, direction: direction)
        let dragOffset =
            targetIndex == nil ? resistedEdgeOffset(for: clampedTranslationX) : clampedTranslationX

        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            spacePager = ShellSidebarSpaceContentPagerState(
                sourceIndex: sourceIndex,
                targetIndex: targetIndex,
                dragOffset: dragOffset,
                pageWidth: sidebarSwipePageWidth,
                settlementPhase: .dragging
            )
        }
    }

    private func finishSpacePager(velocityX: CGFloat) {
        guard let pager = spacePager else { return }
        guard pager.targetIndex != nil else {
            settleSpacePager(committing: false)
            return
        }

        let velocityDirection = velocityX < 0 ? 1 : -1
        let fastEnough = abs(velocityX) >= 120 && velocityDirection == pager.direction
        let farEnough = pager.progress >= 0.28
        settleSpacePager(committing: farEnough || fastEnough)
    }

    private func settleSpacePager(committing: Bool) {
        guard var pager = spacePager else { return }
        let targetIndex = pager.targetIndex
        if committing,
           let targetIndex,
           host.spaces.indices.contains(targetIndex)
        {
            host.select(spaceID: host.spaces[targetIndex].spaceID)
        }

        pager.settlementPhase = committing ? .settlingToTarget : .settlingToSource
        pager.pageWidth = sidebarSwipePageWidth
        pager.dragOffset = committing ? -CGFloat(pager.direction) * sidebarSwipePageWidth : 0
        spacePagerToken += 1
        let token = spacePagerToken
        let duration = reduceMotion ? 0.12 : 0.28

        withAnimation(settleAnimation) {
            spacePager = pager
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + duration) {
            guard spacePagerToken == token else { return }
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) {
                spacePager = nil
            }
        }
    }

    private var settleAnimation: Animation {
        if reduceMotion {
            return .easeOut(duration: 0.12)
        }
        return .interactiveSpring(response: 0.28, dampingFraction: 0.86, blendDuration: 0.04)
    }

    private var sidebarSwipePageWidth: CGFloat {
        max(spacePagerPageWidth, 1)
    }

    private func resistedEdgeOffset(for translationX: CGFloat) -> CGFloat {
        let edgeLimit = sidebarSwipePageWidth * 0.18
        let distance = abs(translationX)
        let resistedDistance = edgeLimit * distance / (distance + edgeLimit)
        return translationX < 0 ? -resistedDistance : resistedDistance
    }

    private func adjacentSpaceIndex(from sourceIndex: Int, direction: Int) -> Int? {
        let targetIndex = sourceIndex + direction
        guard host.spaces.indices.contains(targetIndex) else { return nil }
        return targetIndex
    }

    private var selectedSpaceIndex: Int? {
        guard let selectedSpaceID = host.selectedSpace?.spaceID else { return nil }
        return host.spaces.firstIndex { $0.spaceID == selectedSpaceID }
    }

    private var previewedSpaceID: String? {
        guard let targetIndex = spacePager?.targetIndex else { return nil }
        return spaceID(forSpaceAt: targetIndex)
    }

    private func spaceID(forSpaceAt index: Int) -> String? {
        guard host.spaces.indices.contains(index) else { return nil }
        return host.spaces[index].spaceID
    }

    private var isTabListScrollDisabled: Bool {
        spacePager != nil
    }

    private var spacePageIndices: [Int] {
        guard let spacePager else {
            return selectedSpaceIndex.map { [$0] } ?? []
        }
        return spacePager.pageIndicesForRendering(validRange: host.spaces.indices)
    }

    private func spacePageOffset(for index: Int, pageWidth: CGFloat) -> CGFloat {
        guard var spacePager else { return 0 }
        spacePager.pageWidth = pageWidth
        return spacePager.offset(for: index)
    }

    private func updateSpacePagerPageWidth(_ pageWidth: CGFloat) {
        let clampedPageWidth = max(pageWidth, 1)
        spacePagerPageWidth = clampedPageWidth
        guard var spacePager,
              spacePager.pageWidth != clampedPageWidth
        else {
            return
        }
        spacePager.pageWidth = clampedPageWidth
        self.spacePager = spacePager
    }

    private var sourceSpaceID: String? {
        displaySpaceID ?? host.selectedSpace?.spaceID
    }

    private var tabListBoundaryProgress: CGFloat {
        min(max(-tabListScrollOffsetY / 18, 0), 1)
    }

    private func tabListCoordinateSpaceName(for spaceID: String?) -> String {
        "ShellSidebarTabListScroll-\(spaceID ?? "none")"
    }

    private var activityFreshnessRefreshID: String {
        nextActivityFreshnessExpiry(after: activityFreshnessNow)
            .map { "\($0.timeIntervalSince1970)" } ?? "none"
    }

    private func scheduleActivityFreshnessRefresh() async {
        guard let deadline = nextActivityFreshnessExpiry(after: activityFreshnessNow) else {
            return
        }

        let delay = min(max(deadline.timeIntervalSinceNow, 0), 86_400)
        if delay > 0 {
            let nanoseconds = UInt64(delay * 1_000_000_000)
            try? await Task.sleep(nanoseconds: nanoseconds)
        }

        guard !Task.isCancelled else { return }
        await MainActor.run {
            activityFreshnessNow = Date()
        }
    }

    private func nextActivityFreshnessExpiry(after now: Date) -> Date? {
        host.shellState.panes.compactMap { pane in
            guard let activity = pane.activity else { return nil }
            return nextActivityFreshnessExpiry(for: activity, after: now)
        }
        .min()
    }

    private func nextActivityFreshnessExpiry(
        for activity: TerminalActivitySnapshot,
        after now: Date
    ) -> Date? {
        [
            activity.freshness.staleAt,
            activity.freshness.expiresAt,
        ]
        .compactMap { value in
            value.flatMap(Self.activityFreshnessFormatter.date(from:))
        }
        .filter { $0 > now }
        .min()
    }

    private static let activityFreshnessFormatter = ISO8601DateFormatter()

    private func space(for spaceID: String?) -> ShellSpace? {
        guard let spaceID else { return host.selectedSpace }
        return host.spaces.first { $0.spaceID == spaceID }
    }

    private func close(tab: ShellTab) {
        host.performShellAction(.tabClose, target: .contextTab(tab.tabID))
    }

    private func focusPane(_ paneID: String, in tab: ShellTab) {
        host.select(tabID: tab.tabID)
        _ = host.performShellAutomationCommand(.focusPane(paneID: paneID))
        host.refocusSelectedTerminalPane()
    }

    private func focusNextSplitPane(in tab: ShellTab, summary: ShellTabPaneSummary) {
        guard let paneID = summary.nextPaneID(after: host.shellState.focusedPaneID) else { return }
        focusPane(paneID, in: tab)
    }

    @ViewBuilder
    private func tabListRow(
        for tab: ShellTab,
        in space: ShellSpace,
        section: ShellTabOrganizationSection,
        index: Int
    ) -> some View {
        let isSelected = host.selectedTab?.tabID == tab.tabID
        let isHovered = hoveredTabID == tab.tabID
        let contentState = host.shellState.contentStateProjection()
        let projection = shellSidebarTabProjection(
            for: tab,
            panes: host.shellState.panes,
            contentState: contentState,
            focusedPaneID: host.shellState.focusedPaneID,
            focusedTabID: host.selectedTab?.tabID,
            now: activityFreshnessNow
        )

        ShellTabSidebarRow(
            title: projection.title,
            subtitle: projection.secondaryLine,
            isActivitySubtitle: projection.activity != nil,
            progress: projection.progress,
            attention: strongestAttention(for: tab),
            showsAlanMarker: showsAlanMarker(for: tab, activity: projection.activity),
            paneSummary: paneSummary(for: tab),
            isPinned: host.isTabPinned(tabID: tab.tabID),
            isSelected: isSelected,
            isHovered: isHovered,
            showsCloseAffordance: isHovered,
            onFocusSplitPane: { paneID in
                focusPane(paneID, in: tab)
            },
            onFocusNextSplitPane: { summary in
                focusNextSplitPane(in: tab, summary: summary)
            },
            onClose: { close(tab: tab) }
        )
        .contentShape(Rectangle())
        .onTapGesture {
            host.select(tabID: tab.tabID)
        }
        .onHover { isHovering in
            hoveredTabID = isHovering ? tab.tabID : nil
        }
        .simultaneousGesture(
            DragGesture(minimumDistance: ShellSidebarTabDragState.dragThreshold)
                .onChanged { _ in
                    beginTabDragIfNeeded(tab: tab, space: space, section: section, index: index)
                }
                .onEnded { _ in
                    scheduleTabDragCleanup()
                }
        )
        .onDrag {
            beginTabDragIfNeeded(tab: tab, space: space, section: section, index: index)
            return NSItemProvider(object: tab.tabID as NSString)
        }
        .onDrop(
            of: [.plainText],
            delegate: ShellSidebarTabDropDelegate(
                target: ShellSidebarTabInsertionTarget(
                    spaceID: space.spaceID,
                    section: section,
                    index: index
                ),
                activeDrag: $activeTabDrag,
                preview: $tabInsertionPreview,
                host: host
            )
        )
        .contextMenu {
            Button(host.shellActionTitle(.newTerminalTab)) {
                host.performShellAction(.newTerminalTab, target: .contextSpace(space.spaceID))
            }
            Button(host.shellActionTitle(.newAlanTab)) {
                host.performShellAction(.newAlanTab, target: .contextSpace(space.spaceID))
            }
            Divider()
            if host.isTabPinned(tabID: tab.tabID) {
                Button(host.shellActionTitle(.tabUpdatePin)) {
                    host.performShellAction(.tabUpdatePin, target: .contextTab(tab.tabID))
                }
                .disabled(!host.shellActionAvailability(.tabUpdatePin, target: .contextTab(tab.tabID)).isAvailable)

                Button(host.shellActionTitle(.tabUnpin)) {
                    host.performShellAction(.tabUnpin, target: .contextTab(tab.tabID))
                }
                .disabled(!host.shellActionAvailability(.tabUnpin, target: .contextTab(tab.tabID)).isAvailable)
            } else {
                Button(host.shellActionTitle(.tabPin)) {
                    host.performShellAction(.tabPin, target: .contextTab(tab.tabID))
                }
                .disabled(!host.shellActionAvailability(.tabPin, target: .contextTab(tab.tabID)).isAvailable)
            }
            if host.spaces.count > 1 {
                Menu(host.shellActionTitle(.tabMoveToSpace)) {
                    ForEach(host.spaces.filter { $0.spaceID != space.spaceID }) { targetSpace in
                        Button(targetSpace.title) {
                            host.performShellAction(
                                .tabMoveToSpace,
                                target: .tabToSpace(
                                    tabID: tab.tabID,
                                    spaceID: targetSpace.spaceID
                                )
                            )
                        }
                        .disabled(
                            !host.shellActionAvailability(
                                .tabMoveToSpace,
                                target: .tabToSpace(
                                    tabID: tab.tabID,
                                    spaceID: targetSpace.spaceID
                                )
                            ).isAvailable
                        )
                    }
                }
            }
            Divider()
            Button(host.shellActionTitle(.tabClose), role: .destructive) {
                close(tab: tab)
            }
            .disabled(!host.shellActionAvailability(.tabClose, target: .contextTab(tab.tabID)).isAvailable)
        }
    }

    private func beginTabDragIfNeeded(
        tab: ShellTab,
        space: ShellSpace,
        section: ShellTabOrganizationSection,
        index: Int
    ) {
        let nextDrag = ShellSidebarTabDragState(
            tabID: tab.tabID,
            sourceSpaceID: space.spaceID,
            sourceSection: section,
            sourceIndex: index
        )
        if activeTabDrag != nextDrag {
            activeTabDrag = nextDrag
        }
    }

    private func clearTabDragPreview() {
        activeTabDrag = nil
        tabInsertionPreview = nil
    }

    private func scheduleTabDragCleanup() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
            guard tabInsertionPreview == nil else { return }
            activeTabDrag = nil
        }
    }

    private func paneSummary(for tab: ShellTab) -> ShellTabPaneSummary? {
        let contentState = host.shellState.contentStateProjection()
        let paneIDs = tab.paneTree.paneIDs.filter { paneID in
            contentState.paneSlot(paneSlotID: paneID)?.tabID == tab.tabID
        }
        guard !paneIDs.isEmpty else { return nil }

        return ShellTabPaneSummary(
            paneTree: tab.paneTree,
            visiblePaneIDs: paneIDs,
            focusedPaneID: host.shellState.focusedPaneID
        )
    }

    private func fallbackTitle(for tab: ShellTab) -> String {
        switch tab.kind {
        case .terminal:
            return "Terminal"
        case .scratch:
            return "Scratch"
        case .log:
            return "Logs"
        }
    }

    private func tabTitle(for tab: ShellTab) -> String {
        if let content = host.shellState.contentStateProjection().primaryContent(in: tab.tabID),
           content.kind != .terminal
        {
            return content.title
        }

        let panes = host.shellState.panes.filter { $0.tabID == tab.tabID }
        let primaryPane = panes.first
        return shellDisplayTitle(
            rawTitle: tab.title ?? primaryPane?.viewport?.title,
            workingDirectoryName: primaryPane?.context?.workingDirectoryName,
            cwd: primaryPane?.cwd,
            program: primaryPane?.process?.program,
            launchTarget: primaryPane?.resolvedLaunchTarget ?? .shell,
            fallback: fallbackTitle(for: tab)
        )
    }

    private func tabSubtitle(for tab: ShellTab) -> String {
        if let content = host.shellState.contentStateProjection().primaryContent(in: tab.tabID) {
            if content.kind == .settings {
                return "Settings"
            }
            if content.kind == .markdown {
                return "Document"
            }
        }

        let panes = host.shellState.panes.filter { $0.tabID == tab.tabID }
        let primaryPane = panes.first
        let title = tabTitle(for: tab)

        if let primaryPane,
           let status = shellTerminalStatusSummary(for: primaryPane, now: activityFreshnessNow)
        {
            return status
        }

        if let branch = primaryPane?.context?.gitBranch,
           let folder = primaryPane?.context?.workingDirectoryName
        {
            if folder == title {
                return branch
            }
            return "\(folder)  ·  \(branch)"
        }

        if let folder = shellVisibleLabel(primaryPane?.context?.workingDirectoryName) ?? shellPathLeaf(primaryPane?.cwd) {
            if folder == title, let program = shellVisibleLabel(primaryPane?.process?.program) {
                return program
            }
            return folder
        }

        if let program = primaryPane?.process?.program {
            return program
        }

        return tab.kind.rawValue.capitalized
    }

    private func strongestAttention(for tab: ShellTab) -> ShellAttentionState? {
        host.shellState.panes
            .filter { $0.tabID == tab.tabID }
            .map { shellEffectiveAttention(for: $0, now: activityFreshnessNow) }
            .sorted { attentionRank(for: $0) > attentionRank(for: $1) }
            .first(where: { $0 != .idle })
    }

    private func strongestAttention(for space: ShellSpace) -> ShellAttentionState {
        host.shellState.panes
            .filter { $0.spaceID == space.spaceID }
            .map { shellEffectiveAttention(for: $0, now: activityFreshnessNow) }
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private func spaceSymbol(for space: ShellSpace) -> String {
        let hasAlan = host.shellState.panes.contains { pane in
            pane.spaceID == space.spaceID && pane.resolvedLaunchTarget == .alan
        }

        if hasAlan {
            return "sparkles"
        }

        if space.tabs.count > 1 {
            return "square.stack.3d.up"
        }

        let contentState = host.shellState.contentStateProjection()
        if let content = space.tabs.lazy.compactMap({ contentState.primaryContent(in: $0.tabID) }).first {
            return contentIconName(for: content)
        }

        return "terminal"
    }

    private func showsAlanMarker(for tab: ShellTab, activity: TerminalActivitySnapshot?) -> Bool {
        guard activity?.source.kind != .alan else { return false }
        return host.shellState.panes.contains { pane in
            pane.tabID == tab.tabID && pane.resolvedLaunchTarget == .alan
        }
    }

    private func contentIconName(for content: ShellContentInstance) -> String {
        if let iconName = content.iconName?.trimmingCharacters(in: .whitespacesAndNewlines),
           !iconName.isEmpty
        {
            return iconName
        }

        switch content.kind {
        case .terminal:
            return "terminal"
        case .markdown:
            return "doc.text"
        case .settings:
            return "gearshape"
        }
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
}

private struct ShellSidebarTabListOffsetPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct ShellSidebarTabDragState: Equatable {
    static let dragThreshold: CGFloat = 7

    let tabID: String
    let sourceSpaceID: String
    let sourceSection: ShellTabOrganizationSection
    let sourceIndex: Int
}

private struct ShellSidebarTabInsertionTarget: Equatable {
    let spaceID: String
    let section: ShellTabOrganizationSection
    let index: Int
}

private struct ShellSidebarTabDropDelegate: DropDelegate {
    let target: ShellSidebarTabInsertionTarget
    @Binding var activeDrag: ShellSidebarTabDragState?
    @Binding var preview: ShellSidebarTabInsertionTarget?
    let host: ShellHostController

    func dropEntered(info: DropInfo) {
        preview = resolvedTarget(for: info)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        preview = resolvedTarget(for: info)
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        if preview == target {
            preview = nil
        }
    }

    func performDrop(info: DropInfo) -> Bool {
        guard let activeDrag else {
            preview = nil
            return false
        }

        let insertionTarget = resolvedTarget(for: info)
        preview = nil
        self.activeDrag = nil

        let mutationIndex = mutationIndex(for: insertionTarget, activeDrag: activeDrag)

        if activeDrag.sourceSpaceID == insertionTarget.spaceID,
           activeDrag.sourceSection == insertionTarget.section,
           activeDrag.sourceIndex == mutationIndex
        {
            return true
        }

        return host.reorderTab(
            tabID: activeDrag.tabID,
            targetSpaceID: insertionTarget.spaceID,
            section: insertionTarget.section,
            index: mutationIndex
        )
    }

    private func mutationIndex(
        for insertionTarget: ShellSidebarTabInsertionTarget,
        activeDrag: ShellSidebarTabDragState
    ) -> Int {
        guard activeDrag.sourceSpaceID == insertionTarget.spaceID,
              activeDrag.sourceSection == insertionTarget.section,
              insertionTarget.index > activeDrag.sourceIndex
        else {
            return insertionTarget.index
        }

        return insertionTarget.index - 1
    }

    private func resolvedTarget(for info: DropInfo) -> ShellSidebarTabInsertionTarget {
        let rowMidpoint: CGFloat = 24
        let sectionCount = host.shellState
            .space(spaceID: target.spaceID)?
            .tabs(in: target.section)
            .count ?? target.index
        let adjustedIndex = info.location.y > rowMidpoint
            ? target.index + 1
            : target.index
        return ShellSidebarTabInsertionTarget(
            spaceID: target.spaceID,
            section: target.section,
            index: min(max(adjustedIndex, 0), sectionCount)
        )
    }
}

private struct ShellSidebarTabSectionDivider: View {
    var body: some View {
        Rectangle()
            .fill(ShellPalette.line.opacity(0.18))
            .frame(height: 0.5)
            .padding(.horizontal, ShellSidebarMetrics.rowInset + 4)
            .allowsHitTesting(false)
    }
}

private struct ShellSidebarTabInsertionLine: View {
    let isVisible: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
            .fill(ShellPalette.accent.opacity(isVisible ? 0.72 : 0))
            .frame(height: 2)
            .padding(.horizontal, ShellSidebarMetrics.rowInset + 2)
            .padding(.vertical, isVisible ? 3 : 0)
            .animation(.easeOut(duration: 0.10), value: isVisible)
            .accessibilityHidden(true)
    }
}

private struct ShellSidebarScrollBoundary: View {
    let progress: CGFloat

    var body: some View {
        VStack(spacing: 0) {
            Rectangle()
                .fill(ShellPalette.line.opacity(0.36))
                .frame(height: 0.5)

            LinearGradient(
                colors: [
                    ShellPalette.sidebarInk.opacity(0.10),
                    ShellPalette.sidebarInk.opacity(0.035),
                    ShellPalette.sidebarInk.opacity(0),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 14)
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .opacity(progress)
        .allowsHitTesting(false)
    }
}

private struct ShellSidebarSpaceHeader: View {
    @ObservedObject var host: ShellHostController
    let spaceID: String?

    var body: some View {
        headerPage(for: spaceID)
            .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 26)
    }

    private func headerPage(for spaceID: String?) -> some View {
        let space = space(for: spaceID)
        return HStack(spacing: 10) {
            Image(systemName: symbolName(for: space))
                .font(.system(size: ShellSidebarMetrics.iconPointSize, weight: .semibold))
                .foregroundStyle(ShellPalette.sidebarMutedInk.opacity(0.78))
                .frame(width: ShellSidebarMetrics.iconColumnWidth)

            Text(space?.title ?? "Space")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(ShellPalette.sidebarMutedInk.opacity(0.82))
                .lineLimit(1)
        }
        .padding(.horizontal, ShellSidebarMetrics.rowInset)
        .padding(.vertical, 5)
        .padding(.leading, ShellSidebarMetrics.edgeInset)
        .padding(.trailing, ShellSidebarMetrics.edgeInset)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func space(for spaceID: String?) -> ShellSpace? {
        guard let spaceID else { return host.selectedSpace }
        return host.spaces.first { $0.spaceID == spaceID }
    }

    private func symbolName(for space: ShellSpace?) -> String {
        guard let space else { return "terminal" }
        let hasAlan = host.shellState.panes.contains { pane in
            pane.spaceID == space.spaceID && pane.resolvedLaunchTarget == .alan
        }

        if hasAlan {
            return "sparkles"
        }

        if space.tabs.count > 1 {
            return "square.stack.3d.up"
        }

        let contentState = host.shellState.contentStateProjection()
        if let content = space.tabs.lazy.compactMap({ contentState.primaryContent(in: $0.tabID) }).first {
            return contentIconName(for: content)
        }

        return "terminal"
    }

    private func contentIconName(for content: ShellContentInstance) -> String {
        if let iconName = content.iconName?.trimmingCharacters(in: .whitespacesAndNewlines),
           !iconName.isEmpty
        {
            return iconName
        }

        switch content.kind {
        case .terminal:
            return "terminal"
        case .markdown:
            return "doc.text"
        case .settings:
            return "gearshape"
        }
    }
}

private struct ShellSpaceSwitcherItem: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let title: String
    let symbolName: String
    let attention: ShellAttentionState
    let tabCount: Int
    let isSelected: Bool
    let isPreviewed: Bool
    let isHovered: Bool

    var body: some View {
        ZStack {
            if isSelected || isHovered || isPreviewed {
                ShellMaterialShape(
                    role: isSelected ? .controlGlassSelected : .controlGlassHover,
                    shape: RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                )
            }
            Image(systemName: symbolName)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(isSelected || isPreviewed ? ShellPalette.accent : ShellPalette.sidebarInk.opacity(0.74))
        }
        .frame(width: 30, height: 30)
        .scaleEffect(isSelected ? 1 : (isHovered || isPreviewed ? 1.015 : 1))
        .shellShadow(isSelected || isPreviewed ? ShellShadows.navigationSelection : ShellShadows.none)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.16), value: isHovered)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.16), value: isSelected)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.16), value: isPreviewed)
        .help(title)
        .accessibilityLabel(accessibilityLabel)
    }

    private var accessibilityLabel: String {
        var parts = [title, tabCount == 1 ? "1 tab" : "\(tabCount) tabs"]
        if isSelected {
            parts.append("selected")
        }
        if isPreviewed {
            parts.append("preview")
        }
        if attention != .idle {
            parts.append("needs attention")
        }
        return parts.joined(separator: ", ")
    }
}

private enum ShellSidebarRowVisualState: Equatable {
    case normal
    case hover
    case selected

    var cornerRadius: CGFloat {
        switch self {
        case .normal:
            return ShellRadii.row
        case .hover:
            return ShellRadii.surface
        case .selected:
            return ShellRadii.overlay
        }
    }

    var fill: Color? {
        switch self {
        case .normal:
            return nil
        case .hover:
            return ShellPalette.sidebarRowHover
        case .selected:
            return ShellPalette.sidebarRowSelected
        }
    }

    var stroke: Color {
        switch self {
        case .normal:
            return .clear
        case .hover:
            return ShellPalette.line.opacity(0.08)
        case .selected:
            return ShellPalette.line.opacity(0.12)
        }
    }

    var shadow: ShellShadowStyle {
        switch self {
        case .normal, .hover:
            return ShellShadows.none
        case .selected:
            return ShellShadows.navigationSelection
        }
    }
}

private struct ShellSidebarRowBackground: View {
    @Environment(\.colorScheme) private var colorScheme
    let state: ShellSidebarRowVisualState

    var body: some View {
        if let fill = state.fill {
            let shape = RoundedRectangle(cornerRadius: state.cornerRadius, style: .continuous)
            shape
                .fill(fill)
                .overlay {
                    shape.stroke(state.stroke, lineWidth: 0.5)
                }
                .overlay {
                    if colorScheme == .light && state == .selected {
                        shape
                            .stroke(Color.white.opacity(0.34), lineWidth: 0.55)
                            .mask {
                                shape.fill(
                                    LinearGradient(
                                        colors: [
                                            Color.white,
                                            Color.white.opacity(0),
                                        ],
                                        startPoint: .top,
                                        endPoint: .bottom
                                    )
                                )
                            }
                    }
                }
                .shellShadow(state.shadow)
        }
    }
}

private enum ShellSidebarTypography {
    static let titleSize: CGFloat = 13
    static let secondarySize: CGFloat = 11
    static let markerSize: CGFloat = 9
    static let pinSize: CGFloat = 8.5
    static let closeSize: CGFloat = 9.5

    static func titleWeight(isSelected: Bool) -> Font.Weight {
        isSelected ? .medium : .regular
    }

    static let secondaryWeight: Font.Weight = .regular
    static let secondaryEmphasisWeight: Font.Weight = .medium
    static let iconWeight: Font.Weight = .medium
    static let markerWeight: Font.Weight = .semibold
}

private struct ShellTabSidebarRow: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var isKeyboardFocused: Bool
    @State private var isCloseHovered = false
    let title: String
    let subtitle: String
    let isActivitySubtitle: Bool
    let progress: TerminalActivityProgress?
    let attention: ShellAttentionState?
    let showsAlanMarker: Bool
    let paneSummary: ShellTabPaneSummary?
    let isPinned: Bool
    let isSelected: Bool
    let isHovered: Bool
    let showsCloseAffordance: Bool
    let onFocusSplitPane: (String) -> Void
    let onFocusNextSplitPane: (ShellTabPaneSummary) -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            leadingSlot
                .frame(width: 24, height: 24, alignment: .center)

            VStack(alignment: .leading, spacing: progress == nil ? 3 : 5) {
                HStack(spacing: 6) {
                    Text(title)
                        .font(
                            .system(
                                size: ShellSidebarTypography.titleSize,
                                weight: ShellSidebarTypography.titleWeight(isSelected: isSelected)
                            )
                        )
                        .foregroundStyle(titleForeground)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    if showsAlanMarker {
                        Image(systemName: "sparkles")
                            .font(
                                .system(
                                    size: ShellSidebarTypography.markerSize,
                                    weight: ShellSidebarTypography.markerWeight
                                )
                            )
                            .foregroundStyle(ShellPalette.accent)
                    }

                    if isPinned {
                        Image(systemName: "pin.fill")
                            .font(
                                .system(
                                    size: ShellSidebarTypography.pinSize,
                                    weight: ShellSidebarTypography.markerWeight
                                )
                            )
                            .foregroundStyle(ShellPalette.accent.opacity(isSelected ? 0.84 : 0.64))
                            .help("Pinned tab")
                    }
                }

                subtitleText
                    .foregroundStyle(subtitleForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)

                if let progress {
                    ShellSidebarActivityProgressRail(progress: progress, isSelected: isSelected)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            closeButtonSlot
        }
        .padding(.horizontal, ShellSidebarMetrics.rowInset)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            ShellSidebarRowBackground(state: visualState)
        )
        .contentShape(RoundedRectangle(cornerRadius: visualState.cornerRadius, style: .continuous))
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: visualState)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: showsCloseButton)
        .focusable()
        .focused($isKeyboardFocused)
        .focusEffectDisabled()
        .accessibilityLabel(accessibilityLabel)
        .help("Select tab")
    }

    private var visualState: ShellSidebarRowVisualState {
        if isSelected {
            return .selected
        }

        if isHovered || isKeyboardFocused {
            return .hover
        }

        return .normal
    }

    private var isInteractionActive: Bool {
        isHovered || showsCloseAffordance || isKeyboardFocused
    }

    private var showsCloseButton: Bool {
        isSelected || isInteractionActive
    }

    private var closeButtonSlot: some View {
        Button(action: onClose) {
            Image(systemName: "xmark")
                .font(
                    .system(
                        size: ShellSidebarTypography.closeSize,
                        weight: ShellSidebarTypography.markerWeight
                    )
                )
                .foregroundStyle(closeForeground)
                .frame(width: 20, height: 20)
                .contentShape(Circle())
                .background {
                    if isCloseHovered || isKeyboardFocused {
                        Circle()
                            .fill(ShellPalette.sidebarInk.opacity(isSelected ? 0.05 : 0.035))
                    }
                }
        }
        .buttonStyle(.plain)
        .opacity(showsCloseButton ? 1 : 0)
        .allowsHitTesting(showsCloseButton)
        .accessibilityHidden(!showsCloseButton)
        .help("Close tab")
        .accessibilityLabel("Close tab")
        .onHover { isHovering in
            isCloseHovered = isHovering
        }
    }

    @ViewBuilder
    private var leadingSlot: some View {
        if let paneSummary {
            ShellPaneTopologyIndicator(
                summary: paneSummary,
                isSelected: isSelected,
                onFocusSplitPane: onFocusSplitPane,
                onFocusNextSplitPane: onFocusNextSplitPane
            )
        } else {
            ShellPaneTopologyIndicator.placeholder(isSelected: isSelected)
        }
    }

    private var subtitleText: Text {
        let parts = subtitle.components(separatedBy: " · ")
        guard isActivitySubtitle, !parts.isEmpty else {
            return Text(subtitle)
                .font(
                    .system(
                        size: ShellSidebarTypography.secondarySize,
                        weight: ShellSidebarTypography.secondaryWeight
                    )
                )
        }

        let emphasizedIndex = emphasizedSubtitleIndex(for: parts)
        var attributedSubtitle = AttributedString()
        for element in parts.enumerated() {
            let (index, part) = element
            let prefix = index == 0 ? "" : " · "
            let weight = index == emphasizedIndex
                ? ShellSidebarTypography.secondaryEmphasisWeight
                : ShellSidebarTypography.secondaryWeight
            var fragment = AttributedString(prefix + part)
            fragment.font = .system(size: ShellSidebarTypography.secondarySize, weight: weight)
            attributedSubtitle += fragment
        }
        return Text(attributedSubtitle)
    }

    private func emphasizedSubtitleIndex(for parts: [String]) -> Int {
        if parts.count >= 3,
           parts[0].hasPrefix("Pane ")
        {
            return 1
        }
        return 0
    }

    private var titleForeground: Color {
        isSelected ? ShellPalette.sidebarInk : ShellPalette.sidebarInk.opacity(0.88)
    }

    private var subtitleForeground: Color {
        isSelected ? ShellPalette.sidebarMutedInk.opacity(0.90) : ShellPalette.sidebarMutedInk.opacity(0.68)
    }

    private var closeForeground: Color {
        if isCloseHovered {
            return ShellPalette.sidebarInk.opacity(isSelected ? 0.68 : 0.76)
        }
        return isSelected ? ShellPalette.sidebarInk.opacity(0.46) : ShellPalette.sidebarMutedInk.opacity(0.62)
    }

    private var accessibilityLabel: String {
        var parts = [title, subtitle]
        if isSelected {
            parts.append("selected")
        }
        if attention != nil {
            parts.append("needs attention")
        }
        if let paneSummary {
            parts.append(paneSummary.paneCount == 1 ? "1 pane" : "\(paneSummary.paneCount) panes")
        }
        if isPinned {
            parts.append("pinned")
        }
        return parts.joined(separator: ", ")
    }
}

private struct ShellSidebarActivityProgressRail: View {
    let progress: TerminalActivityProgress
    let isSelected: Bool

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.16 : 0.12))

                Capsule()
                    .fill(fillColor)
                    .frame(width: fillWidth(in: proxy.size.width))
            }
        }
        .frame(height: 2)
        .accessibilityHidden(true)
    }

    private var fillColor: Color {
        switch progress.kind {
        case .failed:
            return ShellPalette.attention.opacity(isSelected ? 0.86 : 0.72)
        case .paused:
            return ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.62 : 0.48)
        case .percent, .indeterminate:
            return ShellPalette.accent.opacity(isSelected ? 0.82 : 0.68)
        }
    }

    private func fillWidth(in width: CGFloat) -> CGFloat {
        switch progress.kind {
        case .percent:
            return width * CGFloat(progress.percent ?? 0) / 100
        case .indeterminate:
            return max(width * 0.36, 18)
        case .paused, .failed:
            return width
        }
    }
}

private struct ShellPaneTopologyIndicator: View {
    let summary: ShellTabPaneSummary
    let isSelected: Bool
    let onFocusSplitPane: (String) -> Void
    let onFocusNextSplitPane: (ShellTabPaneSummary) -> Void

    @ViewBuilder
    var body: some View {
        switch summary.topology.kind {
        case .single:
            singlePaneIndicator
        case .columns(let count):
            columnsIndicator(paneIDs: Array(summary.paneIDs.prefix(count)))
        case .rows(let count):
            rowsIndicator(paneIDs: Array(summary.paneIDs.prefix(count)))
        case .mainLeftWithRightStack:
            mainLeftWithRightStackIndicator
        case .mainRightWithLeftStack:
            mainRightWithLeftStackIndicator
        case .mainTopWithBottomSplit:
            mainTopWithBottomSplitIndicator
        case .mainBottomWithTopSplit:
            mainBottomWithTopSplitIndicator
        case .grid2x2(let rootDirection):
            gridIndicator(rootDirection: rootDirection)
        case .complex:
            complexButton
        }
    }

    private var singlePaneIndicator: some View {
        indicatorFrame {
            RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
                .fill(primaryPaneFill)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityLabel("Single pane")
    }

    private func columnsIndicator(paneIDs: [String]) -> some View {
        indicatorFrame {
            HStack(spacing: 2) {
                ForEach(paneIDs, id: \.self) { paneID in
                    segmentButton(paneID: paneID)
                }
            }
        }
        .help("Focus split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    private func rowsIndicator(paneIDs: [String]) -> some View {
        indicatorFrame {
            VStack(spacing: 2) {
                ForEach(paneIDs, id: \.self) { paneID in
                    segmentButton(paneID: paneID)
                }
            }
        }
        .help("Focus split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    @ViewBuilder
    private var mainLeftWithRightStackIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                HStack(spacing: 2) {
                    segmentButton(paneID: paneIDs[0])
                    VStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[1])
                        segmentButton(paneID: paneIDs[2])
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainRightWithLeftStackIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                HStack(spacing: 2) {
                    VStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[0])
                        segmentButton(paneID: paneIDs[1])
                    }
                    segmentButton(paneID: paneIDs[2])
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainTopWithBottomSplitIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                VStack(spacing: 2) {
                    segmentButton(paneID: paneIDs[0])
                    HStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[1])
                        segmentButton(paneID: paneIDs[2])
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainBottomWithTopSplitIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                VStack(spacing: 2) {
                    HStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[0])
                        segmentButton(paneID: paneIDs[1])
                    }
                    segmentButton(paneID: paneIDs[2])
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private func gridIndicator(rootDirection: ShellSplitDirection) -> some View {
        let paneIDs = Array(summary.paneIDs.prefix(4))
        if paneIDs.count == 4 {
            indicatorFrame {
                if rootDirection == .vertical {
                    HStack(spacing: 2) {
                        VStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[0])
                            segmentButton(paneID: paneIDs[1])
                        }
                        VStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[2])
                            segmentButton(paneID: paneIDs[3])
                        }
                    }
                } else {
                    VStack(spacing: 2) {
                        HStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[0])
                            segmentButton(paneID: paneIDs[1])
                        }
                        HStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[2])
                            segmentButton(paneID: paneIDs[3])
                        }
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    private func indicatorFrame<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: ShellRadii.badge, style: .continuous)

        return content()
            .padding(3)
            .frame(width: 22, height: 18)
            .background {
                shape
                    .fill(containerFill)
                    .overlay {
                        shape.stroke(ShellPalette.line.opacity(isSelected ? 0.20 : 0.15), lineWidth: 0.5)
                    }
            }
    }

    static func placeholder(isSelected: Bool) -> some View {
        let shape = RoundedRectangle(cornerRadius: ShellRadii.badge, style: .continuous)
        return shape
            .fill(ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.20 : 0.13))
            .frame(width: 22, height: 18)
            .overlay {
                shape.stroke(ShellPalette.line.opacity(isSelected ? 0.20 : 0.15), lineWidth: 0.5)
            }
            .accessibilityLabel("Pane")
    }

    private var containerFill: Color {
        ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.105 : 0.075)
    }

    private var primaryPaneFill: Color {
        isSelected ? ShellPalette.accent.opacity(0.82) : ShellPalette.sidebarMutedInk.opacity(0.38)
    }

    private func paneFill(isFocused: Bool) -> Color {
        if isFocused {
            return ShellPalette.accent.opacity(isSelected ? 0.88 : 0.78)
        }
        return ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.40 : 0.32)
    }

    private var complexButton: some View {
        Button {
            onFocusNextSplitPane(summary)
        } label: {
            complexCountOverlay
        }
        .buttonStyle(.plain)
        .help("Focus next split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    private var complexCountOverlay: some View {
        indicatorFrame {
            ZStack {
                RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
                    .fill(primaryPaneFill)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                Text("\(summary.paneCount)")
                    .font(.system(size: 8.5, weight: .bold, design: .monospaced))
                    .foregroundStyle(complexCountForeground)
            }
        }
    }

    private var complexCountForeground: Color {
        isSelected ? Color.white.opacity(0.92) : ShellPalette.sidebarInk.opacity(0.76)
    }

    private func segmentButton(paneID: String) -> some View {
        Button {
            onFocusSplitPane(paneID)
        } label: {
            segmentView(paneID: paneID)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(summary.focusedPaneID == paneID ? "Focused split pane" : "Focus split pane")
        .accessibilityLabel(summary.focusedPaneID == paneID ? "Focused split pane" : "Focus split pane")
    }

    private func segmentView(paneID: String) -> some View {
        let isFocused = summary.focusedPaneID == paneID

        return RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
            .fill(paneFill(isFocused: isFocused))
    }

    private var splitAccessibilityLabel: String {
        "Split tab, \(summary.accessibilityTopologyLabel)"
    }
}

private struct ShellCompactEmptyAction: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var isKeyboardFocused: Bool
    @State private var isHovered = false
    let title: String
    let systemImage: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: systemImage)
                    .font(.system(size: 11, weight: .semibold))
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                Spacer(minLength: 0)
            }
            .foregroundStyle(foreground)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                ShellSidebarRowBackground(state: visualState)
            )
        }
        .buttonStyle(.plain)
        .focusable()
        .focused($isKeyboardFocused)
        .focusEffectDisabled()
        .onHover { isHovered = $0 }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: visualState)
        .accessibilityLabel(title)
    }

    private var visualState: ShellSidebarRowVisualState {
        isHovered || isKeyboardFocused ? .hover : .normal
    }

    private var foreground: Color {
        isHovered || isKeyboardFocused
            ? ShellPalette.sidebarMutedInk.opacity(0.86)
            : ShellPalette.sidebarMutedInk.opacity(0.58)
    }
}
#endif
