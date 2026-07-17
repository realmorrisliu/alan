import Foundation

#if os(macOS)
import Combine

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
        fileManager _: FileManager = .default,
        windowID: String = "window_\(UUID().uuidString.lowercased())",
        installChannel: AlanInstallChannel = .current(),
        terminalRuntimeRegistry: TerminalRuntimeRegistry? = nil
    ) -> ShellWindowContext {
        ShellWindowContext(
            windowID: windowID,
            installChannel: installChannel,
            terminalRuntimeRegistry: terminalRuntimeRegistry ?? TerminalRuntimeRegistry()
        )
    }
}

@MainActor
final class ShellHostController: ObservableObject, TerminalHostActivationDelegate {
    static let gracefulShutdownPollInterval: TimeInterval = 0.05
    static let iso8601Formatter = ISO8601DateFormatter()
    private let fileManager: FileManager
    private let windowContext: ShellWindowContext
    let persistenceCoordinator: ShellWorkspacePersistenceCoordinator
    let actionCoordinator = ShellActionCoordinator()
    let reducerCoordinator = ShellReducerCommandCoordinator()
    var terminalActiveTasksByPaneID: [String: ShellTabActiveTaskState] = [:]
    var terminalContentIDsSuppressingAutoClose: Set<String> = []
    private let paneProjection: ShellPaneProjectionService
    let platformMetadataPreserver: ShellPlatformMetadataPreserver
    let terminalContentProjection: TerminalContentProjectionAdapter
    let terminalContentLifecycle = TerminalContentLifecycleAdapter()
    let pasteboard: ShellPasteboardAccessing
    let closeConfirmationPresenter: ShellCloseConfirmationPresenting
    let gracefulShutdownTimeout: TimeInterval
    let performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?
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

    @Published var shellState: ShellStateSnapshot
    @Published var selectedSpaceID: String?
    @Published var selectedTabID: String?
    @Published var lastCopiedAt: Date?
    var terminalRuntime: TerminalHostRuntimeSnapshot = .placeholder
    @Published var controlPlaneDiagnostics: [String] = []
    @Published var activityNotifications: [ShellActivityNotificationRoute] = []
    @Published var zoomedPaneIDByTabID: [String: String] = [:]
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
    let bootProfileCache: AlanShellBootProfileCache
    let appIsActiveProvider: @MainActor () -> Bool
    var routedActivityNotificationKeys: Set<String> = []
    var pendingVisibleBackgroundRuntimeByPaneID: [String: TerminalHostRuntimeSnapshot] = [:]
    var visibleBackgroundRuntimeProjectionScheduled = false
    var shellWindowIsVisibleForRendering = true
    var workspaceManifest: ShellContentWorkspaceManifest? {
        persistenceCoordinator.currentManifest()
    }

    init(
        shellState: ShellStateSnapshot,
        fileManager: FileManager = .default,
        windowContext: ShellWindowContext? = nil,
        terminalRuntimeRegistry: TerminalRuntimeRegistry? = nil,
        workspaceManifestStore: ShellWorkspaceManifestStore? = nil,
        workspaceManifest: ShellContentWorkspaceManifest? = nil,
        persistenceWriter: ShellPersistenceWriting? = nil,
        manifestFlushScheduler: ManifestFlushScheduling? = nil,
        pasteboard: ShellPasteboardAccessing? = nil,
        closeConfirmationPresenter: ShellCloseConfirmationPresenting? = nil,
        gracefulShutdownTimeout: TimeInterval = 3.0,
        performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil,
        bootProfileCache: AlanShellBootProfileCache? = nil,
        appIsActiveProvider: @escaping @MainActor () -> Bool = {
            ShellAppActivityProvider.isActive
        }
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
        self.persistenceCoordinator = ShellWorkspacePersistenceCoordinator(
            manifestStore: workspaceManifestStore,
            workspaceManifest: workspaceManifest,
            persistenceWriter: persistenceWriter,
            manifestFlushScheduler: manifestFlushScheduler
        )
        self.pasteboard = pasteboard ?? ShellSystemPasteboard()
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

    /// Persist only renderer-owned Agent offsets and presentation. Process identity is immutable.
    func updateAgentRendererState(
        paneID: String,
        offsets: AlanAgentStreamOffsets,
        presentation: AlanAgentContentPresentation
    ) {
        do {
            let result = try reducerCoordinator.apply(
                state: shellState,
                operation: .updateAgentRendererState(
                    paneSlotID: paneID,
                    offsets: offsets,
                    presentation: presentation
                )
            )
            applyMutationResult(result)
        } catch {
            recordControlPlaneDiagnostic(
                "Agent renderer state update failed for \(paneID): \(error)"
            )
        }
    }

    static func live(
        fileManager: FileManager = .default,
        windowContext: ShellWindowContext? = nil,
        workspaceManifestURL: URL? = nil,
        defaultWorkingDirectory: String? = nil,
        now: Date = .now
    ) -> ShellHostController {
        let installChannel = AlanInstallChannel.current()
        let resolvedWindowContext =
            windowContext
            ?? ShellWindowContext.make(
                fileManager: fileManager,
                windowID: "window_main",
                installChannel: installChannel
            )
        let startup = ShellWorkspaceManifestStartupCoordinator(fileManager: fileManager).prepare(
            windowContext: resolvedWindowContext,
            workspaceManifestURL: workspaceManifestURL,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )

        let controller = ShellHostController(
            shellState: startup.shellState,
            fileManager: fileManager,
            windowContext: resolvedWindowContext,
            workspaceManifestStore: startup.manifestStore,
            workspaceManifest: startup.workspaceManifest
        )
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
}

extension ShellHostController {
    static let spikePreview = ShellHostController(shellState: .bootstrapDefault())
}
#endif
