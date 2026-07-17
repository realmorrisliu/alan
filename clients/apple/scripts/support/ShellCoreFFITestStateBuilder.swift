import Foundation

extension ShellStateSnapshot {
    func focusingPane(_ paneID: String) throws -> ShellStateMutationResult {
        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .focusPane(paneSlotID: paneID)
        )
    }

    func creatingSpace(
        launchTarget: ShellLaunchTarget = .shell,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = FileManager.default.homeDirectoryForCurrentUser.path,
        now: Date = .now
    ) -> ShellStateMutationResult {
        creatingTerminalSpace(
            title: title,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: presentationIconSystemName,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func creatingTerminalSpace(
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = FileManager.default.homeDirectoryForCurrentUser.path,
        now: Date = .now
    ) -> ShellStateMutationResult {
        do {
            return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
                state: self,
                operation: .createTerminalSpace(
                    title: title,
                    tabTitle: nil,
                    workingDirectory: workingDirectory,
                    terminalProfileID: terminalProfileID,
                    presentationIcon: presentationIconSystemName,
                    reservedPaneSlotIDs: reservedPaneIDs.sorted()
                )
            )
        } catch {
            return ShellStateMutationResult(
                state: self,
                spaceID: focusedSpaceID,
                tabID: focusedTabID,
                paneID: focusedPaneID
            )
        }
    }

    func settingTerminalProfile(
        _ terminalProfileID: String?,
        forSpaceID targetSpaceID: String
    ) -> ShellStateSnapshot? {
        try? ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .setTerminalProfile(
                spaceID: targetSpaceID,
                terminalProfileID: terminalProfileID
            )
        ).state
    }

    func openingTerminalTab(
        in requestedSpaceID: String?,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = FileManager.default.homeDirectoryForCurrentUser.path,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        let resolvedTerminalProfileID = terminalProfileIDForNewTerminal(
            in: requestedSpaceID,
            explicit: terminalProfileID
        )
        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .openTerminalTab(
                spaceID: requestedSpaceID,
                title: title,
                workingDirectory: workingDirectory,
                terminalProfileID: resolvedTerminalProfileID,
                reservedPaneSlotIDs: reservedPaneIDs.sorted()
            )
        )
    }

    func openingMarkdownTab(
        fileURL: URL,
        in requestedSpaceID: String?,
        title: String?,
        reservedPaneIDs: Set<String> = [],
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .openContentTab(
                spaceID: requestedSpaceID,
                kind: .markdown,
                title: title ?? fileURL.lastPathComponent,
                payload: .markdown(
                    ShellMarkdownContentPayload(fileURL: fileURL.absoluteString, title: title)
                ),
                reservedPaneSlotIDs: reservedPaneIDs.sorted()
            )
        )
    }

    func openingSettingsTab(
        in requestedSpaceID: String?,
        title: String?,
        reservedPaneIDs: Set<String> = [],
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .openContentTab(
                spaceID: requestedSpaceID,
                kind: .settings,
                title: title ?? "Settings",
                payload: .settings(
                    ShellSettingsContentPayload(
                        surfaceID: ShellContentInstance.settingsSurfaceID,
                        title: title ?? "Settings"
                    )
                ),
                reservedPaneSlotIDs: reservedPaneIDs.sorted()
            )
        )
    }

    func splittingPane(
        _ paneID: String,
        direction: ShellSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = FileManager.default.homeDirectoryForCurrentUser.path,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        try splittingPane(
            paneID,
            placement: .defaultPlacement(for: direction),
            contentIntent: contentIntent,
            terminalProfileID: terminalProfileID,
            reservedPaneIDs: reservedPaneIDs,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    func splittingPane(
        _ paneID: String,
        placement: ShellPaneSplitDirection,
        contentIntent: ShellContentIntent? = nil,
        terminalProfileID: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = FileManager.default.homeDirectoryForCurrentUser.path,
        now: Date = .now
    ) throws -> ShellStateMutationResult {
        let resolvedTerminalProfileID = terminalProfileIDForNewSplit(
            from: paneID,
            explicit: terminalProfileID
        )
        if let contentIntent {
            return try splitContentPane(
                paneID,
                placement: placement,
                contentIntent: contentIntent,
                reservedPaneIDs: reservedPaneIDs
            )
        }
        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .splitPane(
                paneSlotID: paneID,
                placement: placement,
                title: nil,
                workingDirectory: nil,
                terminalProfileID: resolvedTerminalProfileID,
                reservedPaneSlotIDs: reservedPaneIDs.sorted()
            )
        )
    }

    func pinningTab(_ tabID: String) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(state: self, operation: .pinTab(tabID: tabID))
    }

    func renamingTab(_ tabID: String, title: String) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .renameTab(tabID: tabID, title: title)
        )
    }

    func settingAutomaticTabTitle(_ tabID: String, title: String?) throws -> ShellStateMutationResult {
        guard let tab = tab(tabID: tabID) else {
            throw ShellStateMutationError.tabNotFound
        }
        guard !tab.isTitleUserLocked else {
            return ShellStateMutationResult(
                state: self,
                spaceID: focusedSpaceID,
                tabID: focusedTabID,
                paneID: focusedPaneID
            )
        }
        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .renameTab(tabID: tabID, title: title ?? "Shell")
        )
    }

    func clearingInactiveTemporaryTabs(
        in spaceID: String,
        activeTaskByTabID: [String: ShellTabActiveTaskState] = [:]
    ) throws -> ShellStateMutationResult {
        let protectedTabIDs = activeTaskByTabID.compactMap { tabID, activeTask in
            activeTask.protectsFromPruning ? tabID : nil
        }
        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .clearInactiveTemporaryTabs(
                spaceID: spaceID,
                protectedTabIDs: protectedTabIDs.sorted()
            )
        )
    }

    func movingTabToSpace(
        tabID: String,
        targetSpaceID: String
    ) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .moveTabToSpace(tabID: tabID, targetSpaceID: targetSpaceID)
        )
    }

    func closingTab(_ tabID: String) throws -> ShellStateMutationResult {
        try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(state: self, operation: .closeTab(tabID: tabID))
    }

    private func splitContentPane(
        _ paneID: String,
        placement: ShellPaneSplitDirection,
        contentIntent: ShellContentIntent,
        reservedPaneIDs: Set<String>
    ) throws -> ShellStateMutationResult {
        let kind: ShellContentKind
        let title: String
        let payload: ShellContentPayload
        switch contentIntent {
        case .terminal:
            return try splittingPane(
                paneID,
                placement: placement,
                reservedPaneIDs: reservedPaneIDs
            )
        case .markdown(let fileURL, let requestedTitle):
            kind = .markdown
            title = requestedTitle ?? fileURL.lastPathComponent
            payload = .markdown(
                ShellMarkdownContentPayload(fileURL: fileURL.absoluteString, title: requestedTitle)
            )
        case .settings(let requestedTitle):
            kind = .settings
            title = requestedTitle ?? "Settings"
            payload = .settings(
                ShellSettingsContentPayload(
                    surfaceID: ShellContentInstance.settingsSurfaceID,
                    title: title
                )
            )
        case .agent(let attachment, let requestedTitle):
            kind = .agent
            title = requestedTitle ?? "Agent \(attachment.process.pid)"
            payload = .agent(attachment)
        }

        return try ShellCoreReducerAdapter(adapter: ShellCoreFFIAdapter()).apply(
            state: self,
            operation: .splitContentPane(
                paneSlotID: paneID,
                placement: placement,
                kind: kind,
                title: title,
                payload: payload,
                reservedPaneSlotIDs: reservedPaneIDs.sorted()
            )
        )
    }
}
