#if os(macOS)
import Foundation

@MainActor
final class AlanWindowTerminalRuntimeService: AlanTerminalRuntimeService {
    typealias SurfaceFactory = (String, String, AlanGhosttyProcessBootstrap) -> AlanTerminalSurfaceHandle

    private let bootstrap: AlanGhosttyProcessBootstrap
    private let ptyRuntime: AlanTerminalPtyRuntime
    private let makeSurfaceHandle: SurfaceFactory
    private var handlesByContentID: [String: AlanTerminalSurfaceHandle] = [:]
    private var restoredTranscriptSnapshotsByContentID: [String: TerminalTranscriptSnapshot] = [:]
    let renderCoordinator: TerminalRenderCoordinator

    init(
        renderCoordinator: TerminalRenderCoordinator = TerminalRenderCoordinator(),
        ptyRuntime: AlanTerminalPtyRuntime? = nil,
        surfaceFactory: SurfaceFactory? = nil
    ) {
        self.renderCoordinator = renderCoordinator
        self.bootstrap = AlanDefaultGhosttyProcessBootstrap.shared
        let ptyRuntime = ptyRuntime ?? Self.makeDefaultPtyRuntime()
        self.ptyRuntime = ptyRuntime
        let coordinator = renderCoordinator
        self.makeSurfaceHandle = surfaceFactory ?? { contentID, paneID, bootstrap in
            AlanGhosttySurfaceHandle(
                contentID: contentID,
                paneID: paneID,
                bootstrap: bootstrap,
                ptyRuntime: ptyRuntime,
                renderCoordinator: coordinator
            )
        }
    }

    init(
        bootstrap: AlanGhosttyProcessBootstrap,
        renderCoordinator: TerminalRenderCoordinator = TerminalRenderCoordinator(),
        ptyRuntime: AlanTerminalPtyRuntime? = nil,
        surfaceFactory: SurfaceFactory? = nil
    ) {
        self.renderCoordinator = renderCoordinator
        self.bootstrap = bootstrap
        let ptyRuntime = ptyRuntime ?? Self.makeDefaultPtyRuntime()
        self.ptyRuntime = ptyRuntime
        let coordinator = renderCoordinator
        self.makeSurfaceHandle = surfaceFactory ?? { contentID, paneID, bootstrap in
            AlanGhosttySurfaceHandle(
                contentID: contentID,
                paneID: paneID,
                bootstrap: bootstrap,
                ptyRuntime: ptyRuntime,
                renderCoordinator: coordinator
            )
        }
    }

    static func makeDefaultPtyRuntime(
        helperClient: AlanPrivilegedHelperClienting = AlanPrivilegedHelperAppClient()
    ) -> AlanTerminalPtyRuntime {
        AlanDarwinTerminalPtyRuntime(
            managedUserPtyProvider: AlanHelperManagedUserPtyProvider(helperClient: helperClient)
        )
    }

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        bootstrap.diagnostics
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    var registeredPaneIDs: Set<String> {
        Set(handlesByContentID.values.map(\.paneID))
    }

    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? {
        renderCoordinator.metricsSnapshot()
    }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        bootstrap.ensureReady()
    }

    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        ensureReady()
        if let handle = handlesByContentID[contentID] {
            handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
            if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
                handle.seedRestoredTranscriptSnapshot(restored)
            }
            return handle
        }
        let handle = makeSurfaceHandle(contentID, paneID, bootstrap)
        handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
        if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
            handle.seedRestoredTranscriptSnapshot(restored)
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult {
        guard let handle = handlesByContentID[contentID] else {
            return .failed(
                TerminalTranscriptCaptureFailure(
                    contentID: contentID,
                    code: .missingRuntime,
                    message: "No service-owned terminal runtime is registered for this content."
                )
            )
        }
        return buildTerminalTranscriptCapture(for: handle)
    }

    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        guard let handle = handlesByContentID[contentID] else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No service-owned terminal runtime is registered for this content."
            )
        }
        return handle.requestGracefulShutdown(reason: reason)
    }

    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    ) {
        let bounded = snapshot.boundedForManifest()
        restoredTranscriptSnapshotsByContentID[contentID] = bounded
        handlesByContentID[contentID]?.seedRestoredTranscriptSnapshot(bounded)
    }

    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        handlesByContentID[contentID]?.clearRestoredTranscriptSnapshot()
    }

    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a service-owned runtime."
            )
        }
        return handle.sendControlText(text)
    }

    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a service-owned runtime."
            )
        }
        return handle.sendControlKey(key)
    }

    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        guard let handle = handlesByContentID.removeValue(forKey: contentID) else {
            return .notStarted
        }
        let status = handle.teardown()
        ptyRuntime.unregisterHandle(forTerminalContentID: contentID)
        return status
    }

    func finalizeTerminalContents(excluding activeContentIDs: Set<String>) {
        let staleContentIDs = Set(handlesByContentID.keys).subtracting(activeContentIDs)
        staleContentIDs.forEach { finalizeTerminalContent($0) }
    }
}

#endif
