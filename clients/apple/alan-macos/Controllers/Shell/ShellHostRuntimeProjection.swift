import Foundation

#if os(macOS)
extension ShellHostController {
    func updateTerminalRuntime(_ runtime: TerminalHostRuntimeSnapshot) {
        let updateStartedAt = performanceDiagnosticsStartTime()
        defer {
            if let updateStartedAt {
                recordPerformanceDiagnostic(
                    .runtimeSnapshotPublish,
                    durationMs: performanceDurationMs(since: updateStartedAt),
                    runtime: runtime
                )
            }
        }
        let shouldProjectToShell = terminalRuntimeRegistry.updateSnapshot(runtime)

        if let paneID = runtime.paneID,
           runtime.isFocused,
           shellState.focusedPaneID != paneID
        {
            focus(paneID: paneID)
            return
        }

        guard shouldProjectToShell else { return }
        terminalRuntimeRegistry.publishShellProjection(runtime) { [weak self] snapshot in
            self?.projectTerminalRuntime(snapshot)
        }
    }

    private func projectTerminalRuntime(_ runtime: TerminalHostRuntimeSnapshot) {
        let projectionStartedAt = performanceDiagnosticsStartTime()
        defer {
            if let projectionStartedAt {
                recordPerformanceDiagnostic(
                    .shellRuntimeProjection,
                    durationMs: performanceDurationMs(since: projectionStartedAt),
                    runtime: runtime
                )
            }
        }
        if let paneID = runtime.paneID,
           let pane = pane(paneID: paneID)
        {
            let bootProfile = bootProfileCache.profile(for: pane, shellState: shellState)
            let effectProjection = terminalContentProjection.projectRuntime(
                runtime,
                for: pane,
                bootProfile: bootProfile
            )
            let activeTaskChanged = terminalRuntimeRegistry.recordActiveTask(
                runtime.paneMetadata.activeTaskState,
                processExited: effectProjection.processExited,
                forPaneID: paneID
            )
            if effectProjection.processExited {
                routeActivityNotificationIfNeeded(from: pane, nextActivity: effectProjection.activity)
            }
            if closePaneAfterChildExitIfNeeded(
                paneID: paneID,
                processExited: effectProjection.processExited
            ) {
                return
            }

            let paneStateStartedAt = performanceDiagnosticsStartTime()
            let didPublishPaneUpdate = updatePaneState(paneID: paneID) { current in
                let currentBootProfile = bootProfileCache.profile(for: current, shellState: shellState)
                return terminalContentProjection.projectRuntime(
                    runtime,
                    for: current,
                    bootProfile: currentBootProfile
                ).pane
            }
            if let paneStateStartedAt {
                recordPerformanceDiagnostic(
                    .shellPaneStatePublication,
                    durationMs: performanceDurationMs(since: paneStateStartedAt),
                    runtime: runtime
                )
            }
            if didPublishPaneUpdate || activeTaskChanged {
                publishControlPlaneState(coalesced: true)
            }
        }
    }

    func recordPerformanceDiagnostic(
        _ kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double,
        runtime: TerminalHostRuntimeSnapshot,
        fallbackPaneID: String? = nil,
        fallbackContentID: String? = nil,
        fallbackPriority: TerminalRuntimeRenderPriority? = nil,
        counts: AlanPerformanceDiagnosticCounts? = nil
    ) {
        if let performanceDiagnosticsRecorder {
            guard performanceDiagnosticsRecorder.isEnabled else { return }
        } else {
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else { return }
        }
        let paneID = runtime.paneID ?? fallbackPaneID
        let contentID = runtime.contentID
            ?? fallbackContentID
            ?? paneID.map { ShellContentInstance.terminalContentID(forPaneID: $0) }
        let priority = fallbackPriority ?? runtime.renderPriority
        let event = AlanPerformanceDiagnosticEvent(
            kind: kind,
            durationMs: durationMs,
            paneID: paneID,
            contentID: contentID,
            priority: priority.diagnosticsValue,
            visibility: priority.diagnosticsVisibility,
            thread: Thread.isMainThread ? "main" : "background",
            counts: counts
        )
        if let performanceDiagnosticsRecorder {
            performanceDiagnosticsRecorder.record(event)
        } else {
            AlanPerformanceDiagnosticsController.shared.record(
                kind,
                durationMs: durationMs,
                paneID: event.paneID,
                contentID: event.contentID,
                priority: event.priority,
                visibility: event.visibility,
                thread: event.thread,
                counts: event.counts
            )
        }
    }

    func performanceDurationMs(since start: DispatchTime) -> Double {
        let end = DispatchTime.now()
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    func performanceDiagnosticsStartTime() -> DispatchTime? {
        if let performanceDiagnosticsRecorder {
            return performanceDiagnosticsRecorder.isEnabled ? DispatchTime.now() : nil
        }
        return AlanPerformanceDiagnosticsController.shared.isEnabled ? DispatchTime.now() : nil
    }

    func updateTerminalMetadata(_ metadata: TerminalPaneMetadataSnapshot, for paneID: String) {
        let metadataStartedAt = performanceDiagnosticsStartTime()
        guard let pane = pane(paneID: paneID) else { return }
        let bootProfile = bootProfileCache.profile(for: pane, shellState: shellState)
        let runtime = runtime(for: pane.paneID)
        defer {
            if let metadataStartedAt {
                recordPerformanceDiagnostic(
                    .terminalMetadataCallback,
                    durationMs: performanceDurationMs(since: metadataStartedAt),
                    runtime: runtime
                )
            }
        }
        let effectProjection = terminalContentProjection.projectMetadata(
            metadata,
            runtime: runtime,
            for: pane,
            bootProfile: bootProfile
        )
        let activeTaskChanged = terminalRuntimeRegistry.recordActiveTask(
            metadata.activeTaskState,
            processExited: effectProjection.processExited,
            forPaneID: paneID
        )
        if effectProjection.processExited {
            routeActivityNotificationIfNeeded(from: pane, nextActivity: effectProjection.activity)
        }
        if closePaneAfterChildExitIfNeeded(
            paneID: paneID,
            processExited: effectProjection.processExited
        ) {
            return
        }

        let didPublishPaneUpdate = updatePaneState(
            paneID: pane.paneID,
            tabTitleOverride: metadata.title
        ) { current in
            let currentBootProfile = bootProfileCache.profile(for: current, shellState: shellState)
            return terminalContentProjection.projectMetadata(
                metadata,
                runtime: runtime,
                for: current,
                bootProfile: currentBootProfile
            ).pane
        }
        if didPublishPaneUpdate || activeTaskChanged {
            publishControlPlaneState(coalesced: true)
        }
    }

    func updateAgentActivity(
        _ activity: TerminalActivitySnapshot,
        workingDirectory: String?,
        observedAt: Date,
        for paneID: String
    ) {
        guard pane(paneID: paneID) != nil else { return }
        _ = updatePaneState(paneID: paneID) { current in
            let bootProfile = bootProfileCache.profile(for: current, shellState: shellState)
            return terminalContentProjection.projectAgentActivity(
                activity,
                workingDirectory: workingDirectory,
                observedAt: observedAt,
                for: current,
                bootProfile: bootProfile
            )
        }
    }

    func applyAlanBinding(_ binding: ShellAlanBinding?, for paneID: String) {
        guard let pane = pane(paneID: paneID) else { return }
        let runtime = runtime(for: pane.paneID)
        updatePaneState(paneID: paneID) { current in
            let currentBootProfile = bootProfileCache.profile(for: current, shellState: shellState)
            return terminalContentProjection.projectAlanBinding(
                binding,
                runtime: runtime,
                for: current,
                bootProfile: currentBootProfile
            ).pane
        }
    }

    func primeBootContext(for paneID: String) {
        guard let pane = pane(paneID: paneID) else { return }
        let runtime = runtime(for: pane.paneID)

        updatePaneState(paneID: paneID) { current in
            let currentBootProfile = bootProfileCache.profile(for: current, shellState: shellState)
            return terminalContentProjection.projectBootContext(
                runtime: runtime,
                for: current,
                bootProfile: currentBootProfile
            ).pane
        }
    }

    @discardableResult
    private func updatePaneState(
        paneID: String,
        tabTitleOverride: String? = nil,
        transform: (ShellPane) -> ShellPane
    ) -> Bool {
        guard let existingPane = shellState.panes.first(where: { $0.paneID == paneID }) else {
            return false
        }
        let transformedPane = transform(existingPane)
        let currentTab = shellState.tab(tabID: existingPane.tabID)
        let currentTabTitle = currentTab?.title
        let requestedTabTitle = currentTab?.isTitleUserLocked == true
            ? currentTabTitle
            : (tabTitleOverride ?? currentTabTitle)

        guard transformedPane != existingPane || requestedTabTitle != currentTabTitle else {
            return false
        }

        let updatedPanes = shellState.panes.map { pane in
            pane.paneID == paneID ? transformedPane : pane
        }
        let updatedSpaces = rebuildSpaces(
            using: updatedPanes,
            tabTitleOverride: tabTitleOverride,
            paneID: paneID
        )

        shellState = ShellStateSnapshot(
            contractVersion: shellState.contractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: shellState.focusedSpaceID,
            focusedTabID: shellState.focusedTabID,
            focusedPaneID: shellState.focusedPaneID,
            spaces: updatedSpaces,
            panes: updatedPanes,
            paneSlots: shellState.paneSlots,
            contents: shellState.contents,
            zoomedPaneIDByTabID: shellState.zoomedPaneIDByTabID
        )
        refreshSelectionRuntimeProjection()
        routeActivityNotificationIfNeeded(from: existingPane, to: transformedPane)
        publishControlPlaneState(coalesced: true)
        return true
    }

    private func routeActivityNotificationIfNeeded(
        from existingPane: ShellPane,
        nextActivity: TerminalActivitySnapshot?
    ) {
        guard existingPane.activity != nextActivity,
              let activity = nextActivity,
              let tab = activityNotificationTab(for: existingPane)
        else {
            return
        }

        routeActivityNotificationIfNeeded(
            activity: activity,
            pane: existingPane,
            tab: tab
        )
    }

    private func routeActivityNotificationIfNeeded(
        from existingPane: ShellPane,
        to updatedPane: ShellPane
    ) {
        guard existingPane.activity != updatedPane.activity,
              let activity = updatedPane.activity,
              let tab = activityNotificationTab(for: updatedPane)
        else {
            return
        }

        routeActivityNotificationIfNeeded(
            activity: activity,
            pane: updatedPane,
            tab: tab
        )
    }

    private func routeActivityNotificationIfNeeded(
        activity: TerminalActivitySnapshot,
        pane: ShellPane,
        tab: ShellTab
    ) {
        let key = shellActivityNotificationKey(for: activity, paneID: pane.paneID)
        guard !routedActivityNotificationKeys.contains(key),
              let route = shellActivityNotificationRoute(
                  for: activity,
                  pane: pane,
                  tab: tab,
                  visibility: activityNotificationVisibility(for: pane),
                  now: .now
              )
        else {
            return
        }

        routedActivityNotificationKeys.insert(key)
        activityNotifications.append(route)
        if activityNotifications.count > 50 {
            activityNotifications.removeFirst(activityNotifications.count - 50)
        }
    }

    private func activityNotificationTab(for pane: ShellPane) -> ShellTab? {
        shellState.tab(tabID: pane.tabID)
    }

    private func activityNotificationVisibility(
        for pane: ShellPane
    ) -> ShellActivityNotificationVisibility {
        let isSelectedSpace = pane.spaceID == selectedSpace?.spaceID
        let isSelectedTab = pane.tabID == selectedTab?.tabID
        guard appIsActiveProvider() else {
            return .background
        }
        if isSelectedSpace,
           isSelectedTab,
           pane.paneID == shellState.focusedPaneID
        {
            return .focusedVisible
        }

        if isSelectedSpace, isSelectedTab {
            return .visibleUnfocused
        }

        return .background
    }

    private func rebuildSpaces(
        using panes: [ShellPane],
        tabTitleOverride: String?,
        paneID: String
    ) -> [ShellSpace] {
        let tabID = shellState.panes.first(where: { $0.paneID == paneID })?.tabID

        return shellState.spaces.map { space in
            let tabs = space.tabs.map { tab in
                let nextTitle: String?
                if tab.tabID == tabID, let tabTitleOverride, !tab.isTitleUserLocked {
                    nextTitle = tabTitleOverride
                } else {
                    nextTitle = tab.title
                }

                return ShellTab(
                    tabID: tab.tabID,
                    kind: tab.kind,
                    title: nextTitle,
                    paneTree: tab.paneTree,
                    isPinned: tab.isPinned,
                    isTitleUserLocked: tab.isTitleUserLocked
                )
            }

            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(in: panes.filter { $0.spaceID == space.spaceID }),
                tabs: tabs,
                selectedTabID: space.selectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }
    }

    private func replaceShellState(
        spaces: [ShellSpace],
        panes: [ShellPane],
        focusedPaneID: String?
    ) {
        let resolvedFocusedPaneID =
            focusedPaneID.flatMap { candidate in
                panes.contains(where: { $0.paneID == candidate }) ? candidate : nil
            } ?? panes.first?.paneID
        let focusedPane = resolvedFocusedPaneID.flatMap { candidate in
            panes.first(where: { $0.paneID == candidate })
        }
        let repairedSpaces = spaces.map { space in
            space.repairingSelectedTabID(
                preferredTabID: focusedPane?.spaceID == space.spaceID ? focusedPane?.tabID : nil
            )
        }

        shellState = ShellStateSnapshot(
            contractVersion: shellState.contractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: focusedPane?.spaceID ?? spaces.first?.spaceID,
            focusedTabID: focusedPane?.tabID ?? spaces.first?.tabs.first?.tabID,
            focusedPaneID: resolvedFocusedPaneID,
            spaces: repairedSpaces,
            panes: panes,
            paneSlots: shellState.paneSlots,
            contents: shellState.contents,
            zoomedPaneIDByTabID: shellState.zoomedPaneIDByTabID
        )
        refreshSelectionRuntimeProjection()
        publishControlPlaneState()
    }

    func applyMutationResult(
        _ result: ShellStateMutationResult,
        publish: Bool = true,
        pinSnapshotTabIDs: Set<String> = []
    ) {
        adoptStateFromControlPlane(result.state, publish: publish && pinSnapshotTabIDs.isEmpty)
        if publish && !pinSnapshotTabIDs.isEmpty {
            publishControlPlaneState(pinSnapshotTabIDs: pinSnapshotTabIDs)
        }
    }

    func adoptStateFromControlPlane(
        _ state: ShellStateSnapshot,
        publish: Bool = true
    ) {
        let previousPanesByID = Dictionary(
            uniqueKeysWithValues: shellState.panes.map { ($0.paneID, $0) }
        )
        terminalRuntimeRegistry.releaseRuntimes(
            excluding: state.contentStateProjection().activeTerminalMounts
        )

        shellState = platformMetadataPreserver.preservingPlatformMetadata(in: state) { [weak self] paneID in
            self?.runtime(for: paneID) ?? .placeholder
        }
        for pane in shellState.panes {
            guard let previousPane = previousPanesByID[pane.paneID] else { continue }
            routeActivityNotificationIfNeeded(from: previousPane, to: pane)
        }
        reconcilePaneZoomState()
        refreshSelectionRuntimeProjection()
        if publish {
            publishControlPlaneState()
        }
    }

    func recordControlPlaneDiagnostic(_ message: String) {
        let line = "\(Self.iso8601Formatter.string(from: .now)) \(message)"
        guard controlPlaneDiagnostics.last != line else { return }
        controlPlaneDiagnostics.append(line)
        if controlPlaneDiagnostics.count > 12 {
            controlPlaneDiagnostics.removeFirst(controlPlaneDiagnostics.count - 12)
        }
    }

    func refreshSelectionRuntimeProjection() {
        let selectionStartedAt = performanceDiagnosticsStartTime()
        defer {
            if let selectionStartedAt {
                let selectedPane = selectedPane
                recordPerformanceDiagnostic(
                    .shellSelectionChange,
                    durationMs: performanceDurationMs(since: selectionStartedAt),
                    runtime: selectedPaneRuntime,
                    fallbackPaneID: selectedPane?.paneID,
                    fallbackContentID: selectedPane?.terminalContentID,
                    fallbackPriority: selectedPane.map { terminalRenderPriority(for: $0) }
                )
            }
        }
        synchronizeTerminalRenderPriorities()
    }

    func synchronizeTerminalRenderPriorities() {
        let synchronizationStartedAt = performanceDiagnosticsStartTime()
        let contentState = shellState.contentStateProjection()
        let prioritiesByContentID = shellState.panes.reduce(
            into: [String: TerminalRuntimeRenderPriority]()
        ) { priorities, pane in
            guard paneHasTerminalContent(pane, in: contentState) else { return }
            let contentID = contentState.contentMounted(in: pane.paneID)?.contentID
                ?? pane.terminalContentID
            priorities[contentID] = terminalRenderPriority(for: pane)
        }
        terminalRuntimeRegistry.updateRenderPriorities(prioritiesByContentID)
        if let synchronizationStartedAt {
            let selectedPane = selectedPane
            recordPerformanceDiagnostic(
                .shellPrioritySynchronization,
                durationMs: performanceDurationMs(since: synchronizationStartedAt),
                runtime: selectedPaneRuntime,
                fallbackPaneID: selectedPane?.paneID,
                fallbackContentID: selectedPane?.terminalContentID,
                fallbackPriority: selectedPane.map { terminalRenderPriority(for: $0) },
                counts: AlanPerformanceDiagnosticCounts(events: prioritiesByContentID.count)
            )
        }
    }

    func zoomedPaneID(in tab: ShellTab) -> String? {
        guard let paneID = zoomedPaneIDByTabID[tab.tabID],
              tab.paneTree.contains(paneID: paneID),
              tab.paneTree.paneIDs.count > 1
        else {
            return nil
        }
        return paneID
    }

    func paneSupportsTerminalCommands(
        _ pane: ShellPane,
        in contentState: ShellContentStateSnapshot
    ) -> Bool {
        if let content = contentState.contentMounted(in: pane.paneID) {
            return content.kind == .terminal
                && content.capabilities.contains(.terminalInput)
        }

        return false
    }

    func paneHasTerminalContent(
        _ pane: ShellPane,
        in contentState: ShellContentStateSnapshot
    ) -> Bool {
        if let content = contentState.contentMounted(in: pane.paneID) {
            return content.kind == .terminal
        }

        return false
    }

    func tab(containingPaneID paneID: String) -> ShellTab? {
        shellState.spaces
            .flatMap(\.tabs)
            .first { $0.contains(paneID: paneID) }
    }

    private func reconcilePaneZoomState() {
        var nextZoomState: [String: String] = [:]
        let tabsByID = Dictionary(uniqueKeysWithValues: shellState.spaces.flatMap(\.tabs).map {
            ($0.tabID, $0)
        })

        for (tabID, paneID) in shellState.zoomedPaneIDByTabID {
            guard let tab = tabsByID[tabID],
                  tab.paneTree.paneIDs.count > 1,
                  tab.paneTree.contains(paneID: paneID),
                  shellState.pane(paneID: paneID)?.tabID == tabID
            else {
                continue
            }
            nextZoomState[tabID] = paneID
        }

        if let focusedPane = focusedPane,
           nextZoomState[focusedPane.tabID] != nil,
           tabsByID[focusedPane.tabID]?.paneTree.contains(paneID: focusedPane.paneID) == true
        {
            nextZoomState[focusedPane.tabID] = focusedPane.paneID
        }

        if nextZoomState != zoomedPaneIDByTabID {
            zoomedPaneIDByTabID = nextZoomState
        }
        if nextZoomState != shellState.zoomedPaneIDByTabID {
            shellState.zoomedPaneIDByTabID = nextZoomState
        }
    }

}
#endif
