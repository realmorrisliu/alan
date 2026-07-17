import SwiftUI
import UniformTypeIdentifiers

#if os(macOS)
struct ShellSidebarView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ObservedObject var host: ShellHostController
    let chromeMetrics: ShellWindowChromeMetrics
    let displaySpaceID: String?
    let isSwipeEnabled: Bool
    @State private var spacePager: ShellSidebarSpaceContentPagerState?
    @State private var spacePagerToken = 0
    @State private var spacePagerPageWidth: CGFloat = 1
    @State private var hoveredTabID: String?
    @State private var tabListScrollOffsetY: CGFloat = 0
    @State private var activityFreshnessNow = Date()
    @State private var activeTabDrag: ShellSidebarTabDragSource?
    @State private var tabInsertionPreview: ShellSidebarTabDropTarget?
    @State private var renamingTabID: String?
    @State private var renameDraftTitle = ""
    @StateObject private var tabListWheelRouter = ShellSidebarTabListWheelRouter()
    @StateObject private var spaceCreationProfileOptions = ShellSpaceCreationProfileOptionStore()

    init(
        host: ShellHostController,
        chromeMetrics: ShellWindowChromeMetrics,
        displaySpaceID: String?,
        previewedSpaceID: String? = nil,
        isSpaceSwipeGestureLocked: Bool = false,
        isSwipeEnabled: Bool,
        onSpaceSwipe: @escaping (ShellSidebarSwipeUpdate) -> Void = { _ in }
    ) {
        self.host = host
        self.chromeMetrics = chromeMetrics
        self.displaySpaceID = displaySpaceID
        self.isSwipeEnabled = isSwipeEnabled
        _ = previewedSpaceID
        _ = isSpaceSwipeGestureLocked
        _ = onSpaceSwipe
    }

    var body: some View {
        sidebarContent
        .scrollDisabled(isTabListScrollDisabled)
        .onChange(of: sourceSpaceID) { _, _ in
            tabListScrollOffsetY = 0
        }
        .task(id: activityFreshnessRefreshID) {
            await scheduleActivityFreshnessRefresh()
        }
        .task {
            spaceCreationProfileOptions.refresh()
        }
        .onChange(of: host.isPresentingSpaceCreation) { _, isPresenting in
            if isPresenting {
                spaceCreationProfileOptions.refresh()
            }
        }
        .alert("Rename Tab", isPresented: renameAlertBinding) {
            TextField("Title", text: $renameDraftTitle)
            Button("Rename") {
                commitRename()
            }
            Button("Cancel", role: .cancel) {
                renamingTabID = nil
            }
        }
    }

    private var renameAlertBinding: Binding<Bool> {
        Binding(
            get: { renamingTabID != nil },
            set: { isPresented in
                if !isPresented {
                    renamingTabID = nil
                }
            }
        )
    }

    private var sidebarContent: some View {
        VStack(alignment: .leading, spacing: 0) {
            fixedSpaceSlider
                .padding(.bottom, ShellSidebarTabListMetrics.itemSpacing)
            if host.isPresentingSpaceCreation {
                ShellSpaceCreationForm(
                    profiles: spaceCreationProfileOptions.options,
                    draftName: $host.spaceDraftName,
                    draftIcon: $host.spaceDraftIcon,
                    draftProfileID: $host.spaceDraftProfileID,
                    onCreate: {
                        _ = host.createSpaceFromForm()
                    },
                    onCancel: { host.cancelSpaceCreation() }
                )
                .transition(.opacity)
            } else {
                spaceContentPager
                    .padding(.top, -ShellSidebarTabListMetrics.sliderToListLift)
            }
        }
        .padding(.top, chromeMetrics.commandLauncherTopInset)
        .padding(.bottom, ShellSidebarMetrics.edgeInset)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .animation(
            reduceMotion ? nil : .easeInOut(duration: 0.18),
            value: host.isPresentingSpaceCreation
        )
    }

    private var fixedSpaceSlider: some View {
        ShellSidebarSpaceSlider(
            host: host,
            displaySpaceID: sourceSpaceID,
            previewedSpaceID: previewedSpaceID,
            activityFreshnessNow: activityFreshnessNow,
            onForwardVerticalWheel: tabListWheelRouter.forward,
            creationDraft: host.isPresentingSpaceCreation
                ? ShellSpaceSliderDraft(name: host.spaceDraftName, iconSystemName: host.spaceDraftIcon)
                : nil
        )
        .frame(maxWidth: .infinity)
        .frame(height: ShellSidebarSpaceSliderLayout.trackHeight + 4)
    }

    private var spaceContentPager: some View {
        GeometryReader { proxy in
            let pageWidth = max(proxy.size.width, 1)
            ZStack(alignment: .leading) {
                ForEach(spacePageIndices, id: \.self) { index in
                    tabSection(for: spaceID(forSpaceAt: index))
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
        .background {
            if isSwipeEnabled {
                ShellSidebarSwipeMonitor(onUpdate: handleSpaceSwipe)
            }
        }
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
            if spaceID == sourceSpaceID {
                ShellSidebarTabListWheelForwardingAnchor(router: tabListWheelRouter)
                    .frame(width: 0, height: 0)
                    .accessibilityHidden(true)
            }

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
        let temporarySectionPresentation = ShellSidebarTemporaryTabSectionPresentation.model(
            pinnedTabCount: pinnedTabs.count,
            unpinnedTabCount: unpinnedTabs.count,
            clearableTabCount: host.clearableInactiveTabCount(in: space.spaceID)
        )

        VStack(alignment: .leading, spacing: ShellSidebarTabListMetrics.itemSpacing) {
            if !pinnedTabs.isEmpty {
                tabRows(
                    pinnedTabs,
                    in: space,
                    section: .pinned
                )
            }

            if temporarySectionPresentation.showsControlRow {
                tabControlRow(for: space, presentation: temporarySectionPresentation)
            }
            newTabRow(for: space)

            tabRows(
                unpinnedTabs,
                in: space,
                section: .unpinned
            )
        }
    }

    private func newTabRow(for space: ShellSpace) -> some View {
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
    }

    private func tabControlRow(
        for space: ShellSpace,
        presentation: ShellSidebarTemporaryTabSectionPresentation
    ) -> some View {
        ShellSidebarTabControlRow(
            showsDivider: presentation.showsDivider,
            showsClear: presentation.showsClear,
            isClearEnabled: presentation.isClearEnabled,
            clearAction: {
                host.clearInactiveTemporaryTabs(in: space.spaceID)
            }
        )
    }

    @ViewBuilder
    private func tabRows(
        _ tabs: [ShellTab],
        in space: ShellSpace,
        section: ShellTabOrganizationSection
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(tabs.enumerated()), id: \.element.id) { index, tab in
                VStack(alignment: .leading, spacing: 0) {
                    insertionPreviewLine(
                        spaceID: space.spaceID,
                        section: section,
                        index: index
                    )
                    tabListRow(for: tab, in: space, section: section, index: index)
                }
                .padding(.bottom, index == tabs.count - 1 ? 0 : ShellSidebarTabListMetrics.itemSpacing)
            }

            insertionPreviewLine(
                spaceID: space.spaceID,
                section: section,
                index: tabs.count
            )
            .frame(height: tabs.isEmpty ? ShellSidebarTabListMetrics.itemSpacing : 0)
            .onDrop(
                of: [.plainText],
                delegate: ShellSidebarTabDropDelegate(
                    target: ShellSidebarTabDropTarget(
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
    }

    @ViewBuilder
    private func insertionPreviewLine(
        spaceID: String,
        section: ShellTabOrganizationSection,
        index: Int
    ) -> some View {
        let target = ShellSidebarTabDropTarget(
            spaceID: spaceID,
            section: section,
            index: index
        )
        ShellSidebarTabInsertionLine(
            isVisible: tabInsertionPreview == target
        )
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
            secondaryIsMachineFact: projection.secondaryIsMachineFact,
            progress: projection.progress,
            stateAccessory: projection.stateAccessory,
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
            let source = ShellSidebarTabDragSource(
                tabID: tab.tabID,
                sourceSpaceID: space.spaceID,
                sourceSection: section,
                sourceIndex: index
            )
            let payload = (try? source.encodedPlainTextPayload()) ?? tab.tabID
            return NSItemProvider(object: payload as NSString)
        }
        .onDrop(
            of: [.plainText],
            delegate: ShellSidebarTabDropDelegate(
                target: ShellSidebarTabDropTarget(
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
            Button(host.shellActionTitle(.tabRename)) {
                beginRenaming(tab: tab)
            }
            .disabled(!host.shellActionAvailability(.tabRename, target: .contextTab(tab.tabID)).isAvailable)

            Button(host.shellActionTitle(.tabDuplicate)) {
                host.performShellAction(.tabDuplicate, target: .contextTab(tab.tabID))
            }
            .disabled(!host.shellActionAvailability(.tabDuplicate, target: .contextTab(tab.tabID)).isAvailable)

            Button(host.shellActionTitle(.tabOpenInSplitView)) {
                host.performShellAction(.tabOpenInSplitView, target: .contextTab(tab.tabID))
            }
            .disabled(!host.shellActionAvailability(.tabOpenInSplitView, target: .contextTab(tab.tabID)).isAvailable)

            Divider()
            if host.isTabPinned(tabID: tab.tabID) {
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

    private func beginRenaming(tab: ShellTab) {
        renamingTabID = tab.tabID
        renameDraftTitle = tab.title ?? ""
    }

    private func commitRename() {
        guard let tabID = renamingTabID else { return }
        let title = renameDraftTitle
        renamingTabID = nil
        host.renameTab(tabID: tabID, title: title)
    }

    private func beginTabDragIfNeeded(
        tab: ShellTab,
        space: ShellSpace,
        section: ShellTabOrganizationSection,
        index: Int
    ) {
        let nextDrag = ShellSidebarTabDragSource(
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
            .filter(\.requiresUserAction)
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
    }

    private func showsAlanMarker(for tab: ShellTab, activity: TerminalActivitySnapshot?) -> Bool {
        false
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
        case .agent:
            return "sparkles"
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
#endif
