import Foundation

struct ShellCorePortableWorkspaceState: Codable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [ShellCorePortableSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [ShellCorePortableContentInstance]
    let quickTerminal: ShellCorePortableQuickTerminalState?

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
        case quickTerminal = "quick_terminal"
    }

    init(projecting state: ShellStateSnapshot) {
        let contentState = state.contentStateProjection()
        let projectedQuickTerminal = ShellCorePortableQuickTerminalState(projecting: state)
        contractVersion = contentState.contractVersion
        windowID = contentState.windowID
        focusedSpaceID = contentState.focusedSpaceID
        focusedTabID = contentState.focusedTabID
        if projectedQuickTerminal?.paneID == state.focusedPaneID {
            focusedPaneID = state.focusedPaneID
        } else {
            focusedPaneID = contentState.focusedPaneSlotID
        }
        spaces = contentState.spaces.map(ShellCorePortableSpace.init(contentSpace:))
        paneSlots = contentState.paneSlots
        contents = contentState.contents.map(ShellCorePortableContentInstance.init(contentInstance:))
        quickTerminal = projectedQuickTerminal
    }

    func materializedShellState() throws -> ShellStateSnapshot {
        let contentState = ShellContentStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneSlotID: focusedPaneID,
            spaces: spaces.map(\.contentSpace),
            paneSlots: paneSlots,
            contents: contents.map(\.contentInstance)
        )
        guard var shellState = contentState.materializingShellState() else {
            throw ShellCoreFFIAdapterError.materializationFailed(
                "portable workspace state could not be projected into shell state"
            )
        }
        guard let quickTerminal,
              let restoredQuickTerminal = quickTerminal.materialized()
        else {
            return shellState
        }
        guard !shellState.panes.contains(where: { $0.paneID == restoredQuickTerminal.pane.paneID }) else {
            return ShellStateSnapshot(
                contractVersion: shellState.contractVersion,
                windowID: shellState.windowID,
                focusedSpaceID: shellState.focusedSpaceID,
                focusedTabID: shellState.focusedTabID,
                focusedPaneID: shellState.focusedPaneID,
                spaces: shellState.spaces,
                panes: shellState.panes,
                paneSlots: shellState.paneSlots,
                contents: shellState.contents,
                quickTerminal: restoredQuickTerminal.slot
            )
        }

        var materializedContents = shellState.contents ?? []
        if !materializedContents.contains(where: { $0.contentID == restoredQuickTerminal.content.contentID }) {
            materializedContents.append(restoredQuickTerminal.content)
        }
        shellState = ShellStateSnapshot(
            contractVersion: shellState.contractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: shellState.focusedSpaceID,
            focusedTabID: shellState.focusedTabID,
            focusedPaneID: shellState.focusedPaneID,
            spaces: shellState.spaces,
            panes: shellState.panes + [restoredQuickTerminal.pane],
            paneSlots: shellState.paneSlots,
            contents: materializedContents.isEmpty ? nil : materializedContents,
            quickTerminal: restoredQuickTerminal.slot
        )
        return shellState
    }
}

struct ShellCorePortableSpace: Codable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellCorePortableTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIconSystemName: String?

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIconSystemName = "presentation_icon"
    }

    init(contentSpace: ShellContentSpace) {
        spaceID = contentSpace.spaceID
        title = contentSpace.title
        attention = contentSpace.attention
        tabs = contentSpace.tabs.map(ShellCorePortableTab.init(contentTab:))
        selectedTabID = contentSpace.selectedTabID
        terminalProfileID = contentSpace.terminalProfileID
        presentationIconSystemName = contentSpace.presentationIconSystemName
    }

    var contentSpace: ShellContentSpace {
        ShellContentSpace(
            spaceID: spaceID,
            title: title,
            attention: attention,
            tabs: tabs.map(\.contentTab),
            selectedTabID: selectedTabID,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: presentationIconSystemName
        )
    }
}

struct ShellCorePortableTab: Codable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    init(contentTab: ShellContentTab) {
        tabID = contentTab.tabID
        kind = contentTab.kind
        title = contentTab.title
        paneTree = contentTab.paneTree.restoringPaneTree()
        isPinned = contentTab.isPinned
        isTitleUserLocked = contentTab.isTitleUserLocked
    }

    var contentTab: ShellContentTab {
        ShellContentTab(
            tabID: tabID,
            kind: kind,
            title: title,
            paneTree: ShellPaneSlotTreeNode.migrating(paneTree: paneTree),
            isPinned: isPinned,
            isTitleUserLocked: isTitleUserLocked
        )
    }

    var shellTab: ShellTab {
        ShellTab(
            tabID: tabID,
            kind: kind,
            title: title,
            paneTree: paneTree,
            isPinned: isPinned,
            isTitleUserLocked: isTitleUserLocked
        )
    }
}

struct ShellCorePortableContentInstance: Codable {
    let contentID: String
    let kind: ShellContentKind
    let title: String
    let iconName: String?
    let capabilities: [ShellContentCapability]
    let payload: ShellContentPayload
    let lifecycle: ShellContentLifecycleState

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case iconName = "icon_name"
        case capabilities
        case payload
        case lifecycle
    }

    init(contentInstance: ShellContentInstance) {
        contentID = contentInstance.contentID
        kind = contentInstance.kind
        title = contentInstance.title
        iconName = contentInstance.iconName
        capabilities = contentInstance.capabilities
        payload = contentInstance.payload
        lifecycle = contentInstance.lifecycle
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        contentID = try container.decode(String.self, forKey: .contentID)
        kind = try container.decode(ShellContentKind.self, forKey: .kind)
        title = try container.decode(String.self, forKey: .title)
        iconName = try container.decodeIfPresent(String.self, forKey: .iconName)
        capabilities = try container.decodeIfPresent(
            [ShellContentCapability].self,
            forKey: .capabilities
        ) ?? ShellContentInstance.defaultCapabilities(for: kind)
        payload = try container.decodeIfPresent(ShellContentPayload.self, forKey: .payload)
            ?? ShellContentPayload(terminal: nil, markdown: nil, settings: nil)
        lifecycle = try container.decodeIfPresent(
            ShellContentLifecycleState.self,
            forKey: .lifecycle
        ) ?? .active
    }

    var contentInstance: ShellContentInstance {
        ShellContentInstance(
            contentID: contentID,
            kind: kind,
            title: title,
            iconName: iconName,
            capabilities: capabilities,
            payload: payload,
            lifecycle: lifecycle,
            rendererState: Self.materializedRendererState(kind: kind, payload: payload)
        )
    }

    /// shell-core's portable contents do not carry the Swift-only `rendererState`. Terminal
    /// renderer state is recomputed from live pane context during content projection, but
    /// markdown/settings contents have no runtime to repopulate it, so reconstruct the same
    /// "ready" state the native mount path assigns instead of leaving them at `.placeholder`
    /// (which would otherwise report non-terminal panes as not ready in the event stream).
    private static func materializedRendererState(
        kind: ShellContentKind,
        payload: ShellContentPayload
    ) -> ShellContentRendererState {
        switch kind {
        case .terminal:
            return .placeholder
        case .markdown:
            let detail = payload.markdown.flatMap { URL(string: $0.fileURL)?.path }
            return ShellContentRendererState(phase: "ready", detail: detail)
        case .settings:
            return ShellContentRendererState(phase: "ready", detail: payload.settings?.surfaceID)
        }
    }
}

struct ShellCorePortableQuickTerminalState: Codable {
    let paneID: String
    let presentation: ShellQuickTerminalPresentation
    let lastWorkingDirectory: String?
    let contentID: String
    let terminalPayload: ShellTerminalContentPayload?
    let terminalMetadata: ShellCorePortableTerminalMetadata?
    let attention: ShellAttentionState

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
        case contentID = "content_id"
        case terminalPayload = "terminal_payload"
        case terminalMetadata = "terminal_metadata"
        case attention
    }

    init?(projecting state: ShellStateSnapshot) {
        guard let slot = state.quickTerminal else { return nil }
        let pane = state.panes.first { $0.paneID == slot.paneID }
        let contentID = pane?.terminalContentID
            ?? ShellContentInstance.terminalContentID(forPaneID: slot.paneID)
        let content = state.contents?.first { $0.contentID == contentID }
        let terminalPayload = content?.payload.terminal ?? pane.map {
            ShellTerminalContentPayload(
                launchTarget: $0.resolvedLaunchTarget,
                cwd: $0.cwd,
                title: $0.viewport?.title,
                terminalProfileID: $0.terminalProfileID
            )
        }

        self.paneID = slot.paneID
        presentation = slot.presentation
        lastWorkingDirectory = slot.lastWorkingDirectory
        self.contentID = contentID
        self.terminalPayload = terminalPayload
        terminalMetadata = pane.map {
            ShellCorePortableTerminalMetadata(
                title: $0.viewport?.title,
                cwd: $0.cwd,
                activity: $0.activity
            )
        }
        attention = pane?.attention ?? .idle
    }

    func materialized() -> (
        slot: ShellQuickTerminalSlot,
        pane: ShellPane,
        content: ShellContentInstance
    )? {
        guard let terminalPayload else { return nil }
        let title = terminalPayload.title ?? terminalMetadata?.title ?? "Shell"
        let payload = ShellTerminalContentPayload(
            launchTarget: terminalPayload.launchTarget,
            cwd: terminalPayload.cwd ?? terminalMetadata?.cwd,
            title: terminalPayload.title ?? terminalMetadata?.title,
            transcriptSnapshot: terminalPayload.transcriptSnapshot,
            terminalProfileID: terminalPayload.terminalProfileID
        )
        let content = ShellContentInstance(
            contentID: contentID,
            kind: .terminal,
            title: title,
            payload: .terminal(payload),
            rendererState: .placeholder
        )
        let pane = ShellPane(
            paneID: paneID,
            tabID: ShellQuickTerminalSlot.globalTabID,
            spaceID: ShellQuickTerminalSlot.globalSpaceID,
            launchTarget: payload.launchTarget,
            cwd: payload.cwd,
            process: nil,
            attention: attention,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: title,
                summary: nil,
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            activity: terminalMetadata?.activity,
            alanBinding: nil,
            terminalProfileID: payload.terminalProfileID
        )
        let slot = ShellQuickTerminalSlot(
            paneID: paneID,
            presentation: presentation,
            lastWorkingDirectory: lastWorkingDirectory ?? payload.cwd
        )
        return (slot, pane, content)
    }
}

struct ShellCorePortableTerminalMetadata: Codable {
    let title: String?
    let cwd: String?
    let activity: TerminalActivitySnapshot?
}

extension ShellStateSnapshot {
    func preservingPlatformPaneFields(from authoritative: ShellStateSnapshot) -> ShellStateSnapshot {
        let authoritativePanesByID = Dictionary(
            uniqueKeysWithValues: authoritative.panes.map { ($0.paneID, $0) }
        )
        let mergedPanes = panes.map { pane in
            pane.preservingPlatformFields(from: authoritativePanesByID[pane.paneID])
        }

        return ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneID: focusedPaneID,
            spaces: spaces,
            panes: mergedPanes,
            paneSlots: paneSlots,
            contents: contents,
            quickTerminal: quickTerminal
        )
    }
}

extension ShellPane {
    func preservingPlatformFields(from authoritative: ShellPane?) -> ShellPane {
        guard let authoritative else { return self }

        return ShellPane(
            paneID: paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: launchTarget ?? authoritative.launchTarget,
            cwd: cwd ?? authoritative.cwd,
            process: process ?? authoritative.process,
            attention: attention,
            context: context.preservingPlatformFields(from: authoritative.context),
            viewport: viewport.preservingPlatformFields(from: authoritative.viewport),
            activity: activity ?? authoritative.activity,
            alanBinding: alanBinding ?? authoritative.alanBinding,
            terminalProfileID: terminalProfileID ?? authoritative.terminalProfileID
        )
    }
}

extension Optional where Wrapped == ShellContextSnapshot {
    func preservingPlatformFields(from authoritative: ShellContextSnapshot?) -> ShellContextSnapshot? {
        guard self != nil || authoritative != nil else { return nil }

        return ShellContextSnapshot(
            workingDirectoryName: self?.workingDirectoryName ?? authoritative?.workingDirectoryName,
            repositoryRoot: self?.repositoryRoot ?? authoritative?.repositoryRoot,
            gitBranch: self?.gitBranch ?? authoritative?.gitBranch,
            controlPath: self?.controlPath ?? authoritative?.controlPath,
            socketPath: self?.socketPath ?? authoritative?.socketPath,
            alanBindingFile: self?.alanBindingFile ?? authoritative?.alanBindingFile,
            launchCommand: self?.launchCommand ?? authoritative?.launchCommand,
            launchStrategy: self?.launchStrategy ?? authoritative?.launchStrategy,
            terminalProfileState: self?.terminalProfileState ?? authoritative?.terminalProfileState,
            terminalProfileRequestedID: self?.terminalProfileRequestedID
                ?? authoritative?.terminalProfileRequestedID,
            terminalProfileID: self?.terminalProfileID ?? authoritative?.terminalProfileID,
            terminalProfileKind: self?.terminalProfileKind ?? authoritative?.terminalProfileKind,
            terminalProfileTitle: self?.terminalProfileTitle ?? authoritative?.terminalProfileTitle,
            shellIntegrationSource: self?.shellIntegrationSource
                ?? authoritative?.shellIntegrationSource,
            processState: self?.processState ?? authoritative?.processState,
            rendererPhase: self?.rendererPhase ?? authoritative?.rendererPhase,
            rendererHealth: self?.rendererHealth ?? authoritative?.rendererHealth,
            surfaceReadiness: self?.surfaceReadiness ?? authoritative?.surfaceReadiness,
            inputReady: self?.inputReady ?? authoritative?.inputReady,
            readonly: self?.readonly ?? authoritative?.readonly,
            terminalMode: self?.terminalMode ?? authoritative?.terminalMode,
            displayName: self?.displayName ?? authoritative?.displayName,
            displayID: self?.displayID ?? authoritative?.displayID,
            windowTitle: self?.windowTitle ?? authoritative?.windowTitle,
            lastMetadataAt: self?.lastMetadataAt ?? authoritative?.lastMetadataAt,
            lastCommandExitCode: self?.lastCommandExitCode ?? authoritative?.lastCommandExitCode
        )
    }
}

extension Optional where Wrapped == ShellViewportSnapshot {
    func preservingPlatformFields(from authoritative: ShellViewportSnapshot?) -> ShellViewportSnapshot? {
        guard self != nil || authoritative != nil else { return nil }

        return ShellViewportSnapshot(
            title: self?.title ?? authoritative?.title,
            summary: self?.summary ?? authoritative?.summary,
            visibleExcerpt: self?.visibleExcerpt ?? authoritative?.visibleExcerpt,
            lastActivityAt: self?.lastActivityAt ?? authoritative?.lastActivityAt
        )
    }
}
