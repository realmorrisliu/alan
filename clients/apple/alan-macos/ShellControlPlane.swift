import Foundation

#if os(macOS)
func alanShellControlNamespace(
    channel: AlanInstallChannel = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> String {
    guard let override = environment["ALAN_SHELL_CONTROL_NAMESPACE"]?
        .trimmingCharacters(in: .whitespacesAndNewlines),
        !override.isEmpty
    else {
        return channel.shellControlNamespace
    }

    let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_."))
    let sanitized = String(
        override.unicodeScalars.map { scalar in
            allowed.contains(scalar) ? Character(scalar) : "-"
        }
    )
    .trimmingCharacters(in: CharacterSet(charactersIn: "-_."))

    return sanitized.isEmpty ? channel.shellControlNamespace : sanitized
}

func alanShellControlPlaneRootURL(
    windowID: String,
    fileManager: FileManager = .default,
    channel: AlanInstallChannel = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> URL {
    fileManager.temporaryDirectory
        .appendingPathComponent(
            alanShellControlNamespace(channel: channel, environment: environment),
            isDirectory: true
        )
        .appendingPathComponent(windowID, isDirectory: true)
}

func alanShellControlPlaneSocketURL(
    windowID: String,
    fileManager: FileManager = .default,
    channel: AlanInstallChannel = .current()
) -> URL {
    alanShellControlPlaneRootURL(windowID: windowID, fileManager: fileManager, channel: channel)
        .appendingPathComponent("shell.sock")
}

func alanShellPaneSupportDirectoryURL(
    windowID: String,
    paneID: String,
    fileManager: FileManager = .default,
    channel: AlanInstallChannel = .current()
) -> URL {
    alanShellControlPlaneRootURL(windowID: windowID, fileManager: fileManager, channel: channel)
        .appendingPathComponent("panes", isDirectory: true)
        .appendingPathComponent(paneID, isDirectory: true)
}

func alanShellBindingFileURL(
    windowID: String,
    paneID: String,
    fileManager: FileManager = .default,
    channel: AlanInstallChannel = .current()
) -> URL {
    alanShellPaneSupportDirectoryURL(
        windowID: windowID,
        paneID: paneID,
        fileManager: fileManager,
        channel: channel
    )
        .appendingPathComponent("alan-binding.json")
}

@MainActor
final class AlanShellControlPlane {
    private let windowID: String
    private let fileManager: FileManager
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder
    private let rootURL: URL
    private let socketURL: URL
    private let panesURL: URL
    private let commandsURL: URL
    private let resultsURL: URL
    private let stateFileURL: URL
    private let commandHandler: (AlanShellControlCommand) -> AlanShellControlResponse
    private let stateAdoptionHandler: @MainActor (ShellStateSnapshot) -> Void
    private let diagnostics: AlanShellDiagnostics
    private let socketServer: AlanShellSocketServer
    private let eventStore: AlanShellEventStore
    private var filePoller: AlanShellControlFilePoller?
    private var trackedPaneIDs: Set<String> = []
    private let stateFileQueue = DispatchQueue(
        label: "app.alan.shell.control-plane-state",
        qos: .utility
    )
    private var pendingStateFileWrite: DispatchWorkItem?
    private var latestMergedState: ShellStateSnapshot?
    private var lastPersistedState: ShellStateSnapshot?

    init(
        windowID: String,
        fileManager: FileManager = .default,
        channel: AlanInstallChannel = .current(),
        commandHandler: @escaping (AlanShellControlCommand) -> AlanShellControlResponse,
        stateAdoptionHandler: @escaping @MainActor (ShellStateSnapshot) -> Void,
        bindingProjectionHandler: @escaping @MainActor (String, ShellAlanBinding?) -> Void,
        diagnosticHandler: @escaping @MainActor (String) -> Void = { _ in }
    ) {
        self.windowID = windowID
        self.fileManager = fileManager
        self.encoder = JSONEncoder()
        self.encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        self.decoder = JSONDecoder()
        self.rootURL = alanShellControlPlaneRootURL(
            windowID: windowID,
            fileManager: fileManager,
            channel: channel
        )
        self.socketURL = alanShellControlPlaneSocketURL(
            windowID: windowID,
            fileManager: fileManager,
            channel: channel
        )
        self.panesURL = rootURL.appendingPathComponent("panes", isDirectory: true)
        self.commandsURL = rootURL.appendingPathComponent("commands", isDirectory: true)
        self.resultsURL = rootURL.appendingPathComponent("results", isDirectory: true)
        self.stateFileURL = rootURL.appendingPathComponent("state.json")
        self.commandHandler = commandHandler
        self.stateAdoptionHandler = stateAdoptionHandler
        self.diagnostics = AlanShellDiagnostics(handler: diagnosticHandler)
        self.eventStore = AlanShellEventStore(
            windowID: windowID,
            fileManager: fileManager,
            eventsFileURL: rootURL.appendingPathComponent("events.jsonl"),
            encoder: encoder,
            diagnosticHandler: diagnostics.record
        )
        self.socketServer = AlanShellSocketServer(
            socketURL: self.socketURL,
            commandHandler: commandHandler,
            stateAdoptionHandler: { state in
                Task { @MainActor in
                    stateAdoptionHandler(state)
                }
            },
            sideEffectHandler: { _ in }
        )
        self.filePoller = nil
        self.filePoller = AlanShellControlFilePoller(
            windowID: windowID,
            fileManager: fileManager,
            channel: channel,
            commandsURL: commandsURL,
            resultsURL: resultsURL,
            encoder: encoder,
            decoder: decoder,
            commandHandler: { [weak self, commandHandler] command in
                guard let self else { return commandHandler(command) }
                return self.responseForPolledCommand(command)
            },
            bindingProjectionHandler: bindingProjectionHandler,
            diagnosticHandler: diagnostics.record
        )

        ensureDirectories()
        socketServer.start()
        filePoller?.start()
    }

    deinit {
        socketServer.stop()
    }

    var rootPath: String {
        rootURL.path
    }

    var stateFilePath: String {
        stateFileURL.path
    }

    var commandsPath: String {
        commandsURL.path
    }

    var resultsPath: String {
        resultsURL.path
    }

    var socketPath: String {
        socketURL.path
    }

    var latestEventID: String? {
        eventStore.latestEventID
    }

    /// Full publish: prompt in-memory merge plus deferred disk/event persistence.
    /// Used by structural mutations and the debounced flush.
    func publish(state: ShellStateSnapshot) {
        publishInMemory(state: state)
        persistPublished()
    }

    /// Prompt path: updates the published-state cache that shell IPC clients read
    /// (`.state` / `.pane.list` / `.pane.snapshot`) and ensures pane support
    /// directories + binding-file poller tracking exist for the current panes.
    /// The pane-directory work only touches disk for newly-appeared panes, so it
    /// is cheap on the steady terminal-output path while keeping boot/structural
    /// pane setup prompt (a restored pane's child needs its binding directory
    /// before it can write `alan-binding.json`).
    @discardableResult
    func publishInMemory(state: ShellStateSnapshot) -> ShellStateSnapshot {
        let mergeResult = socketServer.mergePublishedState(state)
        latestMergedState = mergeResult.merged
        synchronizePaneSupportDirectories(for: mergeResult.merged)
        return mergeResult.merged
    }

    /// Deferred persistence for the latest in-memory state: the change-event log
    /// (coalesced since the last persist) and the `state.json` mirror (encode +
    /// write off the main thread). Run from the debounced flush, not the
    /// per-callback path.
    func persistPublished() {
        guard let mergedState = latestMergedState else { return }
        ensureDirectories()
        eventStore.recordChanges(from: lastPersistedState, to: mergedState)
        lastPersistedState = mergedState
        scheduleStateFilePersist()
    }

    /// Forces the latest published state to `state.json` synchronously. Call on
    /// app background/quit so the on-disk mirror is current before exit.
    func flushStateFile() {
        pendingStateFileWrite?.cancel()
        pendingStateFileWrite = nil
        guard let mergedState = latestMergedState else { return }
        let url = stateFileURL
        let succeeded = stateFileQueue.sync { Self.writeStateFile(mergedState, to: url) }
        if !succeeded {
            diagnostics.record("Failed to persist shell state to \(url.lastPathComponent)")
        }
    }

    private func scheduleStateFilePersist() {
        pendingStateFileWrite?.cancel()
        guard let mergedState = latestMergedState else { return }
        let url = stateFileURL
        let item = DispatchWorkItem { [weak self] in
            guard Self.writeStateFile(mergedState, to: url) == false else { return }
            DispatchQueue.main.async {
                self?.diagnostics.record("Failed to persist shell state to \(url.lastPathComponent)")
            }
        }
        pendingStateFileWrite = item
        stateFileQueue.asyncAfter(deadline: .now() + .milliseconds(150), execute: item)
    }

    @discardableResult
    private static func writeStateFile(_ state: ShellStateSnapshot, to url: URL) -> Bool {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        do {
            let data = try encoder.encode(state)
            try data.write(to: url, options: .atomic)
            return true
        } catch {
            return false
        }
    }

    private func ensureDirectories() {
        [rootURL, panesURL, commandsURL, resultsURL].forEach { url in
            do {
                try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
            } catch {
                diagnostics.record("Failed to create shell control directory \(url.path): \(error.localizedDescription)")
            }
        }
    }

    func specialCommandResponse(for command: AlanShellControlCommand) -> AlanShellControlResponse? {
        guard command.command == .eventsRead else { return nil }
        let rows = eventStore.read(afterEventID: command.afterEventID, limit: command.limit)
        return AlanShellControlResponse(
            requestID: command.requestID,
            contractVersion: ShellContentStateSnapshot.currentContractVersion,
            applied: true,
            state: nil,
            spaces: nil,
            tabs: nil,
            panes: nil,
            pane: nil,
            items: nil,
            candidates: nil,
            events: rows,
            focusedPaneID: nil,
            spaceID: nil,
            tabID: nil,
            paneID: nil,
            acceptedBytes: nil,
            deliveryCode: nil,
            runtimePhase: nil,
            latestEventID: eventStore.latestEventID,
            errorCode: nil,
            errorMessage: nil
        )
    }

    private func responseForPolledCommand(
        _ command: AlanShellControlCommand
    ) -> AlanShellControlResponse {
        specialCommandResponse(for: command)
            ?? socketServer.handleLocally(command)
            ?? commandHandler(command)
    }

    func recordTextDelivery(
        requestID: String,
        spaceID: String?,
        tabID: String?,
        paneID: String,
        contentID: String,
        delivery: TerminalRuntimeDeliveryResult
    ) {
        eventStore.recordTextDelivery(
            requestID: requestID,
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            contentID: contentID,
            delivery: delivery
        )
    }

    func recordContentCommandRejected(
        requestID: String,
        command: AlanShellControlCommandKind,
        spaceID: String?,
        tabID: String?,
        paneSlotID: String,
        content: ShellContentInstance,
        errorCode: String,
        errorMessage: String
    ) {
        eventStore.recordContentCommandRejected(
            requestID: requestID,
            command: command.rawValue,
            spaceID: spaceID,
            tabID: tabID,
            paneSlotID: paneSlotID,
            content: content,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    func recordSplitEqualized(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        changedSplitIDs: [String],
        affectedPaneIDs: [String]
    ) {
        eventStore.recordSplitEqualized(
            requestID: requestID,
            spaceID: spaceID,
            tabID: tabID,
            changedSplitIDs: changedSplitIDs,
            affectedPaneIDs: affectedPaneIDs
        )
    }

    func recordZoomStateChanged(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        paneID: String?,
        zoomedPaneID: String?
    ) {
        eventStore.recordZoomStateChanged(
            requestID: requestID,
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            zoomedPaneID: zoomedPaneID
        )
    }

    func recordSpatialFocus(
        requestID: String?,
        spaceID: String?,
        tabID: String?,
        previousPaneID: String?,
        currentPaneID: String?,
        direction: ShellSpatialFocusDirection,
        applied: Bool
    ) {
        eventStore.recordSpatialFocus(
            requestID: requestID,
            spaceID: spaceID,
            tabID: tabID,
            previousPaneID: previousPaneID,
            currentPaneID: currentPaneID,
            direction: direction,
            applied: applied
        )
    }

    func recordPaneMovedInTab(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        paneID: String,
        placement: ShellPaneSplitDirection,
        mountedContentInstanceID: String
    ) {
        eventStore.recordPaneMovedInTab(
            requestID: requestID,
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            placement: placement,
            mountedContentInstanceID: mountedContentInstanceID
        )
    }

    private func synchronizePaneSupportDirectories(for state: ShellStateSnapshot) {
        let paneIDs = Set(state.panes.map(\.paneID))
        let previousPaneIDs = trackedPaneIDs
        trackedPaneIDs = paneIDs
        filePoller?.updateTrackedPaneIDs(paneIDs)

        for paneID in paneIDs.subtracting(previousPaneIDs) {
            let paneURL = alanShellPaneSupportDirectoryURL(
                windowID: windowID,
                paneID: paneID,
                fileManager: fileManager
            )
            do {
                try fileManager.createDirectory(at: paneURL, withIntermediateDirectories: true)
            } catch {
                diagnostics.record("Failed to create pane support directory \(paneURL.path): \(error.localizedDescription)")
            }
        }

        for paneID in previousPaneIDs.subtracting(paneIDs) {
            let paneURL = alanShellPaneSupportDirectoryURL(
                windowID: windowID,
                paneID: paneID,
                fileManager: fileManager
            )
            do {
                try fileManager.removeItem(at: paneURL)
            } catch {
                diagnostics.record("Failed to remove stale pane support directory \(paneURL.path): \(error.localizedDescription)")
            }
        }
    }
}
#endif
