import Foundation

#if os(macOS)
extension ShellHostController {
    @discardableResult
    func createSpace(
        launchTarget: ShellLaunchTarget = .shell,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) -> String? {
        let resolvedTerminalProfileID = terminalProfileID
        let result: ShellStateMutationResult
        do {
            switch launchTarget {
            case .shell:
                result = try reducerAdapter.apply(
                    state: shellState,
                    operation: .createTerminalSpace(
                        title: title,
                        tabTitle: nil,
                        workingDirectory: workingDirectory,
                        terminalProfileID: resolvedTerminalProfileID,
                        presentationIcon: presentationIconSystemName,
                        reservedPaneSlotIDs: terminalRuntimeRegistry.registeredPaneIDs.sorted()
                    )
                )
            }
        } catch {
            return nil
        }
        applyMutationResult(result)
        return result.spaceID
    }

    @discardableResult
    func createTerminalSpace(
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) -> String? {
        return createSpace(
            launchTarget: .shell,
            title: title,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: presentationIconSystemName
        )
    }

    @discardableResult
    func setTerminalProfile(_ terminalProfileID: String?, forSpaceID spaceID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .setTerminalProfile(
                    spaceID: spaceID,
                    terminalProfileID: terminalProfileID
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    /// Sets (or clears) the presentation icon for a Space.
    ///
    /// Pass a valid SF Symbol name to override, or `nil` to clear back to the monogram default.
    /// Invalid symbol names are treated as `nil` (clear) — the mutation rejects garbage input.
    @discardableResult
    func setPresentationIcon(_ systemName: String?, forSpaceID spaceID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .setPresentationIcon(
                    spaceID: spaceID,
                    presentationIcon: systemName
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func deleteSpace(spaceID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .deleteSpace(
                    spaceID: spaceID,
                    defaultWorkingDirectory: FileManager.default.homeDirectoryForCurrentUser.path
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    func isTabPinned(tabID: String) -> Bool {
        persistenceCoordinator.isTabPinned(tabID: tabID, in: shellState)
    }

    @discardableResult
    func pinTab(tabID: String? = nil) -> Bool {
        guard let targetTabID = tabID ?? selectedTabID else { return false }
        if isTabPinned(tabID: targetTabID) {
            return updatePinnedTabSnapshot(tabID: targetTabID)
        }

        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .pinTab(tabID: targetTabID)
            )
        } catch {
            return false
        }
        applyMutationResult(result, pinSnapshotTabIDs: [targetTabID])
        recordControlPlaneDiagnostic("workspace manifest pinned tab: \(targetTabID)")
        return true
    }

    @discardableResult
    func unpinTab(tabID: String? = nil) -> Bool {
        guard let targetTabID = tabID ?? selectedTabID else { return false }
        guard isTabPinned(tabID: targetTabID) else { return true }
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .unpinTab(tabID: targetTabID)
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        recordControlPlaneDiagnostic("workspace manifest unpinned tab: \(targetTabID)")
        return true
    }

    @discardableResult
    func updatePinnedTabSnapshot(tabID: String? = nil) -> Bool {
        guard let targetTabID = tabID ?? selectedTabID else { return false }
        guard isTabPinned(tabID: targetTabID) else { return false }
        return updateWorkspaceManifestTab(tabID: targetTabID) { tab, snapshot in
            tab.pinSnapshot = snapshot
            tab.liveSnapshot = snapshot
        } diagnostic: {
            "workspace manifest updated pinned tab: \($0)"
        }
    }

    @discardableResult
    func reorderTab(
        tabID: String,
        targetSpaceID: String? = nil,
        section: ShellTabOrganizationSection,
        index: Int
    ) -> Bool {
        let wasPinned = isTabPinned(tabID: tabID)
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .organizeTab(
                    tabID: tabID,
                    targetSpaceID: targetSpaceID,
                    section: section,
                    index: index
                )
            )
        } catch {
            return false
        }
        let needsPinSnapshot = !wasPinned && section == .pinned
        applyMutationResult(result, pinSnapshotTabIDs: needsPinSnapshot ? [tabID] : [])
        return true
    }

    @discardableResult
    func moveTab(tabID: String? = nil, offset: Int) -> Bool {
        guard let targetTabID = tabID ?? selectedTabID else { return false }
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .moveTab(tabID: targetTabID, sectionOffset: offset)
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func moveTabToSpace(tabID: String, targetSpaceID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .moveTabToSpace(
                    tabID: tabID,
                    targetSpaceID: targetSpaceID
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func renameTab(tabID: String, title: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .renameTab(
                    tabID: tabID,
                    title: title
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func duplicateTab(tabID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .duplicateTab(
                    tabID: tabID,
                    reservedPaneSlotIDs: terminalRuntimeRegistry.registeredPaneIDs.sorted()
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func openTabInSplitView(tabID: String) -> Bool {
        guard let tab = shellState.tab(tabID: tabID),
              let paneID = tab.contains(paneID: shellState.focusedPaneID ?? "")
                ? shellState.focusedPaneID
                : tab.paneTree.paneIDs.first,
              shellState.terminalBackedPane(paneID: paneID) != nil
        else {
            return false
        }

        select(tabID: tabID)
        let result: ShellStateMutationResult
        do {
            let sourcePane = pane(paneID: paneID)
            let terminalProfileID = sourcePane?.terminalProfileID
                ?? selectedSpace?.terminalProfileID
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .splitPane(
                    paneSlotID: paneID,
                    placement: .right,
                    title: nil,
                    workingDirectory: terminalProfileID == nil ? sourcePane?.cwd : nil,
                    terminalProfileID: terminalProfileID,
                    reservedPaneSlotIDs: terminalRuntimeRegistry.registeredPaneIDs.sorted()
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        refocusSelectedTerminalPane()
        return true
    }

    func clearableInactiveTabCount(in spaceID: String) -> Int {
        (try? shellState.clearableInactiveTemporaryTabIDs(
            in: spaceID,
            activeTaskByTabID: activeTaskByTabID()
        ).count) ?? 0
    }

    @discardableResult
    func clearInactiveTemporaryTabs(in spaceID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            let protectedTabIDs = activeTaskByTabID().compactMap { tabID, activeTask in
                activeTask.protectsFromPruning ? tabID : nil
            }
            result = try reducerAdapter.apply(
                state: shellState,
                operation: .clearInactiveTemporaryTabs(
                    spaceID: spaceID,
                    protectedTabIDs: protectedTabIDs
                )
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    @discardableResult
    func openContentTab(
        _ contentIntent: ShellContentIntent = .terminal(
            launchTarget: .shell,
            title: nil,
            workingDirectory: nil
        ),
        in spaceID: String? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        let result: ShellStateMutationResult
        do {
            let reservedPaneSlotIDs = terminalRuntimeRegistry.registeredPaneIDs.sorted()
            switch contentIntent {
            case .terminal(let launchTarget, let title, let workingDirectory):
                switch launchTarget {
                case .shell:
                    let resolvedTerminalProfileID = targetTerminalProfileID(
                        in: spaceID,
                        explicit: terminalProfileID
                    )
                    let resolvedWorkingDirectory =
                        workingDirectory
                        ?? (resolvedTerminalProfileID == nil
                            ? focusedPaneWorkingDirectory()
                            : nil)
                    result = try reducerAdapter.apply(
                        state: shellState,
                        operation: .openTerminalTab(
                            spaceID: spaceID,
                            title: title,
                            workingDirectory: resolvedWorkingDirectory,
                            terminalProfileID: resolvedTerminalProfileID,
                            reservedPaneSlotIDs: reservedPaneSlotIDs
                        )
                    )
                }
            case .markdown(let fileURL, let title):
                let content = markdownContentDescriptor(fileURL: fileURL, title: title)
                result = try reducerAdapter.apply(
                    state: shellState,
                    operation: .openContentTab(
                        spaceID: spaceID,
                        kind: .markdown,
                        title: content.title,
                        payload: content.payload,
                        reservedPaneSlotIDs: reservedPaneSlotIDs
                    )
                )
            case .settings(let title):
                let content = settingsContentDescriptor(title: title)
                result = try reducerAdapter.apply(
                    state: shellState,
                    operation: .openContentTab(
                        spaceID: spaceID,
                        kind: .settings,
                        title: content.title,
                        payload: content.payload,
                        reservedPaneSlotIDs: reservedPaneSlotIDs
                    )
                )
            case .agent(let attachment, let title):
                result = try reducerAdapter.apply(
                    state: shellState,
                    operation: .openContentTab(
                        spaceID: spaceID,
                        kind: .agent,
                        title: title ?? "Agent \(attachment.process.pid)",
                        payload: .agent(attachment),
                        reservedPaneSlotIDs: reservedPaneSlotIDs
                    )
                )
            }
        } catch {
            return nil
        }
        applyMutationResult(result)
        return result.tabID
    }

    @discardableResult
    func openTab(
        launchTarget: ShellLaunchTarget = .shell,
        in spaceID: String? = nil,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        do {
            let result = try openTabMutation(
                launchTarget: launchTarget,
                in: spaceID,
                title: title,
                workingDirectory: workingDirectory,
                terminalProfileID: terminalProfileID
            )
            applyMutationResult(result)
            return result.tabID
        } catch {
            return nil
        }
    }

    private func openTabMutation(
        launchTarget: ShellLaunchTarget = .shell,
        in spaceID: String? = nil,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil
    ) throws -> ShellStateMutationResult {
        switch launchTarget {
        case .shell:
            return try reducerAdapter.apply(
                state: shellState,
                operation: .openTerminalTab(
                    spaceID: spaceID,
                    title: title,
                    workingDirectory: workingDirectory,
                    terminalProfileID: terminalProfileID,
                    reservedPaneSlotIDs: terminalRuntimeRegistry.registeredPaneIDs.sorted()
                )
            )
        }
    }

    @discardableResult
    func openTerminalTab(
        in spaceID: String? = nil,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        let resolvedTerminalProfileID = targetTerminalProfileID(
            in: spaceID,
            explicit: terminalProfileID
        )
        let resolvedWorkingDirectory =
            workingDirectory
            ?? (resolvedTerminalProfileID == nil
                ? focusedPaneWorkingDirectory()
                : nil)
        return openTab(
            launchTarget: .shell,
            in: spaceID,
            title: title,
            workingDirectory: resolvedWorkingDirectory,
            terminalProfileID: resolvedTerminalProfileID
        )
    }

    func openTerminalTabMutation(
        in spaceID: String? = nil,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil
    ) throws -> ShellStateMutationResult {
        let resolvedTerminalProfileID = targetTerminalProfileID(
            in: spaceID,
            explicit: terminalProfileID
        )
        let resolvedWorkingDirectory =
            workingDirectory
            ?? (resolvedTerminalProfileID == nil
                ? focusedPaneWorkingDirectory()
                : nil)
        return try openTabMutation(
            launchTarget: .shell,
            in: spaceID,
            title: title,
            workingDirectory: resolvedWorkingDirectory,
            terminalProfileID: resolvedTerminalProfileID
        )
    }

    @discardableResult
    func openMarkdownTab(
        fileURL: URL,
        in spaceID: String? = nil,
        title: String? = nil
    ) -> String? {
        openContentTab(
            .markdown(fileURL: fileURL, title: title),
            in: spaceID
        )
    }

    @discardableResult
    func openSettingsTab(
        in spaceID: String? = nil,
        title: String? = nil
    ) -> String? {
        openContentTab(
            .settings(title: title),
            in: spaceID
        )
    }

    @discardableResult
    func openAgentTab(
        attachment: AlanAgentAttachment,
        in spaceID: String? = nil,
        title: String? = nil
    ) -> String? {
        openContentTab(.agent(attachment: attachment, title: title), in: spaceID)
    }

    @discardableResult
    func splitFocusedPane(
        direction: ShellSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        splitFocusedPane(
            placement: .defaultPlacement(for: direction),
            contentIntent: contentIntent,
            terminalProfileID: terminalProfileID
        )
    }

    @discardableResult
    func splitFocusedPane(
        placement: ShellPaneSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        guard let focusedPaneID = shellState.focusedPaneID else { return nil }
        return splitPane(
            paneID: focusedPaneID,
            placement: placement,
            contentIntent: contentIntent,
            terminalProfileID: terminalProfileID
        )
    }

    @discardableResult
    func splitPane(
        paneID: String,
        direction: ShellSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        splitPane(
            paneID: paneID,
            placement: .defaultPlacement(for: direction),
            contentIntent: contentIntent,
            terminalProfileID: terminalProfileID
        )
    }

    @discardableResult
    func splitPane(
        paneID: String,
        placement: ShellPaneSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil
    ) -> String? {
        let resolvedTerminalProfileID = targetTerminalProfileID(
            forSplitFromPaneID: paneID,
            explicit: terminalProfileID
        )
        let result: ShellStateMutationResult
        do {
            let reservedPaneSlotIDs = terminalRuntimeRegistry.registeredPaneIDs.sorted()
            if let contentIntent {
                switch contentIntent {
                case .terminal(let launchTarget, let title, let workingDirectory):
                    switch launchTarget {
                    case .shell:
                        result = try reducerAdapter.apply(
                            state: shellState,
                            operation: .splitPane(
                                paneSlotID: paneID,
                                placement: placement,
                                title: title,
                                workingDirectory: workingDirectory
                                    ?? (resolvedTerminalProfileID == nil
                                        ? pane(paneID: paneID)?.cwd
                                        : nil),
                                terminalProfileID: resolvedTerminalProfileID,
                                reservedPaneSlotIDs: reservedPaneSlotIDs
                            )
                        )
                    }
                case .markdown(let fileURL, let title):
                    let content = markdownContentDescriptor(fileURL: fileURL, title: title)
                    result = try reducerAdapter.apply(
                        state: shellState,
                        operation: .splitContentPane(
                            paneSlotID: paneID,
                            placement: placement,
                            kind: .markdown,
                            title: content.title,
                            payload: content.payload,
                            reservedPaneSlotIDs: reservedPaneSlotIDs
                        )
                    )
                case .settings(let title):
                    let content = settingsContentDescriptor(title: title)
                    result = try reducerAdapter.apply(
                        state: shellState,
                        operation: .splitContentPane(
                            paneSlotID: paneID,
                            placement: placement,
                            kind: .settings,
                            title: content.title,
                            payload: content.payload,
                            reservedPaneSlotIDs: reservedPaneSlotIDs
                        )
                    )
                case .agent(let attachment, let title):
                    result = try reducerAdapter.apply(
                        state: shellState,
                        operation: .splitContentPane(
                            paneSlotID: paneID,
                            placement: placement,
                            kind: .agent,
                            title: title ?? "Agent \(attachment.process.pid)",
                            payload: .agent(attachment),
                            reservedPaneSlotIDs: reservedPaneSlotIDs
                        )
                    )
                }
            } else {
                result = try reducerAdapter.apply(
                    state: shellState,
                    operation: .splitPane(
                        paneSlotID: paneID,
                        placement: placement,
                        title: nil,
                        workingDirectory: resolvedTerminalProfileID == nil
                            ? pane(paneID: paneID)?.cwd
                            : nil,
                        terminalProfileID: resolvedTerminalProfileID,
                        reservedPaneSlotIDs: reservedPaneSlotIDs
                    )
                )
            }
        } catch {
            return nil
        }
        applyMutationResult(result)
        return result.paneID
    }

    private func markdownContentDescriptor(
        fileURL: URL,
        title: String?
    ) -> (title: String, payload: ShellContentPayload) {
        let resolvedURL = fileURL.isFileURL ? fileURL.standardizedFileURL : fileURL
        let resolvedTitle = Self.markdownContentTitle(for: resolvedURL, explicitTitle: title)
        return (
            title: resolvedTitle,
            payload: .markdown(
                ShellMarkdownContentPayload(
                    fileURL: resolvedURL.absoluteString,
                    title: resolvedTitle
                )
            )
        )
    }

    private func settingsContentDescriptor(
        title: String?
    ) -> (title: String, payload: ShellContentPayload) {
        let resolvedTitle = Self.settingsContentTitle(explicitTitle: title)
        return (
            title: resolvedTitle,
            payload: .settings(
                ShellSettingsContentPayload(
                    surfaceID: ShellContentInstance.settingsSurfaceID,
                    title: resolvedTitle
                )
            )
        )
    }

    private static func markdownContentTitle(for fileURL: URL, explicitTitle: String?) -> String {
        if let title = explicitTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        let lastPathComponent = fileURL.lastPathComponent.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        return lastPathComponent.isEmpty ? "Markdown" : lastPathComponent
    }

    private static func settingsContentTitle(explicitTitle: String?) -> String {
        if let title = explicitTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        return "Settings"
    }

}
#endif
