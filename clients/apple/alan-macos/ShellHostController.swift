import Foundation

#if os(macOS)
import AppKit
import SwiftUI

struct ShellAttentionItem: Identifiable, Equatable {
    let paneID: String
    let spaceID: String
    let tabID: String
    let title: String
    let summary: String
    let attention: ShellAttentionState

    var id: String { paneID }
}

enum ShellTabCloseResult: Equatable {
    case closed
    case tabNotFound
    case lastTab
    case requiresConfirmation(ShellCloseGuardImpact)
}

enum ShellPaneCloseResult: Equatable {
    case closed
    case paneNotFound
    case lastTab
    case requiresConfirmation(ShellCloseGuardImpact)
}

enum ShellCloseGuardScope: Equatable {
    case paneSlot(String)
    case tab(String)
    case window
    case app
}

struct ShellCloseGuardImpact: Equatable {
    let scope: ShellCloseGuardScope
    let affectedTerminalContentIDs: [String]
    let activeTerminalContentIDs: [String]

    var requiresConfirmation: Bool {
        !activeTerminalContentIDs.isEmpty
    }
}

@MainActor
protocol ShellCloseConfirmationPresenting: AnyObject {
    func confirmClose(impact: ShellCloseGuardImpact) -> Bool
}

@MainActor
final class ShellNSAlertCloseConfirmationPresenter: ShellCloseConfirmationPresenting {
    func confirmClose(impact: ShellCloseGuardImpact) -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = closeTitle(for: impact.scope)
        alert.informativeText = closeMessage(for: impact)
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func closeTitle(for scope: ShellCloseGuardScope) -> String {
        switch scope {
        case .paneSlot:
            return "Close pane?"
        case .tab:
            return "Close tab?"
        case .window:
            return "Close window?"
        case .app:
            return "Quit alan?"
        }
    }

    private func closeMessage(for impact: ShellCloseGuardImpact) -> String {
        let count = impact.activeTerminalContentIDs.count
        let noun = count == 1 ? "terminal has" : "terminals have"
        return "\(count) \(noun) active work. Closing will stop the running process and save only restorable terminal history."
    }
}

enum ShellPaneLiftResult {
    case lifted
    case paneNotFound
    case lastPane
}

enum ShellPaneMovementInputSource: Equatable {
    case explicitCommand
    case titleBarDragAffordance
    case terminalContentDrag
}

struct ShellPaneMovementInteractionPolicy: Equatable {
    static let terminalSelectionFirst = ShellPaneMovementInteractionPolicy()

    func allowsPaneMovement(from source: ShellPaneMovementInputSource) -> Bool {
        switch source {
        case .explicitCommand, .titleBarDragAffordance:
            return true
        case .terminalContentDrag:
            return false
        }
    }
}

@MainActor
struct ShellWindowContext {
    let windowID: String
    let persistenceURL: URL
    let installChannel: AlanInstallChannel
    let terminalRuntimeRegistry: TerminalRuntimeRegistry

    var controlRootURL: URL {
        alanShellControlPlaneRootURL(windowID: windowID, channel: installChannel)
    }

    var socketURL: URL {
        alanShellControlPlaneSocketURL(windowID: windowID, channel: installChannel)
    }

    var stateURL: URL {
        controlRootURL.appendingPathComponent("state.json")
    }

    var eventsURL: URL {
        controlRootURL.appendingPathComponent("events.jsonl")
    }

    static func make(
        fileManager: FileManager = .default,
        windowID: String = "window_\(UUID().uuidString.lowercased())",
        installChannel: AlanInstallChannel = .current(),
        terminalRuntimeRegistry: TerminalRuntimeRegistry? = nil
    ) -> ShellWindowContext {
        ShellWindowContext(
            windowID: windowID,
            persistenceURL: ShellStatePersistenceStore.defaultPersistenceURL(
                windowID: windowID,
                fileManager: fileManager,
                channel: installChannel
            ),
            installChannel: installChannel,
            terminalRuntimeRegistry: terminalRuntimeRegistry ?? TerminalRuntimeRegistry()
        )
    }
}

@MainActor
final class ShellHostController: ObservableObject, TerminalHostActivationDelegate {
    enum StartupMode {
        case fresh
        case restorePrevious
        case workspaceManifest

        var workspaceStartupMode: ShellWorkspaceStartupMode {
            switch self {
            case .fresh:
                return .fresh
            case .restorePrevious:
                return .restorePrevious
            case .workspaceManifest:
                return .workspaceManifest
            }
        }
    }

    private static let gracefulShutdownPollInterval: TimeInterval = 0.05
    private static let iso8601Formatter = ISO8601DateFormatter()
    private let fileManager: FileManager
    private let windowContext: ShellWindowContext
    private let persistenceCoordinator: ShellWorkspacePersistenceCoordinator
    private let actionCoordinator = ShellActionCoordinator()
    let reducerCoordinator = ShellReducerCommandCoordinator()
    private var terminalActiveTasksByPaneID: [String: ShellTabActiveTaskState] = [:]
    private var terminalContentIDsSuppressingAutoClose: Set<String> = []
    private let paneProjection: ShellPaneProjectionService
    private let platformMetadataPreserver: ShellPlatformMetadataPreserver
    private let terminalContentProjection: TerminalContentProjectionAdapter
    private let terminalContentLifecycle = TerminalContentLifecycleAdapter()
    private let clipboardWriter: ShellClipboardWriter
    private let closeConfirmationPresenter: ShellCloseConfirmationPresenting
    private let gracefulShutdownTimeout: TimeInterval
    private let performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?
    lazy var controlPlane = AlanShellControlPlane(
        windowID: windowContext.windowID,
        channel: windowContext.installChannel
    ) { [weak self] command in
        self?.handleControlPlaneCommand(command)
            ?? AlanShellControlResponse(
                requestID: command.requestID,
                contractVersion: ShellContentStateSnapshot.currentContractVersion,
                applied: false,
                state: nil,
                spaces: nil,
                tabs: nil,
                panes: nil,
                pane: nil,
                items: nil,
                candidates: nil,
                events: nil,
                focusedPaneID: nil,
                spaceID: command.spaceID,
                tabID: command.tabID,
                paneID: command.paneID,
                acceptedBytes: nil,
                deliveryCode: nil,
                runtimePhase: nil,
                latestEventID: nil,
                errorCode: "host_unavailable",
                errorMessage: "alan terminal workspace host is unavailable."
            )
    } stateAdoptionHandler: { [weak self] state in
        self?.adoptStateFromControlPlane(state)
    } bindingProjectionHandler: { [weak self] paneID, binding in
        self?.applyAlanBinding(binding, for: paneID)
    } diagnosticHandler: { [weak self] message in
        self?.recordControlPlaneDiagnostic(message)
    }

    @Published private(set) var shellState: ShellStateSnapshot
    @Published var selectedSpaceID: String?
    @Published var selectedTabID: String?
    @Published private(set) var lastCopiedAt: Date?
    private(set) var terminalRuntime: TerminalHostRuntimeSnapshot = .placeholder
    @Published private(set) var controlPlaneDiagnostics: [String] = []
    @Published private(set) var activityNotifications: [ShellActivityNotificationRoute] = []
    @Published private(set) var zoomedPaneIDByTabID: [String: String] = [:]
    @Published var isPresentingSpaceCreation = false
    /// Live draft fields for the in-progress Space creation form. Published so
    /// the workspace can read the live name while the form is open.
    @Published var spaceDraftName: String = ""
    @Published var spaceDraftIcon: String? = nil
    @Published var spaceDraftProfileID: String? = nil

    func beginSpaceCreation() {
        // A second New Space press while the form is open is a no-op so an
        // in-progress draft (name/icon/profile) is never silently discarded.
        guard !isPresentingSpaceCreation else { return }
        spaceDraftName = ""
        spaceDraftIcon = nil
        spaceDraftProfileID = nil
        isPresentingSpaceCreation = true
    }

    func cancelSpaceCreation() {
        isPresentingSpaceCreation = false
        spaceDraftName = ""
        spaceDraftIcon = nil
        spaceDraftProfileID = nil
        // The selected terminal host was removed while the modal form was up.
        // SwiftUI reinserts it after the flag flips, but the reused view (same
        // pane/content, already "selected") won't re-request focus on its own,
        // so keyboard input would be lost until the user clicks the terminal.
        // Restore focus on the next runloop, once the terminal is back.
        DispatchQueue.main.async { [weak self] in
            self?.refocusSelectedTerminalPane()
        }
    }

    @discardableResult
    func createSpaceFromForm() -> String? {
        let name = spaceDraftName
        let iconSystemName = spaceDraftIcon
        let profileID = spaceDraftProfileID
        isPresentingSpaceCreation = false
        spaceDraftName = ""
        spaceDraftIcon = nil
        spaceDraftProfileID = nil
        let spaceID = createSpace(
            launchTarget: .shell,
            title: name,
            terminalProfileID: profileID,
            presentationIconSystemName: iconSystemName
        )
        if let spaceID {
            select(spaceID: spaceID)
        }
        return spaceID
    }

    let terminalRuntimeRegistry: TerminalRuntimeRegistry
    private let bootProfileCache: AlanShellBootProfileCache
    private let appIsActiveProvider: @MainActor () -> Bool
    private var routedActivityNotificationKeys: Set<String> = []
    private var pendingVisibleBackgroundRuntimeByPaneID: [String: TerminalHostRuntimeSnapshot] = [:]
    private var visibleBackgroundRuntimeProjectionScheduled = false
    private var shellWindowIsVisibleForRendering = true
    private var workspaceManifest: ShellContentWorkspaceManifest? {
        persistenceCoordinator.currentManifest()
    }

    init(
        shellState: ShellStateSnapshot,
        fileManager: FileManager = .default,
        windowContext: ShellWindowContext? = nil,
        persistenceURL: URL? = nil,
        terminalRuntimeRegistry: TerminalRuntimeRegistry? = nil,
        workspaceManifestStore: ShellWorkspaceManifestStore? = nil,
        workspaceManifest: ShellContentWorkspaceManifest? = nil,
        persistenceWriter: ShellPersistenceWriting? = nil,
        manifestFlushScheduler: ManifestFlushScheduling? = nil,
        closeConfirmationPresenter: ShellCloseConfirmationPresenting? = nil,
        gracefulShutdownTimeout: TimeInterval = 3.0,
        performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil,
        bootProfileCache: AlanShellBootProfileCache? = nil,
        appIsActiveProvider: @escaping @MainActor () -> Bool = { NSApp.isActive }
    ) {
        self.fileManager = fileManager
        let resolvedBootProfileCache = bootProfileCache ?? AlanShellBootProfileCache()
        self.bootProfileCache = resolvedBootProfileCache
        let paneProjection = ShellPaneProjectionService(fileManager: fileManager)
        self.paneProjection = paneProjection
        self.platformMetadataPreserver = ShellPlatformMetadataPreserver(
            paneProjection: paneProjection,
            bootProfileCache: resolvedBootProfileCache
        )
        self.terminalContentProjection = TerminalContentProjectionAdapter(
            paneProjection: paneProjection
        )
        let resolvedContext = windowContext ?? ShellWindowContext.make(fileManager: fileManager)
        self.windowContext = resolvedContext
        let resolvedPersistenceURL = persistenceURL ?? resolvedContext.persistenceURL
        let persistenceStore = ShellStatePersistenceStore(
            fileManager: fileManager,
            persistenceURL: resolvedPersistenceURL
        )
        self.persistenceCoordinator = ShellWorkspacePersistenceCoordinator(
            manifestStore: workspaceManifestStore,
            stateStore: persistenceStore,
            workspaceManifest: workspaceManifest,
            persistenceWriter: persistenceWriter,
            manifestFlushScheduler: manifestFlushScheduler
        )
        self.clipboardWriter = ShellClipboardWriter()
        self.closeConfirmationPresenter =
            closeConfirmationPresenter ?? ShellNSAlertCloseConfirmationPresenter()
        self.gracefulShutdownTimeout = gracefulShutdownTimeout
        self.performanceDiagnosticsRecorder = performanceDiagnosticsRecorder
        self.appIsActiveProvider = appIsActiveProvider
        self.shellState = shellState
        self.terminalRuntimeRegistry =
            terminalRuntimeRegistry
            ?? resolvedContext.terminalRuntimeRegistry
        self.selectedSpaceID = shellState.focusedSpaceID ?? shellState.spaces.first?.spaceID
        self.selectedTabID = shellState.focusedTabID ?? shellState.spaces.first?.tabs.first?.tabID

        // Route async persistence-write failures (debounced restore content) to the
        // control-plane diagnostics surface, mirroring the synchronous paths.
        persistenceCoordinator.onDiagnostic = { [weak self] message in
            self?.recordControlPlaneDiagnostic(message)
        }

        if shellState.panes.isEmpty {
            publishControlPlaneState()
        } else {
            shellState.panes.map(\.paneID).forEach(primeBootContext)
        }
        synchronizeSelection()
    }

    deinit {
        let terminalRuntimeRegistry = terminalRuntimeRegistry
        let terminalContentLifecycle = terminalContentLifecycle
        Task { @MainActor in
            terminalContentLifecycle.finalizeAllRuntimes(registry: terminalRuntimeRegistry)
        }
    }

    func shutdownTerminalRuntimes() {
        terminalContentLifecycle.finalizeAllRuntimes(registry: terminalRuntimeRegistry)
    }

    static func live(
        fileManager: FileManager = .default,
        windowContext: ShellWindowContext? = nil,
        startupMode: StartupMode = .fresh,
        workspaceManifestURL: URL? = nil,
        defaultWorkingDirectory: String? = nil,
        now: Date = .now
    ) -> ShellHostController {
        let installChannel = AlanInstallChannel.current()
        let usesRestorableWindowContext = startupMode == .restorePrevious || startupMode == .workspaceManifest
        let resolvedWindowContext =
            windowContext
            ?? ShellStatePersistenceStore.restoredWindowContext(
                fileManager: fileManager,
                restorePrevious: startupMode == .restorePrevious,
                channel: installChannel
            )
            ?? ShellStatePersistenceStore.defaultWindowContext(
                fileManager: fileManager,
                restorePrevious: usesRestorableWindowContext,
                channel: installChannel
            )
        let startup = ShellWorkspaceManifestStartupCoordinator(fileManager: fileManager).prepare(
            mode: startupMode.workspaceStartupMode,
            windowContext: resolvedWindowContext,
            workspaceManifestURL: workspaceManifestURL,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )

        let controller = ShellHostController(
            shellState: startup.shellState,
            fileManager: fileManager,
            windowContext: resolvedWindowContext,
            persistenceURL: resolvedWindowContext.persistenceURL,
            workspaceManifestStore: startup.manifestStore,
            workspaceManifest: startup.workspaceManifest
        )
        if startup.shouldPersistInitialShellState {
            controller.persistShellState()
        }
        if let manifestRecovery = startup.manifestRecovery {
            controller.recordWorkspaceManifestRecovery(manifestRecovery)
        }
        for diagnostic in startup.diagnostics {
            controller.recordControlPlaneDiagnostic(diagnostic)
        }
        if startup.retiredTabCount > 0 {
            controller.recordControlPlaneDiagnostic(
                "workspace manifest retired \(startup.retiredTabCount) inactive unpinned tab(s)"
            )
        }
        return controller
    }

    var spaces: [ShellSpace] {
        shellState.spaces
    }

    var selectedSpace: ShellSpace? {
        shellState.spaces.first { $0.spaceID == selectedSpaceID } ?? shellState.spaces.first
    }

    var selectedTab: ShellTab? {
        guard let selectedTabID else {
            return selectedSpace?.tabs.first
        }
        return selectedSpace?.tabs.first { $0.tabID == selectedTabID } ?? selectedSpace?.tabs.first
    }

    var selectedTabPaneTree: ShellPaneTreeNode? {
        selectedTab?.paneTree
    }

    var selectedTabZoomedPaneID: String? {
        guard let selectedTab else { return nil }
        return zoomedPaneID(in: selectedTab)
    }

    var panesForSelectedTab: [ShellPane] {
        guard let tabID = selectedTab?.tabID else { return [] }
        return shellState.panes.filter { $0.tabID == tabID }
    }

    var selectedPane: ShellPane? {
        if let focusedPane, focusedPane.tabID == selectedTab?.tabID {
            return focusedPane
        }
        return panesForSelectedTab.first
    }

    var focusedPane: ShellPane? {
        guard let focusedPaneID = shellState.focusedPaneID else { return nil }
        return pane(paneID: focusedPaneID)
    }

    var selectedPaneBootProfile: AlanShellBootProfile? {
        bootProfile(for: selectedPane)
    }

    var selectedPaneRuntime: TerminalHostRuntimeSnapshot {
        runtime(for: selectedPane?.paneID)
    }

    var focusedContentSupportsTerminalCommands: Bool {
        guard let focusedPaneID = shellState.focusedPaneID,
              let pane = pane(paneID: focusedPaneID)
        else {
            return false
        }
        return paneSupportsTerminalCommands(pane, in: shellState.contentStateProjection())
    }

    var attentionItems: [ShellAttentionItem] {
        let now = Date()
        return shellState.panes
            .compactMap { pane in
                let attention = shellEffectiveAttention(for: pane, now: now)
                guard attention != .idle else { return nil }
                return ShellAttentionItem(
                    paneID: pane.paneID,
                    spaceID: pane.spaceID,
                    tabID: pane.tabID,
                    title: pane.viewport?.title ?? pane.process?.program ?? "Pane",
                    summary: pane.viewport?.summary ?? "Activity detected",
                    attention: attention
                )
            }
            .sorted {
                Self.attentionRank(for: $0.attention) == Self.attentionRank(for: $1.attention)
                    ? $0.paneID < $1.paneID
                    : Self.attentionRank(for: $0.attention) > Self.attentionRank(for: $1.attention)
            }
    }

    var routingCandidates: [AlanShellRoutingCandidate] {
        routingCandidates(preferredPaneID: selectedPane?.paneID)
    }

    var moveDestinationTabs: [ShellTab] {
        guard let selectedPane else { return [] }
        return shellState.spaces
            .flatMap(\.tabs)
            .filter { $0.tabID != selectedPane.tabID }
            .sorted {
                if $0.tabID == $1.tabID {
                    return ($0.title ?? "") < ($1.title ?? "")
                }
                return $0.tabID < $1.tabID
            }
    }

    var awaitingAttentionCount: Int {
        attentionItems.filter { $0.attention == .awaitingUser }.count
    }

    var snapshotJSON: String {
        shellState.prettyPrintedJSON
    }

    func bootProfile(for pane: ShellPane?) -> AlanShellBootProfile? {
        guard let pane else { return nil }
        seedRestoredTranscriptSnapshotIfNeeded(for: pane)
        return bootProfileCache.profile(for: pane, shellState: shellState)
    }

    func restoredTranscriptSnapshot(for pane: ShellPane?) -> TerminalTranscriptSnapshot? {
        guard let pane,
              let content = terminalContentInstance(mountedIn: pane)
        else {
            return nil
        }
        return content.payload.terminal?.transcriptSnapshot?.boundedForManifest()
    }

    @discardableResult
    func clearRestoredTranscriptSnapshot(for pane: ShellPane?) -> Bool {
        guard let pane,
              let contentID = terminalContentID(mountedIn: pane)
        else {
            return false
        }
        return clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
    }

    @discardableResult
    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) -> Bool {
        terminalRuntimeRegistry.clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
        let stateResult = shellState.clearingRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
        if stateResult.removed {
            adoptStateFromControlPlane(stateResult.state, publish: false)
            publishControlPlaneState()
        }
        let manifestRemoved = clearRestoredTranscriptSnapshotFromWorkspaceManifest(
            forTerminalContentID: contentID
        )
        return stateResult.removed || manifestRemoved
    }

    private func seedRestoredTranscriptSnapshotIfNeeded(for pane: ShellPane) {
        guard let content = terminalContentInstance(mountedIn: pane),
              let transcriptSnapshot = content.payload.terminal?.transcriptSnapshot
        else {
            return
        }

        terminalRuntimeRegistry.seedRestoredTranscriptSnapshot(
            transcriptSnapshot,
            forTerminalContentID: content.contentID
        )
    }

    private func terminalContentInstance(mountedIn pane: ShellPane) -> ShellContentInstance? {
        let contentID =
            shellState.paneSlots?
                .first { $0.paneSlotID == pane.paneID }?
                .contentID
            ?? pane.terminalContentID
        return shellState.contents?.first { $0.contentID == contentID }
    }

    private func terminalContentID(mountedIn pane: ShellPane) -> String? {
        if let mountedContent = shellState.contentStateProjection().contentMounted(in: pane.paneID) {
            return mountedContent.kind == .terminal ? mountedContent.contentID : nil
        }
        return pane.terminalContentID
    }

    func runtime(for paneID: String?) -> TerminalHostRuntimeSnapshot {
        terminalRuntimeRegistry.snapshot(for: paneID)
    }

    func terminalRenderPriority(for pane: ShellPane) -> TerminalRuntimeRenderPriority {
        let visiblePaneIDs = Set(displayPaneTree(for: selectedTab)?.paneIDs ?? [])
        return terminalRuntimeRenderPriority(
            paneID: pane.paneID,
            paneSpaceID: pane.spaceID,
            paneTabID: pane.tabID,
            selectedSpaceID: selectedSpaceID,
            selectedTabID: selectedTabID,
            focusedPaneID: shellState.focusedPaneID,
            visiblePaneIDs: visiblePaneIDs,
            windowIsVisible: shellWindowIsVisibleForRendering
        )
    }

    func updateShellWindowVisibilityForRendering(_ isVisible: Bool) {
        guard shellWindowIsVisibleForRendering != isVisible else { return }
        shellWindowIsVisibleForRendering = isVisible
        synchronizeTerminalRenderPriorities()
    }

    func displayPaneTree(for tab: ShellTab?) -> ShellPaneTreeNode? {
        guard let tab else { return nil }
        guard let zoomedPaneID = zoomedPaneID(in: tab) else {
            return tab.paneTree
        }
        return tab.paneTree.leafNode(containingPaneID: zoomedPaneID) ?? tab.paneTree
    }

    func isPaneZoomed(_ paneID: String) -> Bool {
        guard let tab = tab(containingPaneID: paneID) else { return false }
        return zoomedPaneIDByTabID[tab.tabID] == paneID
    }

    func canZoomPane(_ paneID: String) -> Bool {
        guard let tab = tab(containingPaneID: paneID) else { return false }
        return tab.paneTree.paneIDs.count > 1
    }

    @discardableResult
    func toggleSelectedPaneZoom() -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        if isPaneZoomed(paneID) {
            return unzoomTab(containingPaneID: paneID)
        }
        return zoomPane(paneID: paneID)
    }

    @discardableResult
    func zoomPane(paneID: String) -> Bool {
        guard canZoomPane(paneID),
              let tab = tab(containingPaneID: paneID)
        else {
            return false
        }
        guard zoomedPaneIDByTabID[tab.tabID] != paneID else {
            return false
        }
        if shellState.focusedPaneID != paneID {
            focus(paneID: paneID)
        }
        zoomedPaneIDByTabID[tab.tabID] = paneID
        controlPlane.recordZoomStateChanged(
            requestID: nil,
            spaceID: shellState.contentStateProjection().paneSlot(paneSlotID: paneID)?.spaceID,
            tabID: tab.tabID,
            paneID: paneID,
            zoomedPaneID: paneID
        )
        synchronizeTerminalRenderPriorities()
        return true
    }

    @discardableResult
    func unzoomSelectedTab() -> Bool {
        guard let tabID = selectedTab?.tabID else { return false }
        return unzoomTab(tabID: tabID)
    }

    @discardableResult
    private func unzoomTab(containingPaneID paneID: String) -> Bool {
        guard let pane = pane(paneID: paneID) else { return false }
        return unzoomTab(tabID: pane.tabID)
    }

    @discardableResult
    func unzoomTab(tabID: String) -> Bool {
        guard let zoomedPaneID = zoomedPaneIDByTabID[tabID] else { return false }
        let pane = pane(paneID: zoomedPaneID)
        zoomedPaneIDByTabID.removeValue(forKey: tabID)
        controlPlane.recordZoomStateChanged(
            requestID: nil,
            spaceID: pane?.spaceID,
            tabID: tabID,
            paneID: zoomedPaneID,
            zoomedPaneID: nil
        )
        synchronizeTerminalRenderPriorities()
        return true
    }

    func select(spaceID: String) {
        guard let paneID = targetPaneID(forSpaceID: spaceID) else {
            guard shellState.space(spaceID: spaceID) != nil else { return }
            let spaces = shellState.spaces.map { space in
                guard space.spaceID == spaceID else { return space }
                return ShellSpace(
                    spaceID: space.spaceID,
                    title: space.title,
                    attention: space.attention,
                    tabs: space.tabs,
                    selectedTabID: nil,
                    terminalProfileID: space.terminalProfileID,
                    presentationIconSystemName: space.presentationIconSystemName
                )
            }
            shellState = ShellStateSnapshot(
                contractVersion: shellState.contractVersion,
                windowID: shellState.windowID,
                focusedSpaceID: spaceID,
                focusedTabID: nil,
                focusedPaneID: nil,
                spaces: spaces,
                panes: shellState.panes,
                paneSlots: shellState.paneSlots,
                contents: shellState.contents
            )
            synchronizeSelection()
            publishControlPlaneState()
            return
        }
        focus(paneID: paneID, requestTerminalFocus: true)
    }

    func select(tabID: String) {
        guard let paneID = targetPaneID(forTabID: tabID, in: selectedSpace) else { return }
        focus(paneID: paneID, requestTerminalFocus: true)
    }

    @discardableResult
    func selectSpace(at index: Int) -> Bool {
        guard spaces.indices.contains(index) else { return false }
        select(spaceID: spaces[index].spaceID)
        return true
    }

    @discardableResult
    func selectAdjacentSpace(offset: Int) -> Bool {
        guard spaces.count > 1 else { return false }
        guard let selectedSpaceID,
              let currentIndex = spaces.firstIndex(where: { $0.spaceID == selectedSpaceID })
        else {
            select(spaceID: spaces[0].spaceID)
            return true
        }

        let nextIndex = (currentIndex + offset + spaces.count) % spaces.count
        select(spaceID: spaces[nextIndex].spaceID)
        return true
    }

    @discardableResult
    func selectAdjacentTab(offset: Int) -> Bool {
        guard let selectedSpace,
              !selectedSpace.tabs.isEmpty
        else {
            return false
        }
        guard selectedSpace.tabs.count > 1 else { return false }
        let currentTabID = selectedTab?.tabID ?? selectedSpace.tabs.first?.tabID
        guard let currentTabID,
              let currentIndex = selectedSpace.tabs.firstIndex(where: { $0.tabID == currentTabID })
        else {
            return false
        }

        let nextIndex = (currentIndex + offset + selectedSpace.tabs.count) % selectedSpace.tabs.count
        select(tabID: selectedSpace.tabs[nextIndex].tabID)
        return true
    }

    func focusAttentionItem(_ item: ShellAttentionItem) {
        focus(paneID: item.paneID, requestTerminalFocus: true)
    }

    func focus(paneID: String) {
        focus(paneID: paneID, requestTerminalFocus: false)
    }

    private func focus(paneID: String, requestTerminalFocus: Bool) {
        let focusStartedAt = performanceDiagnosticsStartTime()
        let result: ShellStateMutationResult
        do {
            let rustResult = try reducerCoordinator.apply(
                state: shellState,
                operation: .focusPane(paneSlotID: paneID)
            )
            // Rust owns workspace focus. Swift keeps this narrow post-pass
            // for platform terminal activity acknowledgement until activity
            // signals are fully domain-owned by shell-core.
            let acknowledgedState = rustResult.tabID.map { tabID in
                rustResult.state.acknowledgingCommandFailureActivities(
                    in: tabID,
                    focusedPaneID: paneID
                )
            } ?? rustResult.state
            result = ShellStateMutationResult(
                state: acknowledgedState,
                spaceID: rustResult.spaceID,
                tabID: rustResult.tabID,
                paneID: rustResult.paneID
            )
        } catch {
            recordControlPlaneDiagnostic("shell-core focus pane failed: \(error)")
            return
        }
        applyMutationResult(result)
        if let focusStartedAt {
            let focusedPane = pane(paneID: paneID)
            recordPerformanceDiagnostic(
                .shellFocusChange,
                durationMs: performanceDurationMs(since: focusStartedAt),
                runtime: runtime(for: paneID),
                fallbackPaneID: paneID,
                fallbackContentID: focusedPane?.terminalContentID,
                fallbackPriority: focusedPane.map { terminalRenderPriority(for: $0) }
            )
        }
        if requestTerminalFocus && canRequestTerminalFocus(for: paneID) {
            terminalRuntimeRegistry.requestFocus(for: paneID)
        }
    }

    private func targetPaneID(forSpaceID spaceID: String) -> String? {
        guard let space = shellState.spaces.first(where: { $0.spaceID == spaceID }) else {
            return nil
        }
        let targetTab =
            space.selectedTabID.flatMap { selectedTabID in
                space.tabs.first { $0.tabID == selectedTabID }
            }
            ?? space.tabs.first { tab in
                guard let focusedPaneID = shellState.focusedPaneID else { return false }
                return tab.contains(paneID: focusedPaneID)
            }
            ?? space.tabs.first
        return targetTab.flatMap(targetPaneID)
    }

    private func targetPaneID(
        forTabID tabID: String,
        in space: ShellSpace?
    ) -> String? {
        guard let tab = space?.tabs.first(where: { $0.tabID == tabID }) else {
            return nil
        }
        return targetPaneID(for: tab)
    }

    private func targetPaneID(for tab: ShellTab) -> String? {
        if let focusedPaneID = shellState.focusedPaneID,
           tab.contains(paneID: focusedPaneID)
        {
            return focusedPaneID
        }
        let contentState = shellState.contentStateProjection()
        return tab.paneTree.paneIDs.first { paneID in
            contentState.paneSlot(paneSlotID: paneID)?.tabID == tab.tabID
        } ?? tab.paneTree.paneIDs.first
    }

    func refocusSelectedTerminalPane() {
        guard let paneID = selectedPane?.paneID else { return }
        guard canRequestTerminalFocus(for: paneID) else { return }
        terminalRuntimeRegistry.requestFocus(for: paneID)
    }

    private func canRequestTerminalFocus(for paneID: String) -> Bool {
        guard let pane = pane(paneID: paneID) else { return false }
        return paneHasTerminalContent(pane, in: shellState.contentStateProjection())
    }

    @discardableResult
    func requestCloseWindow() -> Bool {
        requestCloseShellSurface(scope: .window)
    }

    @discardableResult
    func requestTerminateApp() -> Bool {
        requestCloseShellSurface(scope: .app)
    }

    private func requestCloseShellSurface(scope: ShellCloseGuardScope) -> Bool {
        // Flush any debounced restore content before tearing down so a clean exit
        // never loses the most recent transcript.
        flushWorkspacePersistence()
        if let impact = closeGuardImpact(for: scope) {
            return confirmAndApplyClose(impact)
        }
        shutdownTerminalRuntimes()
        return true
    }

    func terminalHostDidRequestActivation(paneID: String) {
        focus(paneID: paneID)
    }

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
                result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
            result = try reducerCoordinator.apply(
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
                    result = try reducerCoordinator.apply(
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
                result = try reducerCoordinator.apply(
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
                result = try reducerCoordinator.apply(
                    state: shellState,
                    operation: .openContentTab(
                        spaceID: spaceID,
                        kind: .settings,
                        title: content.title,
                        payload: content.payload,
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
            return try reducerCoordinator.apply(
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

    private func openTerminalTabMutation(
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
                        result = try reducerCoordinator.apply(
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
                    result = try reducerCoordinator.apply(
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
                    result = try reducerCoordinator.apply(
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
                }
            } else {
                result = try reducerCoordinator.apply(
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

    @discardableResult
    func focusAdjacentPane(direction: ShellSpatialFocusDirection) -> Bool {
        let previousPaneID = shellState.focusedPaneID
        let rustResult: ShellStateMutationResult
        do {
            rustResult = try reducerCoordinator.apply(
                state: shellState,
                operation: .focusAdjacentPane(direction: direction)
            )
        } catch {
            return false
        }
        // Mirror focus(paneID:): acknowledge command-failure activity/attention on the newly
        // focused pane so spatial focus also clears a stale failure indicator within the tab.
        let result: ShellStateMutationResult
        if let tabID = rustResult.tabID, let focusedPaneID = rustResult.paneID {
            result = ShellStateMutationResult(
                state: rustResult.state.acknowledgingCommandFailureActivities(
                    in: tabID,
                    focusedPaneID: focusedPaneID
                ),
                spaceID: rustResult.spaceID,
                tabID: rustResult.tabID,
                paneID: rustResult.paneID
            )
        } else {
            result = rustResult
        }
        applyMutationResult(result)
        controlPlane.recordSpatialFocus(
            requestID: nil,
            spaceID: result.spaceID,
            tabID: result.tabID,
            previousPaneID: previousPaneID,
            currentPaneID: result.paneID,
            direction: direction,
            applied: true
        )
        return true
    }

    @discardableResult
    func performShellWorkspaceCommand(_ command: ShellWorkspaceCommand) -> Bool {
        switch command {
        case .newTerminalTab:
            return performShellAutomationCommand(
                .createTab(
                    ShellAutomationCreateTabRequest(
                        launchTarget: .shell,
                        spaceID: nil,
                        title: nil,
                        workingDirectory: nil
                    )
                )
            ).applied
        case .splitLeft:
            return performShellAutomationSplitFromFocusedPane(.left)
        case .splitRight:
            return performShellAutomationSplitFromFocusedPane(.right)
        case .splitUp:
            return performShellAutomationSplitFromFocusedPane(.up)
        case .splitDown:
            return performShellAutomationSplitFromFocusedPane(.down)
        case .focusLeft:
            return focusAdjacentPane(direction: .left)
        case .focusRight:
            return focusAdjacentPane(direction: .right)
        case .focusUp:
            return focusAdjacentPane(direction: .up)
        case .focusDown:
            return focusAdjacentPane(direction: .down)
        case .equalizeSplits:
            return equalizeSelectedTabSplits()
        case .togglePaneZoom:
            return toggleSelectedPaneZoom()
        case .movePaneLeft:
            return moveSelectedPaneWithinTab(.left)
        case .movePaneRight:
            return moveSelectedPaneWithinTab(.right)
        case .movePaneUp:
            return moveSelectedPaneWithinTab(.up)
        case .movePaneDown:
            return moveSelectedPaneWithinTab(.down)
        case .closePane:
            guard let paneID = selectedPane?.paneID else { return false }
            return requestClosePane(paneID: paneID)
        case .closeTab:
            guard let tabID = selectedTabID else { return false }
            return requestCloseTab(tabID: tabID)
        }
    }

    private func performShellAutomationSplitFromFocusedPane(
        _ placement: ShellPaneSplitDirection
    ) -> Bool {
        guard let focusedPaneID = shellState.focusedPaneID else { return false }
        return performShellAutomationCommand(
            .splitPane(ShellAutomationPaneSplitRequest(paneID: focusedPaneID, placement: placement))
        ).applied
    }

    func shellActionTitle(_ id: ShellActionID) -> String {
        actionCoordinator.title(id)
    }

    func shellActionAvailability(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionAvailability {
        actionCoordinator.availability(id, target: target, state: shellState)
    }

    func shellActionShortcut(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionShortcut? {
        actionCoordinator.shortcut(id, target: target)
    }

    @discardableResult
    func performShellAction(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection,
        source: ShellTerminalCommandSource = .keyboardShortcut
    ) -> ShellActionExecutionResult {
        actionCoordinator.perform(
            id,
            target: target,
            source: source,
            state: shellState,
            isModalFlowActive: isPresentingSpaceCreation,
            openSearch: { [weak self] source, target in
                self?.openTerminalSearch(source: source, target: target) ?? false
            },
            effectHandlers: shellActionEffectHandlers
        )
    }

    private var shellActionEffectHandlers: ShellActionEffectHandlers {
        ShellActionEffectHandlers(
            selectedTabID: { [weak self] in self?.selectedTabID },
            selectedPaneID: { [weak self] in self?.selectedPane?.paneID },
            performWorkspaceCommand: { [weak self] command in
                self?.performShellWorkspaceCommand(command) ?? false
            },
            openTab: { [weak self] launchTarget, spaceID in
                self?.performShellAutomationCommand(
                    .createTab(
                        ShellAutomationCreateTabRequest(
                            launchTarget: launchTarget,
                            spaceID: spaceID,
                            title: nil,
                            workingDirectory: nil
                        )
                    )
                ).applied ?? false
            },
            requestCloseTab: { [weak self] tabID in
                self?.requestCloseTab(tabID: tabID) ?? false
            },
            duplicateTab: { [weak self] tabID in
                self?.duplicateTab(tabID: tabID) ?? false
            },
            openTabInSplitView: { [weak self] tabID in
                self?.openTabInSplitView(tabID: tabID) ?? false
            },
            requestClosePane: { [weak self] paneID in
                self?.requestClosePane(paneID: paneID) ?? false
            },
            selectAdjacentTab: { [weak self] offset in
                self?.selectAdjacentTab(offset: offset) ?? false
            },
            selectAdjacentSpace: { [weak self] offset in
                self?.selectAdjacentSpace(offset: offset) ?? false
            },
            selectSpaceAt: { [weak self] index in
                self?.selectSpace(at: index) ?? false
            },
            pinTab: { [weak self] tabID in
                self?.pinTab(tabID: tabID) ?? false
            },
            unpinTab: { [weak self] tabID in
                self?.unpinTab(tabID: tabID) ?? false
            },
            updatePinnedTab: { [weak self] tabID in
                self?.updatePinnedTabSnapshot(tabID: tabID) ?? false
            },
            moveTab: { [weak self] tabID, offset in
                self?.moveTab(tabID: tabID, offset: offset) ?? false
            },
            moveTabToSpace: { [weak self] tabID, spaceID in
                self?.moveTabToSpace(tabID: tabID, targetSpaceID: spaceID) ?? false
            },
            movePaneWithinTab: { [weak self] paneID, placement in
                self?.movePaneWithinTab(paneID: paneID, placement: placement) ?? false
            },
            clearTerminal: { [weak self] paneID in
                self?.clearTerminal(paneID: paneID) ?? false
            }
        )
    }

    @discardableResult
    private func clearTerminal(paneID: String?) -> Bool {
        guard let pane = paneID.flatMap({ shellState.pane(paneID: $0) }) ?? selectedPane,
              let contentID = terminalContentID(mountedIn: pane)
        else {
            return false
        }

        clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
        let delivery = terminalRuntimeRegistry.sendText(toTerminalContentID: contentID, text: "\u{0c}")
        return delivery.applied
    }

    @discardableResult
    func resizeSplit(splitNodeID: String, ratio: Double, persist: Bool = true) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerCoordinator.apply(
                state: shellState,
                operation: .resizeSplit(splitNodeID: splitNodeID, ratio: ratio)
            )
        } catch {
            return false
        }
        applyMutationResult(result, publish: persist)
        return true
    }

    @discardableResult
    func equalizeSelectedTabSplits() -> Bool {
        let previousTab = selectedTab
        let result: ShellStateMutationResult
        do {
            result = try reducerCoordinator.apply(
                state: shellState,
                operation: .equalizeSplits(tabID: selectedTabID)
            )
        } catch {
            return false
        }
        let changedSplitIDs = previousTab
            .flatMap { previous in
                result.state.tab(tabID: previous.tabID)?.paneTree
                    .splitNodeIDsWithChangedRatios(comparedTo: previous.paneTree)
            } ?? []
        applyMutationResult(result)
        if let tabID = result.tabID,
           let previousTab,
           !changedSplitIDs.isEmpty
        {
            controlPlane.recordSplitEqualized(
                requestID: nil,
                spaceID: result.spaceID,
                tabID: tabID,
                changedSplitIDs: changedSplitIDs,
                affectedPaneIDs: previousTab.paneTree.paneIDs
            )
        }
        return true
    }

    @discardableResult
    func closeSelectedTab() -> Bool {
        guard let selectedTabID else { return false }
        return requestCloseTab(tabID: selectedTabID)
    }

    @discardableResult
    func closeSelectedPane() -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return requestClosePane(paneID: paneID)
    }

    @discardableResult
    func closePaneByID(_ paneID: String) -> Bool {
        requestClosePane(paneID: paneID)
    }

    @discardableResult
    func liftSelectedPaneToTab(title: String? = nil) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return liftPaneToTab(paneID: paneID, title: title) == .lifted
    }

    @discardableResult
    func moveSelectedPane(
        toTab tabID: String,
        direction: ShellSplitDirection = .vertical
    ) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return movePane(paneID: paneID, toTab: tabID, direction: direction)
    }

    @discardableResult
    func moveSelectedPaneWithinTab(_ placement: ShellPaneSplitDirection) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return movePaneWithinTab(paneID: paneID, placement: placement)
    }

    @discardableResult
    func focusTopRoutingCandidate(preferredPaneID: String? = nil) -> String? {
        guard let candidate = routingCandidates(preferredPaneID: preferredPaneID).first else {
            return nil
        }
        focus(paneID: candidate.paneID)
        return candidate.paneID
    }

    @discardableResult
    func setAttention(_ attention: ShellAttentionState, for paneID: String) -> Bool {
        let result: ShellStateMutationResult
        do {
            result = try reducerCoordinator.apply(
                state: shellState,
                operation: .setAttention(paneSlotID: paneID, attention: attention)
            )
        } catch {
            return false
        }
        applyMutationResult(result)
        return true
    }

    func copySnapshotJSON() {
        clipboardWriter.writeString(snapshotJSON)
        lastCopiedAt = .now
    }

    func terminalCommandResolution(
        for command: ShellTerminalCommand,
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> ShellTerminalCommandResolution {
        ShellCommandTargetResolver.resolveTerminalCommand(
            command,
            source: source,
            target: target,
            state: shellState
        ) { [terminalRuntimeRegistry] paneID in
            terminalRuntimeRegistry.terminalCommandRuntimeState(for: paneID)
        }
    }

    func canCopyTerminalSelection(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .copySelection,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    func canPasteIntoTerminal(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .paste,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    func canOpenTerminalSearch(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        terminalCommandResolution(
            for: .search,
            source: source,
            target: target
        ).terminalTarget != nil
    }

    @discardableResult
    func copyTerminalSelection(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection,
        writer: AlanTerminalPasteboardWriting? = nil
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .copySelection,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        if let writer {
            return terminalRuntimeRegistry.copySelection(for: terminalTarget.paneID, to: writer)
        }
        return terminalRuntimeRegistry.copySelection(for: terminalTarget.paneID)
    }

    @discardableResult
    func pasteIntoTerminalFromPasteboard(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else {
            return false
        }
        return pasteIntoTerminal(text, source: source, target: target)
    }

    @discardableResult
    func pasteIntoTerminal(
        _ text: String,
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .paste,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.pasteText(text, to: terminalTarget.paneID).applied
    }

    @discardableResult
    func openTerminalSearch(
        source: ShellTerminalCommandSource = .keyboardShortcut,
        target: ShellActionTarget = .currentSelection
    ) -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .search,
            source: source,
            target: target
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.beginFindInteraction(for: terminalTarget.paneID)
    }

    var focusedPaneHasReliableSemanticCommands: Bool {
        guard focusedContentSupportsTerminalCommands else { return false }
        guard let paneID = selectedPane?.paneID,
              paneID == terminalRuntime.paneID
        else {
            return false
        }
        return terminalRuntime.surfaceState.terminalMode == .normalBuffer
            && terminalRuntime.surfaceState.semanticCommands.hasReliableCommandBoundaries
    }

    @discardableResult
    func jumpToPreviousPrompt() -> Bool {
        navigateSemanticPrompt(.previous)
    }

    @discardableResult
    func jumpToNextPrompt() -> Bool {
        navigateSemanticPrompt(.next)
    }

    @discardableResult
    func copyLastCommandOutput() -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .copyLastCommandOutput,
            source: .commandUI
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.copyLastCommandOutput(for: terminalTarget.paneID)
    }

    @discardableResult
    func searchLastCommandOutput() -> Bool {
        guard let terminalTarget = terminalCommandResolution(
            for: .searchLastCommandOutput,
            source: .commandUI
        ).terminalTarget else {
            return false
        }
        return terminalRuntimeRegistry.beginLastCommandOutputSearch(for: terminalTarget.paneID)
    }

    @discardableResult
    private func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        return terminalRuntimeRegistry.navigateSemanticPrompt(for: paneID, direction: direction)
    }

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
        let previousRuntime = runtime.paneID.map { self.runtime(for: $0) }
        terminalRuntimeRegistry.updateSnapshot(runtime)

        if let paneID = runtime.paneID,
           runtime.isFocused,
           shellState.focusedPaneID != paneID
        {
            focus(paneID: paneID)
            return
        }

        guard TerminalRuntimePublicationPolicy.shouldProjectToShell(
            previous: previousRuntime,
            next: runtime
        ) else {
            return
        }

        if runtime.renderPriority == .visibleBackground {
            scheduleVisibleBackgroundRuntimeProjection(runtime)
            return
        }

        projectTerminalRuntime(runtime)
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
        if runtime.paneID == selectedPane?.paneID || runtime.paneID == shellState.focusedPaneID {
            setSelectedTerminalRuntime(runtime)
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
            let activeTaskChanged = recordTerminalActiveTask(
                runtime.paneMetadata.activeTaskState,
                processExited: effectProjection.processExited,
                for: paneID
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

    private func recordPerformanceDiagnostic(
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

    private func performanceDurationMs(since start: DispatchTime) -> Double {
        let end = DispatchTime.now()
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    private func performanceDiagnosticsStartTime() -> DispatchTime? {
        if let performanceDiagnosticsRecorder {
            return performanceDiagnosticsRecorder.isEnabled ? DispatchTime.now() : nil
        }
        return AlanPerformanceDiagnosticsController.shared.isEnabled ? DispatchTime.now() : nil
    }

    private func scheduleVisibleBackgroundRuntimeProjection(_ runtime: TerminalHostRuntimeSnapshot) {
        guard let paneID = runtime.paneID else {
            projectTerminalRuntime(runtime)
            return
        }
        pendingVisibleBackgroundRuntimeByPaneID[paneID] = runtime
        guard !visibleBackgroundRuntimeProjectionScheduled else { return }
        visibleBackgroundRuntimeProjectionScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(16)) { [weak self] in
            self?.flushVisibleBackgroundRuntimeProjections()
        }
    }

    private func flushVisibleBackgroundRuntimeProjections() {
        let pending = pendingVisibleBackgroundRuntimeByPaneID
        pendingVisibleBackgroundRuntimeByPaneID.removeAll()
        visibleBackgroundRuntimeProjectionScheduled = false
        for runtime in pending.values.sorted(by: { ($0.paneID ?? "") < ($1.paneID ?? "") }) {
            projectTerminalRuntime(runtime)
        }
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
        let activeTaskChanged = recordTerminalActiveTask(
            metadata.activeTaskState,
            processExited: effectProjection.processExited,
            for: paneID
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

    private func applyAlanBinding(_ binding: ShellAlanBinding?, for paneID: String) {
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

    private func primeBootContext(for paneID: String) {
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
            contents: shellState.contents
        )
        synchronizeSelection()
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
            contents: shellState.contents
        )
        synchronizeSelection()
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

    private func adoptStateFromControlPlane(
        _ state: ShellStateSnapshot,
        publish: Bool = true
    ) {
        terminalContentLifecycle.reconcileRuntimes(
            afterAdopting: state,
            registry: terminalRuntimeRegistry
        )

        shellState = platformMetadataPreserver.preservingPlatformMetadata(in: state) { [weak self] paneID in
            self?.runtime(for: paneID) ?? .placeholder
        }
        reconcilePaneZoomState()
        synchronizeSelection()
        if publish {
            publishControlPlaneState()
        }
    }

    private func recordControlPlaneDiagnostic(_ message: String) {
        let line = "\(Self.iso8601Formatter.string(from: .now)) \(message)"
        guard controlPlaneDiagnostics.last != line else { return }
        controlPlaneDiagnostics.append(line)
        if controlPlaneDiagnostics.count > 12 {
            controlPlaneDiagnostics.removeFirst(controlPlaneDiagnostics.count - 12)
        }
    }

    private func synchronizeSelection() {
        let selectionStartedAt = performanceDiagnosticsStartTime()
        defer {
            if let selectionStartedAt {
                let selectedPane = selectedPane
                recordPerformanceDiagnostic(
                    .shellSelectionChange,
                    durationMs: performanceDurationMs(since: selectionStartedAt),
                    runtime: terminalRuntime,
                    fallbackPaneID: selectedPane?.paneID,
                    fallbackContentID: selectedPane?.terminalContentID,
                    fallbackPriority: selectedPane.map { terminalRenderPriority(for: $0) }
                )
            }
        }
        if let focusedPane = focusedPane {
            selectedSpaceID = focusedPane.spaceID
            selectedTabID = focusedPane.tabID
            setSelectedTerminalRuntime(runtime(for: focusedPane.paneID))
            synchronizeTerminalRenderPriorities()
            return
        }

        selectedSpaceID = shellState.focusedSpaceID ?? selectedSpaceID ?? shellState.spaces.first?.spaceID
        selectedTabID = shellState.focusedTabID ?? selectedSpace?.tabs.first?.tabID
        setSelectedTerminalRuntime(runtime(for: selectedPane?.paneID))
        synchronizeTerminalRenderPriorities()
    }

    /// Assigns the selected-pane runtime snapshot only when it differs from the
    /// current one in something other than its publish timestamp. This stays off
    /// the host's broad `@Published` surface so terminal-output churn does not
    /// invalidate unrelated SwiftUI chrome.
    private func setSelectedTerminalRuntime(_ runtime: TerminalHostRuntimeSnapshot) {
        guard !terminalRuntime.equalsIgnoringTimestamp(runtime) else { return }
        terminalRuntime = runtime
    }

    private func synchronizeTerminalRenderPriorities() {
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
                runtime: terminalRuntime,
                fallbackPaneID: selectedPane?.paneID,
                fallbackContentID: selectedPane?.terminalContentID,
                fallbackPriority: selectedPane.map { terminalRenderPriority(for: $0) },
                counts: AlanPerformanceDiagnosticCounts(events: prioritiesByContentID.count)
            )
        }
    }

    private func zoomedPaneID(in tab: ShellTab) -> String? {
        guard let paneID = zoomedPaneIDByTabID[tab.tabID],
              tab.paneTree.contains(paneID: paneID),
              tab.paneTree.paneIDs.count > 1
        else {
            return nil
        }
        return paneID
    }

    private func paneSupportsTerminalCommands(
        _ pane: ShellPane,
        in contentState: ShellContentStateSnapshot
    ) -> Bool {
        if let content = contentState.contentMounted(in: pane.paneID) {
            return content.kind == .terminal
                && content.capabilities.contains(.terminalInput)
        }

        return false
    }

    private func paneHasTerminalContent(
        _ pane: ShellPane,
        in contentState: ShellContentStateSnapshot
    ) -> Bool {
        if let content = contentState.contentMounted(in: pane.paneID) {
            return content.kind == .terminal
        }

        return false
    }

    private func tab(containingPaneID paneID: String) -> ShellTab? {
        shellState.spaces
            .flatMap(\.tabs)
            .first { $0.contains(paneID: paneID) }
    }

    private func reconcilePaneZoomState() {
        var nextZoomState: [String: String] = [:]
        let tabsByID = Dictionary(uniqueKeysWithValues: shellState.spaces.flatMap(\.tabs).map {
            ($0.tabID, $0)
        })

        for (tabID, paneID) in zoomedPaneIDByTabID {
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
    }

    private func persistShellState(coalesced: Bool = false) {
        persistenceCoordinator.persistShellState(shellState, coalesced: coalesced)
    }

    /// Forces pending debounced persistence to disk synchronously. Wired to app
    /// background/resign-active and quit so a clean exit never loses pending
    /// restore content; also a deterministic flush point for tests.
    func flushWorkspacePersistence() {
        persistenceCoordinator.flushWorkspacePersistence(
            state: shellState,
            controlPlane: controlPlane,
            makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                self?.makeWorkspaceManifestFromShellState(
                    now: now,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
            },
            makePinnedSnapshot: { [weak self] tabID in
                self?.makePinnedTabSnapshot(tabID: tabID)
            }
        )
    }

    private func clearRestoredTranscriptSnapshotFromWorkspaceManifest(
        forTerminalContentID contentID: String
    ) -> Bool {
        persistenceCoordinator.clearRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
    }

    private func makePinnedTabSnapshot(tabID: String) -> ShellContentTabRestoreSnapshot? {
        shellState.tab(tabID: tabID).map(makeRestoreSnapshot)
    }

    private func updateWorkspaceManifestTab(
        tabID: String,
        mutate: (inout ShellContentWorkspaceTabRecord, ShellContentTabRestoreSnapshot) -> Void,
        diagnostic: (String) -> String
    ) -> Bool {
        let updated = persistenceCoordinator.updateManifestTab(
            tabID: tabID,
            makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                self?.makeWorkspaceManifestFromShellState(
                    now: now,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
            },
            makePinnedSnapshot: { [weak self] tabID in
                self?.makePinnedTabSnapshot(tabID: tabID)
            },
            mutate: mutate,
            diagnostic: diagnostic
        )
        if updated {
            objectWillChange.send()
        }
        return updated
    }

    private func makeWorkspaceManifestFromShellState(now: Date) -> ShellContentWorkspaceManifest {
        makeWorkspaceManifestFromShellState(now: now, transcriptSnapshotOverrides: [:])
    }

    private func makeWorkspaceManifestFromShellState(
        now: Date,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentWorkspaceManifest {
        let existingSpaces = Dictionary(
            uniqueKeysWithValues: (workspaceManifest?.spaces ?? []).map { ($0.spaceID, $0) }
        )
        let existingTabs = Dictionary(
            uniqueKeysWithValues: (workspaceManifest?.spaces ?? [])
                .flatMap(\.tabs)
                .map { ($0.tabID, $0) }
        )
        let contentState = shellState.contentStateProjection()

        let spaces = shellState.spaces.enumerated().map { index, space -> ShellContentWorkspaceSpaceRecord in
            let existingSpace = existingSpaces[space.spaceID]
            let tabRecords = space.tabs.map { tab -> ShellContentWorkspaceTabRecord in
                let existingTab = existingTabs[tab.tabID]
                let panes = shellState.panes(in: tab.tabID)
                let snapshot = makeRestoreSnapshot(
                    for: tab,
                    contentState: contentState,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
                let paneActivityAt = panes.compactMap { paneActivityDate($0) }.max()
                let lastActivatedAt = tab.tabID == shellState.focusedTabID
                    ? now
                    : (existingTab?.lastActivatedAt ?? now)
                let lastActivityAt = max(
                    existingTab?.lastActivityAt ?? now,
                    paneActivityAt ?? existingTab?.lastActivityAt ?? now
                )

                return ShellContentWorkspaceTabRecord(
                    tabID: tab.tabID,
                    title: tab.title,
                    kind: tab.kind,
                    createdAt: existingTab?.createdAt ?? now,
                    lastActivatedAt: lastActivatedAt,
                    lastActivityAt: lastActivityAt,
                    isPinned: tab.isPinned,
                    isTitleUserLocked: tab.isTitleUserLocked,
                    pinSnapshot: tab.isPinned ? existingTab?.pinSnapshot : nil,
                    liveSnapshot: snapshot,
                    activeTask: projectedActiveTask(for: tab, panes: panes)
                )
            }

            return ShellContentWorkspaceSpaceRecord(
                spaceID: space.spaceID,
                title: space.title,
                order: existingSpace?.order ?? index,
                createdAt: existingSpace?.createdAt ?? now,
                updatedAt: now,
                selectedTabID: space.resolvedSelectedTabID,
                tabs: tabRecords,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }

        var manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: shellState.windowID,
            selectedSpaceID: shellState.focusedSpaceID ?? selectedSpaceID,
            selectedTabID: shellState.focusedTabID,
            spaces: spaces
        )
        manifest.repairSelection()
        return manifest
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab
    ) -> ShellContentTabRestoreSnapshot {
        makeRestoreSnapshot(for: tab, contentState: shellState.contentStateProjection())
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab,
        contentState: ShellContentStateSnapshot
    ) -> ShellContentTabRestoreSnapshot {
        makeRestoreSnapshot(
            for: tab,
            contentState: contentState,
            transcriptSnapshotOverrides: [:]
        )
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab,
        contentState: ShellContentStateSnapshot,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentTabRestoreSnapshot {
        let snapshot = ShellContentTabRestoreSnapshot.projecting(tab: tab, contentState: contentState)
        var capturedTranscripts = capturedTerminalTranscriptSnapshots(for: snapshot)
        capturedTranscripts.merge(transcriptSnapshotOverrides) { _, override in override }
        return snapshot.overlayingTerminalTranscriptSnapshots(capturedTranscripts)
    }

    private func capturedTerminalTranscriptSnapshots(
        for snapshot: ShellContentTabRestoreSnapshot
    ) -> [String: TerminalTranscriptSnapshot] {
        var capturedByContentID: [String: TerminalTranscriptSnapshot] = [:]
        for content in snapshot.contents where content.kind == .terminal {
            if let transcript = capturedTerminalTranscriptSnapshot(forContentID: content.contentID) {
                capturedByContentID[content.contentID] = transcript
            }
        }
        return capturedByContentID
    }

    private func capturedTerminalTranscriptSnapshot(
        forContentID contentID: String
    ) -> TerminalTranscriptSnapshot? {
        switch terminalRuntimeRegistry.captureTranscriptSnapshot(forTerminalContentID: contentID) {
        case .captured(let transcript):
            return transcript
        case .failed(let failure):
            recordControlPlaneDiagnostic(
                "terminal transcript capture failed for \(contentID): \(failure.code.rawValue)"
            )
            return nil
        }
    }

    @discardableResult
    private func captureTerminalTranscriptSnapshots(
        for impact: ShellCloseGuardImpact
    ) -> [String: TerminalTranscriptSnapshot] {
        var capturedByContentID: [String: TerminalTranscriptSnapshot] = [:]
        for contentID in impact.affectedTerminalContentIDs {
            switch terminalRuntimeRegistry.captureTranscriptSnapshot(forTerminalContentID: contentID) {
            case .captured(let transcript):
                capturedByContentID[contentID] = transcript
            case .failed(let failure):
                recordControlPlaneDiagnostic(
                    "terminal transcript capture failed for \(contentID): \(failure.code.rawValue)"
                )
            }
        }
        return capturedByContentID
    }

    private func paneActivityDate(_ pane: ShellPane) -> Date? {
        if let lastActivityAt = pane.viewport?.lastActivityAt,
           let date = Self.iso8601Formatter.date(from: lastActivityAt)
        {
            return date
        }

        if let lastMetadataAt = pane.context?.lastMetadataAt,
           let date = Self.iso8601Formatter.date(from: lastMetadataAt)
        {
            return date
        }

        return nil
    }

    private func projectedActiveTask(
        for tab: ShellTab,
        panes: [ShellPane]
    ) -> ShellTabActiveTaskState {
        if let terminalActiveTask = strongestTerminalActiveTask(
            in: panes.filter { tab.contains(paneID: $0.paneID) }
        ),
           terminalActiveTask.protectsFromPruning
        {
            return terminalActiveTask
        }

        for pane in panes where tab.contains(paneID: pane.paneID) {
            if pane.alanBinding?.pendingRequest == true {
                return .alanPendingYield
            }

            if let machineState = pane.alanBinding?.machineState,
               !Self.inactiveAlanMachineStates.contains(machineState.lowercased())
            {
                return .alanRunning
            }

            if pane.context?.processState == "foreground_command" {
                return .foregroundCommand
            }
        }

        return .inactive
    }

    private func activeTaskByTabID() -> [String: ShellTabActiveTaskState] {
        shellState.spaces
            .flatMap(\.tabs)
            .reduce(into: [String: ShellTabActiveTaskState]()) { result, tab in
                result[tab.tabID] = projectedActiveTask(for: tab, panes: shellState.panes)
            }
    }

    @discardableResult
    private func recordTerminalActiveTask(
        _ activeTaskState: ShellTabActiveTaskState?,
        processExited: Bool,
        for paneID: String
    ) -> Bool {
        let nextState: ShellTabActiveTaskState?
        if processExited {
            nextState = .inactive
        } else {
            nextState = activeTaskState
        }

        guard let nextState else { return false }
        guard terminalActiveTasksByPaneID[paneID] != nextState else { return false }
        terminalActiveTasksByPaneID[paneID] = nextState
        return true
    }

    private func strongestTerminalActiveTask(in panes: [ShellPane]) -> ShellTabActiveTaskState? {
        panes
            .compactMap { terminalActiveTasksByPaneID[$0.paneID] }
            .max { activeTaskRank($0) < activeTaskRank($1) }
    }

    private func activeTaskRank(_ state: ShellTabActiveTaskState) -> Int {
        switch state {
        case .inactive:
            return 0
        case .unknown:
            return 1
        case .foregroundCommand:
            return 2
        case .alanRunning:
            return 3
        case .alanProcess:
            return 4
        case .alanPendingYield:
            return 5
        }
    }

    private static let inactiveAlanMachineStates: Set<String> = [
        "completed",
        "failed",
        "cancelled",
        "canceled",
        "exited",
        "idle",
    ]

    private func recordWorkspaceManifestRecovery(_ recovery: ShellWorkspaceManifestRecovery) {
        switch recovery {
        case .loadedExisting:
            return
        case .migratedLegacyTerminalManifest:
            recordControlPlaneDiagnostic("workspace manifest migrated terminal snapshots to content containers")
        case .createdDefault:
            recordControlPlaneDiagnostic("workspace manifest created default")
        case .quarantinedCorruptFile(let url):
            recordControlPlaneDiagnostic("workspace manifest corrupt file quarantined: \(url.path)")
        }
    }

    func pane(paneID: String) -> ShellPane? {
        shellState.panes.first { $0.paneID == paneID }
    }

    private func nextID(prefix: String, existing: [String]) -> String {
        let nextOrdinal = existing
            .compactMap { identifier -> Int? in
                let components = identifier.split(separator: "_")
                guard let last = components.last else { return nil }
                return Int(last)
            }
            .max()
            .map { $0 + 1 }
            ?? (existing.isEmpty ? 1 : existing.count + 1)

        return "\(prefix)_\(nextOrdinal)"
    }

    func closeTab(tabID: String) -> ShellTabCloseResult {
        if let impact = closeGuardImpact(for: .tab(tabID)) {
            return .requiresConfirmation(impact)
        }
        return applyCloseTabMutation(tabID: tabID)
    }

    @discardableResult
    func requestCloseTab(tabID: String) -> Bool {
        switch closeTab(tabID: tabID) {
        case .closed:
            return true
        case .requiresConfirmation(let impact):
            return confirmAndApplyClose(impact)
        case .tabNotFound, .lastTab:
            return false
        }
    }

    private func applyCloseTabMutation(tabID: String) -> ShellTabCloseResult {
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .closeTab(tabID: tabID)
            )
            applyMutationResult(result)
            return .closed
        } catch ShellStateMutationError.lastTab {
            return .lastTab
        } catch ShellStateMutationError.tabNotFound {
            return .tabNotFound
        } catch {
            return .tabNotFound
        }
    }

    func closePane(paneID: String) -> ShellPaneCloseResult {
        if let impact = closeGuardImpact(for: .paneSlot(paneID)) {
            return .requiresConfirmation(impact)
        }
        return applyClosePaneMutation(paneID: paneID)
    }

    @discardableResult
    func requestClosePane(paneID: String) -> Bool {
        switch closePane(paneID: paneID) {
        case .closed:
            return true
        case .requiresConfirmation(let impact):
            return confirmAndApplyClose(impact)
        case .paneNotFound, .lastTab:
            return false
        }
    }

    private func applyClosePaneMutation(paneID: String) -> ShellPaneCloseResult {
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .closePane(paneSlotID: paneID)
            )
            applyMutationResult(result)
            return .closed
        } catch ShellStateMutationError.lastTab {
            return .lastTab
        } catch ShellStateMutationError.paneNotFound {
            return .paneNotFound
        } catch {
            return .paneNotFound
        }
    }

    @discardableResult
    func closePaneAfterTerminalRuntimeExit(paneID: String) -> Bool {
        guard !terminalAutoCloseIsSuppressed(paneID: paneID) else { return false }
        return applyClosePaneMutation(paneID: paneID) == .closed
    }

    private func confirmAndApplyClose(_ impact: ShellCloseGuardImpact) -> Bool {
        guard closeConfirmationPresenter.confirmClose(impact: impact) else {
            return false
        }
        return withTerminalAutoCloseSuppressed(for: impact.affectedTerminalContentIDs) {
            let gracefullyRequestedContentIDs = requestGracefulShutdownForConfirmedClose(impact)
            waitForGracefulShutdownDrain(contentIDs: gracefullyRequestedContentIDs)
            let capturedTranscripts = captureTerminalTranscriptSnapshots(for: impact)
            return applyConfirmedClose(impact, transcriptSnapshotOverrides: capturedTranscripts)
        }
    }

    private func withTerminalAutoCloseSuppressed<T>(
        for contentIDs: [String],
        operation: () -> T
    ) -> T {
        guard !contentIDs.isEmpty else { return operation() }
        let previous = terminalContentIDsSuppressingAutoClose
        terminalContentIDsSuppressingAutoClose.formUnion(contentIDs)
        defer {
            terminalContentIDsSuppressingAutoClose = previous
        }
        return operation()
    }

    @discardableResult
    private func requestGracefulShutdownForConfirmedClose(
        _ impact: ShellCloseGuardImpact
    ) -> [String] {
        let reason = gracefulShutdownReason(for: impact.scope)
        var requestedContentIDs: [String] = []
        var seenContentIDs: Set<String> = []
        for contentID in impact.activeTerminalContentIDs
            where seenContentIDs.insert(contentID).inserted
        {
            let result = terminalRuntimeRegistry.requestGracefulShutdown(
                forTerminalContentID: contentID,
                reason: reason
            )
            if result.wasRequested {
                requestedContentIDs.append(contentID)
            } else if result.code != .alreadyExited {
                recordControlPlaneDiagnostic(
                    "terminal graceful shutdown request \(result.code.rawValue) for \(contentID)"
                )
            }
        }
        return requestedContentIDs
    }

    private func waitForGracefulShutdownDrain(contentIDs: [String]) {
        guard gracefulShutdownTimeout > 0, !contentIDs.isEmpty else { return }
        let deadline = Date().addingTimeInterval(gracefulShutdownTimeout)
        while Date() < deadline {
            if contentIDs.allSatisfy({ terminalGracefulShutdownSettled(contentID: $0) }) {
                return
            }
            let remaining = max(0, deadline.timeIntervalSinceNow)
            _ = RunLoop.current.run(
                mode: .default,
                before: Date().addingTimeInterval(
                    min(Self.gracefulShutdownPollInterval, remaining)
                )
            )
        }

        let timedOutContentIDs = contentIDs.filter {
            !terminalGracefulShutdownSettled(contentID: $0)
        }
        guard !timedOutContentIDs.isEmpty else { return }
        recordControlPlaneDiagnostic(
            "terminal graceful shutdown timed out for \(timedOutContentIDs.joined(separator: ","))"
        )
    }

    private func terminalGracefulShutdownSettled(contentID: String) -> Bool {
        let runtime = terminalRuntimeRegistry.snapshot(forTerminalContentID: contentID)
        let metadata = runtime.paneMetadata
        if metadata.processExited {
            return true
        }
        if let activeTaskState = metadata.activeTaskState {
            return !activeTaskState.protectsFromPruning
        }
        return !terminalRuntimeRegistry.registeredContentIDs.contains(contentID)
    }

    private func gracefulShutdownReason(
        for scope: ShellCloseGuardScope
    ) -> TerminalRuntimeGracefulShutdownReason {
        switch scope {
        case .paneSlot:
            return .paneClose
        case .tab:
            return .tabClose
        case .window:
            return .windowClose
        case .app:
            return .appQuit
        }
    }

    @discardableResult
    private func applyConfirmedClose(
        _ impact: ShellCloseGuardImpact,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot] = [:]
    ) -> Bool {
        switch impact.scope {
        case .paneSlot(let paneID):
            return applyClosePaneMutation(paneID: paneID) == .closed
        case .tab(let tabID):
            return applyCloseTabMutation(tabID: tabID) == .closed
        case .window, .app:
            persistenceCoordinator.syncManifestFromShellState(
                transcriptSnapshotOverrides: transcriptSnapshotOverrides,
                makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                    self?.makeWorkspaceManifestFromShellState(
                        now: now,
                        transcriptSnapshotOverrides: transcriptSnapshotOverrides
                    )
                },
                makePinnedSnapshot: { [weak self] tabID in
                    self?.makePinnedTabSnapshot(tabID: tabID)
                }
            )
            shutdownTerminalRuntimes()
            return true
        }
    }

    func closeGuardImpact(for scope: ShellCloseGuardScope) -> ShellCloseGuardImpact? {
        let paneIDs = terminalPaneIDsAffected(by: scope)
        guard !paneIDs.isEmpty else { return nil }
        let contentState = shellState.contentStateProjection()
        let affectedContentIDs = paneIDs.compactMap { paneID -> String? in
            terminalContentIDForCloseGuard(paneID: paneID, contentState: contentState)
        }
        let activeContentIDs = paneIDs.compactMap { paneID -> String? in
            guard let pane = shellState.pane(paneID: paneID),
                  let contentID = terminalContentIDForCloseGuard(
                    paneID: paneID,
                    contentState: contentState
                  ),
                  terminalRequiresCloseConfirmation(pane: pane, contentID: contentID)
            else {
                return nil
            }
            return contentID
        }
        let impact = ShellCloseGuardImpact(
            scope: scope,
            affectedTerminalContentIDs: affectedContentIDs,
            activeTerminalContentIDs: activeContentIDs
        )
        return impact.requiresConfirmation ? impact : nil
    }

    private func terminalContentIDForCloseGuard(
        paneID: String,
        contentState: ShellContentStateSnapshot
    ) -> String? {
        if let content = contentState.contentMounted(in: paneID),
           content.kind == .terminal
        {
            return content.contentID
        }
        return nil
    }

    private func terminalPaneIDsAffected(by scope: ShellCloseGuardScope) -> [String] {
        switch scope {
        case .paneSlot(let paneID):
            return shellState.pane(paneID: paneID).map { [$0.paneID] } ?? []
        case .tab(let tabID):
            return shellState.tab(tabID: tabID)?.paneTree.paneIDs ?? []
        case .window, .app:
            return shellState.spaces.flatMap(\.tabs).flatMap(\.paneTree.paneIDs)
        }
    }

    private func terminalRequiresCloseConfirmation(
        pane: ShellPane,
        contentID: String
    ) -> Bool {
        if pane.alanBinding?.pendingRequest == true {
            return true
        }
        if let processState = pane.context?.processState {
            if processState == "exited" {
                return false
            }
            if processState == ShellTabActiveTaskState.foregroundCommand.rawValue
                || processState == ShellTabActiveTaskState.alanRunning.rawValue
                || processState == ShellTabActiveTaskState.alanPendingYield.rawValue
                || processState == ShellTabActiveTaskState.alanProcess.rawValue
                || processState == ShellTabActiveTaskState.unknown.rawValue
            {
                return true
            }
        }
        let runtime = terminalRuntimeRegistry.snapshot(forTerminalContentID: contentID)
        let metadata = runtime.paneMetadata
        if metadata.processExited {
            return false
        }
        if let activeTaskState = metadata.activeTaskState {
            return activeTaskState.protectsFromPruning
        }
        return terminalRuntimeRegistry.registeredContentIDs.contains(contentID)
    }

    private func focusedPaneWorkingDirectory() -> String? {
        guard let pane = focusedPane ?? selectedPane else { return nil }
        let runtimeCwd = runtime(for: pane.paneID).paneMetadata.workingDirectory
        return nonEmptyWorkingDirectory(runtimeCwd)
            ?? nonEmptyWorkingDirectory(pane.cwd)
    }

    private func targetTerminalProfileID(in requestedSpaceID: String?, explicit: String?) -> String? {
        shellState.terminalProfileIDForNewTerminal(in: requestedSpaceID, explicit: explicit)
    }

    private func targetTerminalProfileID(forSplitFromPaneID paneID: String, explicit: String?) -> String? {
        shellState.terminalProfileIDForNewSplit(from: paneID, explicit: explicit)
    }

    private func nonEmptyWorkingDirectory(_ path: String?) -> String? {
        guard let path else { return nil }
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    @discardableResult
    private func closePaneAfterChildExitIfNeeded(
        paneID: String,
        processExited: Bool
    ) -> Bool {
        guard processExited else { return false }
        guard pane(paneID: paneID) != nil else { return false }
        guard !terminalAutoCloseIsSuppressed(paneID: paneID) else { return false }
        return applyClosePaneMutation(paneID: paneID) == .closed
    }

    private func terminalAutoCloseIsSuppressed(paneID: String) -> Bool {
        let contentState = shellState.contentStateProjection()
        let contentID = terminalContentIDForCloseGuard(
            paneID: paneID,
            contentState: contentState
        ) ?? pane(paneID: paneID)?.terminalContentID
        guard let contentID else { return false }
        return terminalContentIDsSuppressingAutoClose.contains(contentID)
    }

    func movePane(
        paneID: String,
        toTab targetTabID: String,
        direction: ShellSplitDirection
    ) -> Bool {
        let targetTabTitle = shellState.tab(tabID: targetTabID)?.title ?? targetTabID
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .movePaneToTab(
                    paneSlotID: paneID,
                    targetTabID: targetTabID,
                    direction: direction
                )
            )
            let annotatedResult = annotatingPaneViewport(
                result,
                paneID: paneID,
                fallbackSummary: "pane moved to \(targetTabTitle)"
            )
            applyMutationResult(annotatedResult)
            return true
        } catch {
            return false
        }
    }

    func movePaneWithinTab(
        paneID: String,
        placement: ShellPaneSplitDirection
    ) -> Bool {
        movePaneWithinTab(
            paneID: paneID,
            placement: placement,
            source: .explicitCommand
        )
    }

    func movePaneWithinTab(
        paneID: String,
        placement: ShellPaneSplitDirection,
        source: ShellPaneMovementInputSource
    ) -> Bool {
        guard ShellPaneMovementInteractionPolicy.terminalSelectionFirst
            .allowsPaneMovement(from: source)
        else {
            return false
        }

        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .movePaneWithinTab(paneSlotID: paneID, placement: placement)
            )
            applyMutationResult(result)
            if let tabID = result.tabID {
                controlPlane.recordPaneMovedInTab(
                    requestID: nil,
                    spaceID: result.spaceID,
                    tabID: tabID,
                    paneID: paneID,
                    placement: placement,
                    mountedContentInstanceID: paneID
                )
            }
            return true
        } catch {
            return false
        }
    }

    func liftPaneToTab(paneID: String, title: String? = nil) -> ShellPaneLiftResult {
        let resolvedTitle = title ?? shellState.pane(paneID: paneID)?.viewport?.title ?? "Lifted Pane"
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .movePaneToNewTab(
                    paneSlotID: paneID,
                    title: resolvedTitle
                )
            )
            let annotatedResult = annotatingPaneViewport(
                result,
                paneID: paneID,
                fallbackSummary: "pane moved to its own tab"
            )
            applyMutationResult(annotatedResult)
            return .lifted
        } catch ShellStateMutationError.lastPane {
            return .lastPane
        } catch ShellStateMutationError.paneNotFound {
            return .paneNotFound
        } catch {
            return .paneNotFound
        }
    }

    private func annotatingPaneViewport(
        _ result: ShellStateMutationResult,
        paneID: String,
        fallbackSummary: String,
        now: Date = .now
    ) -> ShellStateMutationResult {
        let formatter = ISO8601DateFormatter()
        let timestamp = formatter.string(from: now)
        let nextPanes = result.state.panes.map { pane in
            guard pane.paneID == paneID else { return pane }
            let viewport = ShellViewportSnapshot(
                title: pane.viewport?.title,
                summary: pane.viewport?.summary ?? fallbackSummary,
                visibleExcerpt: pane.viewport?.visibleExcerpt,
                lastActivityAt: timestamp
            )
            return ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.cwd,
                process: pane.process,
                attention: pane.attention,
                context: pane.context,
                viewport: viewport,
                activity: pane.activity,
                alanBinding: pane.alanBinding,
                terminalProfileID: pane.terminalProfileID
            )
        }
        let nextState = ShellStateSnapshot(
            contractVersion: result.state.contractVersion,
            windowID: result.state.windowID,
            focusedSpaceID: result.state.focusedSpaceID,
            focusedTabID: result.state.focusedTabID,
            focusedPaneID: result.state.focusedPaneID,
            spaces: result.state.spaces,
            panes: nextPanes,
            paneSlots: result.state.paneSlots,
            contents: result.state.contents
        )
        return ShellStateMutationResult(
            state: nextState,
            spaceID: result.spaceID,
            tabID: result.tabID,
            paneID: result.paneID
        )
    }

    private var totalTabCount: Int {
        shellState.spaces.reduce(into: 0) { partialResult, space in
            partialResult += space.tabs.count
        }
    }

    private func strongestAttention(in panes: [ShellPane]) -> ShellAttentionState {
        let now = Date()
        return panes
            .map { shellEffectiveAttention(for: $0, now: now) }
            .max(by: { Self.attentionRank(for: $0) < Self.attentionRank(for: $1) })
            ?? .idle
    }

    private func publishControlPlaneState(
        pinSnapshotTabIDs: Set<String> = [],
        coalesced: Bool = false
    ) {
        persistenceCoordinator.publishControlPlaneState(
            state: shellState,
            controlPlane: controlPlane,
            pinSnapshotTabIDs: pinSnapshotTabIDs,
            coalesced: coalesced,
            latestState: { [weak self] in self?.shellState },
            makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                self?.makeWorkspaceManifestFromShellState(
                    now: now,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
            },
            makePinnedSnapshot: { [weak self] tabID in
                self?.makePinnedTabSnapshot(tabID: tabID)
            }
        )
    }

    static func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .idle:
            return 0
        case .active:
            return 1
        case .notable:
            return 2
        case .awaitingUser:
            return 3
        }
    }
}

extension ShellHostController: ShellAutomationCommandHandling {
    func performShellAutomationCommand(
        _ command: ShellAutomationCommand
    ) -> ShellAutomationCommandResult {
        switch command {
        case .createTab(let request):
            let result: ShellStateMutationResult
            do {
                switch request.launchTarget {
                case .shell:
                    result = try openTerminalTabMutation(
                        in: request.spaceID,
                        title: request.title,
                        workingDirectory: request.workingDirectory,
                        terminalProfileID: request.terminalProfileID
                    )
                }
            } catch let error as ShellStateMutationError {
                return shellAutomationResult(
                    code: .missingTarget,
                    spaceID: request.spaceID,
                    errorCode: error.rawValue,
                    errorMessage: shellStateMutationErrorMessage(error)
                )
            } catch {
                return shellAutomationResult(
                    code: .rejected,
                    spaceID: request.spaceID,
                    errorCode: "shell_mutation_failed",
                    errorMessage: String(describing: error)
                )
            }
            applyMutationResult(result)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: result.tabID,
                paneID: shellState.focusedPaneID
            )

        case .splitPane(let request):
            guard pane(paneID: request.paneID) != nil else {
                return shellAutomationMissingPaneResult(request.paneID)
            }
            // Carry explicit launch fields through a terminal content intent so a requested cwd
            // or title is honored instead of falling back to the source/default launch settings.
            let contentIntent: ShellContentIntent? =
                (request.title != nil || request.workingDirectory != nil)
                ? .terminal(
                    launchTarget: .shell,
                    title: request.title,
                    workingDirectory: request.workingDirectory
                )
                : nil
            guard let paneID = splitPane(
                paneID: request.paneID,
                placement: request.placement,
                contentIntent: contentIntent,
                terminalProfileID: request.terminalProfileID
            ) else {
                return shellAutomationMissingPaneResult(request.paneID)
            }
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )

        case .focusPane(let paneID):
            guard pane(paneID: paneID) != nil else {
                return shellAutomationMissingPaneResult(paneID)
            }
            focus(paneID: paneID)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )

        case .closePane(let paneID):
            switch closePane(paneID: paneID) {
            case .closed:
                return shellAutomationResult(
                    code: .accepted,
                    spaceID: shellState.focusedSpaceID,
                    tabID: shellState.focusedTabID,
                    paneID: shellState.focusedPaneID
                )
            case .paneNotFound:
                return shellAutomationMissingPaneResult(paneID)
            case .lastTab:
                return shellAutomationResult(
                    code: .lastTab,
                    paneID: paneID,
                    errorCode: "last_tab",
                    errorMessage: "alan terminal workspace must keep at least one pane open."
                )
            case .requiresConfirmation(let impact):
                return shellAutomationCloseRequiresConfirmationResult(
                    impact: impact,
                    tabID: shellState.pane(paneID: paneID)?.tabID,
                    paneID: paneID
                )
            }

        case .closeTab(let tabID):
            switch closeTab(tabID: tabID) {
            case .closed:
                return shellAutomationResult(
                    code: .accepted,
                    tabID: tabID,
                    paneID: shellState.focusedPaneID
                )
            case .tabNotFound:
                return shellAutomationResult(
                    code: .missingTarget,
                    tabID: tabID,
                    errorCode: "tab_not_found",
                    errorMessage: "The requested tab does not exist."
                )
            case .lastTab:
                return shellAutomationResult(
                    code: .lastTab,
                    tabID: tabID,
                    errorCode: "last_tab",
                    errorMessage: "alan terminal workspace must keep at least one tab open."
                )
            case .requiresConfirmation(let impact):
                return shellAutomationCloseRequiresConfirmationResult(
                    impact: impact,
                    tabID: tabID,
                    paneID: shellState.tab(tabID: tabID)?.paneTree.paneIDs.first
                )
            }

        case .sendText(let request):
            let delivery: TerminalRuntimeDeliveryResult
            if let terminalContentID = request.terminalContentID {
                delivery = terminalRuntimeRegistry.sendText(
                    toTerminalContentID: terminalContentID,
                    text: request.text
                )
            } else {
                delivery = terminalRuntimeRegistry.sendText(
                    to: request.paneID,
                    text: request.text
                )
            }
            return shellAutomationResult(
                code: shellAutomationResultCode(for: delivery),
                paneID: request.paneID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .sendKey(let request):
            let delivery: TerminalRuntimeDeliveryResult
            if let terminalContentID = request.terminalContentID {
                delivery = terminalRuntimeRegistry.sendKey(
                    toTerminalContentID: terminalContentID,
                    key: request.key
                )
            } else {
                delivery = terminalRuntimeRegistry.sendKey(
                    to: request.paneID,
                    key: request.key
                )
            }
            return shellAutomationResult(
                code: shellAutomationResultCode(for: delivery),
                paneID: request.paneID,
                acceptedBytes: delivery.acceptedBytes,
                deliveryCode: delivery.code.rawValue,
                runtimePhase: delivery.runtimePhase,
                errorCode: delivery.errorCode,
                errorMessage: delivery.errorMessage
            )

        case .readPaneSummary(let paneID):
            guard let summary = shellState.automationPaneSummary(paneID: paneID) else {
                return shellAutomationMissingPaneResult(paneID)
            }
            return shellAutomationResult(
                code: .accepted,
                summary: summary,
                spaceID: summary.spaceID,
                tabID: summary.tabID,
                paneID: summary.paneID
            )

        case .activateAttentionItem(let paneID):
            guard pane(paneID: paneID) != nil else {
                return shellAutomationMissingPaneResult(paneID)
            }
            focus(paneID: paneID, requestTerminalFocus: true)
            return shellAutomationResult(
                code: .accepted,
                spaceID: shellState.focusedSpaceID,
                tabID: shellState.focusedTabID,
                paneID: paneID
            )
        }
    }

    private func shellAutomationMissingPaneResult(_ paneID: String) -> ShellAutomationCommandResult {
        shellAutomationResult(
            code: .missingTarget,
            paneID: paneID,
            errorCode: "pane_not_found",
            errorMessage: "The requested pane does not exist."
        )
    }

    private func shellAutomationCloseRequiresConfirmationResult(
        impact: ShellCloseGuardImpact,
        tabID: String? = nil,
        paneID: String? = nil
    ) -> ShellAutomationCommandResult {
        shellAutomationResult(
            code: .requiresConfirmation,
            tabID: tabID,
            paneID: paneID,
            errorCode: "requires_confirmation",
            errorMessage: "The requested close contains active terminal work and requires confirmation."
        )
    }

    private func shellStateMutationErrorMessage(_ error: ShellStateMutationError) -> String {
        switch error {
        case .spaceNotFound:
            return "The requested space does not exist."
        case .tabNotFound:
            return "The requested tab does not exist."
        case .paneNotFound:
            return "The requested pane does not exist."
        case .unsupportedContent:
            return "This action requires terminal content."
        case .splitNotFound:
            return "The requested split does not exist."
        case .spatialFocusTargetNotFound:
            return "There is no pane in that direction."
        case .lastTab:
            return "alan terminal workspace must keep at least one tab open."
        case .lastPane:
            return "alan terminal workspace must keep at least one pane open."
        case .invalidMoveTarget:
            return "The requested move target is not available."
        case .invalidTabOrganizationTarget:
            return "The requested tab organization target is not available."
        }
    }

    private func shellAutomationResult(
        code: ShellAutomationCommandResultCode,
        summary: ShellAutomationPaneSummary? = nil,
        spaceID: String? = nil,
        tabID: String? = nil,
        paneID: String? = nil,
        acceptedBytes: Int? = nil,
        deliveryCode: String? = nil,
        runtimePhase: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) -> ShellAutomationCommandResult {
        let resolvedSummary = summary ?? paneID.flatMap {
            shellState.automationPaneSummary(paneID: $0)
        }
        return ShellAutomationCommandResult(
            code: code,
            summary: resolvedSummary,
            spaceID: spaceID ?? resolvedSummary?.spaceID,
            tabID: tabID ?? resolvedSummary?.tabID,
            paneID: paneID ?? resolvedSummary?.paneID,
            acceptedBytes: acceptedBytes,
            deliveryCode: deliveryCode,
            runtimePhase: runtimePhase,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    private func shellAutomationResultCode(
        for delivery: TerminalRuntimeDeliveryResult
    ) -> ShellAutomationCommandResultCode {
        switch delivery.code {
        case .accepted:
            return .accepted
        case .queued:
            return .queued
        case .rejected:
            return .rejected
        case .missingTarget:
            return .missingTarget
        case .unavailableRuntime:
            return .runtimeUnavailable
        case .timeout:
            return .timeout
        }
    }
}

extension ShellHostController {
    static let spikePreview = ShellHostController(shellState: .bootstrapDefault())
}
#endif
