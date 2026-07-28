#if os(macOS)
import Darwin
import Foundation

@MainActor
final class AlanUnavailableManagedUserPtyProvider: AlanManagedUserPtyProviding {
    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        AlanUnavailableManagedUserPtyHandle(
            contentID: contentID,
            bootRequest: bootRequest,
            reason: "Managed User helper PTY provider is unavailable."
        )
    }
}

@MainActor
final class AlanHelperManagedUserPtyProvider: AlanManagedUserPtyProviding {
    let helperClient: AlanPrivilegedHelperClienting
    let defaultDimensions: AlanTerminalPtyDimensions
    let shell: String

    init(
        helperClient: AlanPrivilegedHelperClienting,
        defaultDimensions: AlanTerminalPtyDimensions = AlanTerminalPtyDimensions(columns: 80, rows: 24),
        shell: String = "/bin/zsh"
    ) {
        self.helperClient = helperClient
        self.defaultDimensions = defaultDimensions
        self.shell = shell
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        guard let accountName = bootRequest.managedUserAccountName?.trimmingCharacters(
            in: .whitespacesAndNewlines
        ), !accountName.isEmpty else {
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: "Managed User helper PTY request is missing an account."
            )
        }

        let status = helperClient.status()
        guard status.isHealthy else {
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: status.sanitizedMessage ?? "Managed User helper is unavailable."
            )
        }

        let request = AlanManagedUserPTYStartRequest(
            operationID: UUID().uuidString,
            channelID: status.identity.channelID,
            accountName: accountName,
            homeDirectory: ManagedTerminalAccountRequest.canonicalHomeDirectory(for: accountName),
            workingDirectory: bootRequest.workingDirectory,
            shell: shell,
            contentID: contentID,
            columns: defaultDimensions.columns,
            rows: defaultDimensions.rows
        )
        switch helperClient.startManagedUserPTY(request) {
        case .success(let session):
            return AlanHelperManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                helperClient: helperClient,
                session: session,
                initialDimensions: defaultDimensions
            )
        case .failure(let diagnostic):
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: diagnostic.sanitizedMessage
            )
        }
    }
}

@MainActor
final class AlanUnavailableManagedUserPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    private let reason: String
    let shellActivityState: AlanTerminalPtyShellActivityState = .unknown
    var onShellActivityStateChange: ((AlanTerminalPtyShellActivityState) -> Void)?

    init(
        contentID: String,
        bootRequest: AlanTerminalBootRequest,
        reason: String
    ) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        self.reason = reason
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: .failed,
            dimensions: nil,
            acceptedInputBytes: 0,
            inputClosed: true,
            lastSignal: nil,
            exitStatus: .unknown,
            transcriptLines: [reason]
        )
    }

    var isInputReady: Bool {
        false
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        .unavailable(errorMessage: reason, runtimePhase: AlanTerminalPtyRuntimePhase.failed.rawValue)
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        .rejected(.rejected("managed_user_helper_unavailable", message: reason))
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        .accepted("managed_user_helper_unavailable")
    }
}

@MainActor
final class AlanHelperManagedUserPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    let helperClient: AlanPrivilegedHelperClienting
    let session: AlanManagedUserPTYSession
    private(set) var dimensions: AlanTerminalPtyDimensions
    private(set) var phase: AlanTerminalPtyRuntimePhase = .running
    private(set) var inputClosed = false
    private(set) var lastSignal: AlanTerminalPtySignal?
    private(set) var exitStatus: AlanTerminalProcessExitStatus?
    private var acceptedInputBytes = 0
    private var cleanupRequested = false
    private var transcriptRingBufferLines: [String] = []
    private var rendererProxy: AlanHelperManagedUserPtyRendererProxy?
    private var observedFinalOutputChunk = false
    private let helperQueue: DispatchQueue
    private let outputProcessor = AlanTerminalPtyControlSequenceProcessor()
    private var outputPump: AlanHelperManagedUserPtyOutputPump?
    private var semanticShellState: AlanTerminalPtySemanticShellState?
    private var processGroupShellActivityState: AlanTerminalPtyShellActivityState?
    private var pendingRendererReplay = AlanTerminalPtyBoundedReplayBuffer(
        maxBytes: 1024 * 1024
    )
    private(set) var shellActivityState: AlanTerminalPtyShellActivityState = .unknown
    var onShellActivityStateChange: ((AlanTerminalPtyShellActivityState) -> Void)?

    init(
        contentID: String,
        bootRequest: AlanTerminalBootRequest,
        helperClient: AlanPrivilegedHelperClienting,
        session: AlanManagedUserPTYSession,
        initialDimensions: AlanTerminalPtyDimensions
    ) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        self.helperClient = helperClient
        self.session = session
        self.dimensions = initialDimensions
        self.helperQueue = DispatchQueue(
            label: "dev.alan.terminal.managed-user-pty.\(contentID)",
            qos: .userInteractive
        )
        let outputPump = AlanHelperManagedUserPtyOutputPump(
            ptyHandle: self,
            helperClient: helperClient,
            sessionID: session.sessionID,
            ioQueue: helperQueue,
            outputProcessor: outputProcessor
        )
        self.outputPump = outputPump
        outputPump.start()
    }

    deinit {
        outputPump?.invalidate()
        rendererProxy?.invalidate()
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        applyPendingOutput()
        if let rendererProxy, rendererProxy.invalidated {
            rendererProxyDidInvalidate(rendererProxy)
        }
        _ = drainAvailableOutput()
        refreshExitObservation()
        if observedFinalOutputChunk, exitStatus == nil {
            exitStatus = .unknown
        }
        return AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: phase,
            dimensions: dimensions,
            acceptedInputBytes: acceptedInputBytes,
            inputClosed: inputClosed,
            lastSignal: lastSignal,
            exitStatus: exitStatus,
            transcriptLines: transcriptRingBufferLines.isEmpty
                ? [session.sanitizedMessage]
                : transcriptRingBufferLines
        )
    }

    var isInputReady: Bool {
        phase == .running && !inputClosed && exitStatus == nil
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The managed-user terminal process has exited.",
                runtimePhase: phase.rawValue
            )
        }
        guard !inputClosed else {
            return .rejected(
                errorCode: "terminal_pty_input_closed",
                errorMessage: "The managed-user terminal PTY input stream is closed.",
                runtimePhase: phase.rawValue
            )
        }

        let result = helperClient.writeManagedUserPTY(
            AlanManagedUserPTYInputRequest(sessionID: session.sessionID, text: text)
        )
        guard result.accepted else {
            return .rejected(
                errorCode: helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_input_rejected"),
                errorMessage: result.diagnostic.sanitizedMessage,
                runtimePhase: phase.rawValue
            )
        }
        let byteCount = text.lengthOfBytes(using: .utf8)
        acceptedInputBytes += byteCount
        return .accepted(byteCount: byteCount, runtimePhase: phase.rawValue)
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        let nextDimensions = AlanTerminalPtyDimensions(columns: max(0, columns), rows: max(0, rows))
        let result = helperClient.resizeManagedUserPTY(
            AlanManagedUserPTYResizeRequest(
                sessionID: session.sessionID,
                columns: nextDimensions.columns,
                rows: nextDimensions.rows
            )
        )
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_resize_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        dimensions = nextDimensions
        return .accepted("resized")
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        guard !inputClosed else {
            return .accepted("input_closed")
        }
        let result = helperClient.closeManagedUserPTYInput(sessionID: session.sessionID)
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_eof_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        inputClosed = true
        phase = .inputClosed
        return .accepted("input_closed")
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        let result = helperClient.signalManagedUserPTY(
            AlanManagedUserPTYSignalRequest(
                sessionID: session.sessionID,
                signal: AlanManagedUserPTYSignal(signal)
            )
        )
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_signal_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        lastSignal = signal
        refreshExitObservation()
        return .accepted(signal.rawValue)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        if let rendererProxy {
            rendererProxy.invalidate()
            rendererProxyDidInvalidate(rendererProxy)
        }
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                .rejected(
                    "terminal_child_exited",
                    message: "The managed-user terminal process has exited."
                )
            )
        }
        guard phase == .running || phase == .inputClosed else {
            return .rejected(
                .rejected(
                    "managed_user_helper_pty_unavailable",
                    message: "Managed User helper PTY session is unavailable."
                )
            )
        }

        var descriptors = [Int32](repeating: -1, count: 2)
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            return .rejected(
                .rejected(
                    "managed_user_helper_renderer_socketpair_failed",
                    message: String(cString: strerror(errno))
                )
            )
        }

        setNonBlockingFileDescriptor(descriptors[0])
        setNonBlockingFileDescriptor(descriptors[1])
        setNoSigpipeSocketOption(descriptors[0])
        setNoSigpipeSocketOption(descriptors[1])

        guard let outputPump else {
            close(descriptors[0])
            close(descriptors[1])
            return .rejected(
                .rejected(
                    "managed_user_helper_pty_unavailable",
                    message: "Managed User helper PTY output service is unavailable."
                )
            )
        }
        let proxy = AlanHelperManagedUserPtyRendererProxy(
            ptyHandle: self,
            helperClient: helperClient,
            sessionID: session.sessionID,
            hostFileDescriptor: descriptors[0],
            ioQueue: helperQueue
        )
        let preparation = outputPump.attachRendererProxy(
            proxy,
            replayChunks: pendingRendererReplay.chunks,
            maxBytes: 4096
        )
        if preparation.attached {
            rendererProxy = proxy
            pendingRendererReplay.removeAll()
        }
        applyOutputUpdates(preparation.updates)
        guard preparation.attached,
              exitStatus == nil,
              phase == .running || phase == .inputClosed,
              !proxy.invalidated
        else {
            proxy.invalidate()
            close(descriptors[1])
            return .rejected(
                .rejected(
                    "managed_user_helper_renderer_attachment_failed",
                    message: "Managed User helper renderer attachment failed."
                )
            )
        }

        return .attached(
            AlanTerminalPtyRendererAttachment(
                readFileDescriptor: descriptors[1],
                writeFileDescriptor: descriptors[1],
                closeFileDescriptors: true
            )
        )
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        guard !cleanupRequested else { return .accepted("terminated") }
        cleanupRequested = true
        if exitStatus != nil {
            return .accepted("already_exited")
        }
        let diagnostic = helperClient.terminatePTY(sessionID: session.sessionID)
        guard diagnostic.code == nil else {
            return .rejected(
                diagnostic.code?.rawValue ?? "managed_user_helper_pty_terminate_rejected",
                message: diagnostic.sanitizedMessage
            )
        }
        refreshExitObservation()
        if exitStatus == nil {
            inputClosed = true
            phase = .exited
            exitStatus = .unknown
        }
        return .accepted("terminated")
    }

    private func refreshExitObservation() {
        guard exitStatus == nil else { return }
        guard let observation = helperClient.observeManagedUserPTYExit(sessionID: session.sessionID) else {
            return
        }
        if let code = observation.exitCode {
            exitStatus = .exitCode(code)
        } else if let signal = observation.terminatingSignal {
            exitStatus = .signal(signal)
        } else if observation.final {
            exitStatus = .unknown
        }
        if exitStatus != nil {
            inputClosed = true
            phase = .exited
            outputPump?.invalidate()
            invalidateRendererProxy()
        }
    }

    @discardableResult
    fileprivate func drainAvailableOutput(maxBytes: Int = 4096) -> Data {
        guard exitStatus == nil, let outputPump else {
            return Data()
        }
        return applyOutputUpdates(
            outputPump.pollSynchronously(maxBytes: maxBytes)
        )
    }

    @discardableResult
    fileprivate func applyOutputUpdates(
        _ updates: [AlanHelperManagedUserPtyPendingOutputUpdate]
    ) -> Data {
        var collected = Data()
        for update in updates {
            switch update {
            case .output(let output, let routedToRenderer):
                applyHelperOutput(output)
                if !routedToRenderer {
                    appendPendingRendererReplay(output.rendererOutput)
                }
                collected.append(output.rendererOutput)
            case .failure(let diagnostic):
                applyHelperOutputFailure(diagnostic)
            }
        }
        return collected
    }

    private func appendPendingRendererReplay(_ data: Data) {
        pendingRendererReplay.append(data)
    }

    fileprivate func applyPendingOutput(
        from pendingOutputPump: AlanHelperManagedUserPtyOutputPump? = nil
    ) {
        if let pendingOutputPump, outputPump !== pendingOutputPump { return }
        guard let outputPump else { return }
        applyOutputUpdates(outputPump.takePendingUpdates())
    }

    @MainActor
    fileprivate func rendererProxyDidInvalidate(
        _ invalidatedProxy: AlanHelperManagedUserPtyRendererProxy
    ) {
        guard rendererProxy === invalidatedProxy else { return }
        appendPendingRendererReplay(
            invalidatedProxy.takeInvalidationHandoffOutput()
        )
        rendererProxy = nil
        outputPump?.setRendererProxy(nil)
    }

    @MainActor
    private func applyHelperOutput(_ output: AlanHelperManagedUserPtyProcessedOutput) {
        applyHelperOutputChunk(output.chunk, rendererOutput: output.rendererOutput)
        if let transition = output.semanticShellStateTransition {
            recordSemanticShellState(transition)
        }
        switch output.chunk.foregroundProcessGroupState {
        case .shell:
            recordProcessGroupShellActivityState(.shellInput)
        case .foreground:
            recordProcessGroupShellActivityState(.foregroundCommand)
        case .unavailable:
            break
        }
        if let responseFailure = output.responseFailure {
            applyHelperOutputFailure(responseFailure)
        }
    }

    @MainActor
    fileprivate func applyHelperOutputChunk(
        _ chunk: AlanManagedUserPTYOutputChunk,
        rendererOutput: Data? = nil
    ) {
        if chunk.final {
            observedFinalOutputChunk = true
            inputClosed = true
            phase = .exited
            invalidateRendererProxy()
        }
        guard !chunk.data.isEmpty else { return }
        let text = String(decoding: rendererOutput ?? chunk.data, as: UTF8.self)
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    @MainActor
    fileprivate func applyHelperOutputFailure(_ diagnostic: AlanPrivilegedHelperDiagnostic) {
        guard exitStatus == nil else { return }
        inputClosed = true
        phase = .failed
        exitStatus = .unknown
        outputPump?.invalidate()
        invalidateRendererProxy()
        transcriptRingBufferLines.append(diagnostic.sanitizedMessage)
    }

    @MainActor
    fileprivate func recordHelperAcceptedInput(byteCount: Int) {
        acceptedInputBytes += byteCount
    }

    private func recordSemanticShellState(
        _ state: AlanTerminalPtySemanticShellState
    ) {
        semanticShellState = state
        reconcileShellActivityState()
    }

    private func recordProcessGroupShellActivityState(
        _ state: AlanTerminalPtyShellActivityState
    ) {
        processGroupShellActivityState = state
        reconcileShellActivityState()
    }

    private func reconcileShellActivityState() {
        recordShellActivityState(
            AlanTerminalPtyShellActivityResolver.resolve(
                launchesInteractiveShell: bootRequest.strategy.launchesInteractiveShell,
                semanticState: semanticShellState,
                processGroupState: processGroupShellActivityState
            )
        )
    }

    private func recordShellActivityState(_ state: AlanTerminalPtyShellActivityState) {
        guard state != shellActivityState else { return }
        shellActivityState = state
        onShellActivityStateChange?(state)
    }

    private func invalidateRendererProxy() {
        guard let rendererProxy else {
            outputPump?.setRendererProxy(nil)
            return
        }
        rendererProxy.invalidate()
        rendererProxyDidInvalidate(rendererProxy)
    }

}

fileprivate struct AlanHelperManagedUserPtyProcessedOutput {
    let chunk: AlanManagedUserPTYOutputChunk
    let rendererOutput: Data
    let semanticShellStateTransition: AlanTerminalPtySemanticShellState?
    let responseFailure: AlanPrivilegedHelperDiagnostic?
}

fileprivate enum AlanHelperManagedUserPtyPendingOutputUpdate {
    case output(
        AlanHelperManagedUserPtyProcessedOutput,
        routedToRenderer: Bool
    )
    case failure(AlanPrivilegedHelperDiagnostic)
}

fileprivate struct AlanHelperManagedUserPtyRendererPreparation {
    let attached: Bool
    let updates: [AlanHelperManagedUserPtyPendingOutputUpdate]
}

private func readHelperManagedUserPtyProcessedOutput(
    helperClient: AlanPrivilegedHelperClienting,
    sessionID: String,
    maxBytes: Int,
    outputProcessor: AlanTerminalPtyControlSequenceProcessor
) -> Result<AlanHelperManagedUserPtyProcessedOutput, AlanPrivilegedHelperDiagnostic> {
    helperClient.readManagedUserPTY(
        AlanManagedUserPTYReadRequest(
            sessionID: sessionID,
            maxBytes: maxBytes
        )
    ).map { chunk in
        let response = outputProcessor.process(chunk.data)
        let responseFailure: AlanPrivilegedHelperDiagnostic?
        if response.didRespond {
            let result = helperClient.writeManagedUserPTY(
                AlanManagedUserPTYInputRequest(
                    sessionID: sessionID,
                    data: response.ptyResponse
                )
            )
            responseFailure = result.accepted ? nil : result.diagnostic
        } else {
            responseFailure = nil
        }
        return AlanHelperManagedUserPtyProcessedOutput(
            chunk: chunk,
            rendererOutput: response.rendererOutput,
            semanticShellStateTransition: response.semanticShellStateTransition,
            responseFailure: responseFailure
        )
    }
}

fileprivate final class AlanHelperManagedUserPtyOutputPump {
    private weak var ptyHandle: AlanHelperManagedUserPtyHandle?
    private let helperClient: AlanPrivilegedHelperClienting
    private let sessionID: String
    private let ioQueue: DispatchQueue
    private let outputProcessor: AlanTerminalPtyControlSequenceProcessor
    private var timer: DispatchSourceTimer?
    private var pendingUpdates: [AlanHelperManagedUserPtyPendingOutputUpdate] = []
    private var publishScheduled = false
    private var lastForegroundProcessGroupState:
        AlanManagedUserPTYForegroundProcessGroupState = .unavailable
    private weak var rendererProxy: AlanHelperManagedUserPtyRendererProxy?

    init(
        ptyHandle: AlanHelperManagedUserPtyHandle,
        helperClient: AlanPrivilegedHelperClienting,
        sessionID: String,
        ioQueue: DispatchQueue,
        outputProcessor: AlanTerminalPtyControlSequenceProcessor
    ) {
        self.ptyHandle = ptyHandle
        self.helperClient = helperClient
        self.sessionID = sessionID
        self.ioQueue = ioQueue
        self.outputProcessor = outputProcessor
    }

    deinit {
        invalidate()
    }

    func start() {
        ioQueue.sync {
            guard timer == nil else { return }
            let timer = DispatchSource.makeTimerSource(queue: ioQueue)
            timer.schedule(deadline: .now(), repeating: .milliseconds(30))
            timer.setEventHandler { [weak self] in
                self?.pollAndPublishOnQueue(maxBytes: 4096)
            }
            timer.resume()
            self.timer = timer
        }
    }

    func pollSynchronously(
        maxBytes: Int
    ) -> [AlanHelperManagedUserPtyPendingOutputUpdate] {
        ioQueue.sync {
            pollOnQueue(maxBytes: maxBytes)
            return takePendingUpdatesOnQueue()
        }
    }

    func takePendingUpdates() -> [AlanHelperManagedUserPtyPendingOutputUpdate] {
        ioQueue.sync {
            takePendingUpdatesOnQueue()
        }
    }

    func setRendererProxy(_ rendererProxy: AlanHelperManagedUserPtyRendererProxy?) {
        ioQueue.sync {
            self.rendererProxy = rendererProxy
        }
    }

    func attachRendererProxy(
        _ rendererProxy: AlanHelperManagedUserPtyRendererProxy,
        replayChunks: [Data],
        maxBytes: Int
    ) -> AlanHelperManagedUserPtyRendererPreparation {
        ioQueue.sync {
            pollOnQueue(maxBytes: maxBytes)
            let updates = takePendingUpdatesOnQueue()
            rendererProxy.startOnOutputPumpQueue()

            for chunk in replayChunks {
                let route = rendererProxy.routeRendererOutputFromOutputPump(chunk)
                guard route.accepted, route.healthy else {
                    rendererProxy.invalidate()
                    return AlanHelperManagedUserPtyRendererPreparation(
                        attached: false,
                        updates: updates
                    )
                }
            }
            var preparedUpdates: [AlanHelperManagedUserPtyPendingOutputUpdate] = []
            for update in updates {
                guard case .output(let output, _) = update,
                      !output.rendererOutput.isEmpty
                else {
                    preparedUpdates.append(update)
                    continue
                }
                let route = rendererProxy.routeRendererOutputFromOutputPump(
                    output.rendererOutput
                )
                guard route.accepted, route.healthy else {
                    rendererProxy.invalidate()
                    return AlanHelperManagedUserPtyRendererPreparation(
                        attached: false,
                        updates: updates
                    )
                }
                preparedUpdates.append(
                    .output(output, routedToRenderer: true)
                )
            }
            self.rendererProxy = rendererProxy
            return AlanHelperManagedUserPtyRendererPreparation(
                attached: true,
                updates: preparedUpdates
            )
        }
    }

    func invalidate() {
        ioQueue.sync {
            timer?.cancel()
            timer = nil
            pendingUpdates.removeAll()
            publishScheduled = false
        }
    }

    private func pollAndPublishOnQueue(maxBytes: Int) {
        pollOnQueue(maxBytes: maxBytes)
        guard !pendingUpdates.isEmpty, !publishScheduled else { return }
        publishScheduled = true
        DispatchQueue.main.async { [weak self, weak ptyHandle] in
            guard let self else { return }
            ptyHandle?.applyPendingOutput(from: self)
        }
    }

    private func pollOnQueue(maxBytes: Int) {
        var remainingBytes = 64 * 1024
        while remainingBytes > 0 {
            switch readHelperManagedUserPtyProcessedOutput(
                helperClient: helperClient,
                sessionID: sessionID,
                maxBytes: min(max(1, maxBytes), remainingBytes),
                outputProcessor: outputProcessor
            ) {
            case .success(let output):
                remainingBytes -= min(remainingBytes, output.chunk.data.count)
                var routedToRenderer = false
                if !output.rendererOutput.isEmpty,
                   let rendererProxy
                {
                    let route = rendererProxy.routeRendererOutputFromOutputPump(
                        output.rendererOutput
                    )
                    routedToRenderer = route.accepted
                    if !route.healthy {
                        rendererProxy.invalidate()
                        self.rendererProxy = nil
                    }
                }
                let processGroupChanged =
                    output.chunk.foregroundProcessGroupState != .unavailable
                    && output.chunk.foregroundProcessGroupState
                        != lastForegroundProcessGroupState
                if output.chunk.foregroundProcessGroupState != .unavailable {
                    lastForegroundProcessGroupState =
                        output.chunk.foregroundProcessGroupState
                }
                if !output.chunk.data.isEmpty
                    || output.chunk.final
                    || output.responseFailure != nil
                    || processGroupChanged
                {
                    pendingUpdates.append(
                        .output(
                            output,
                            routedToRenderer: routedToRenderer
                        )
                    )
                }
                guard !output.chunk.data.isEmpty,
                      output.responseFailure == nil,
                      !output.chunk.final
                else {
                    if output.chunk.final || output.responseFailure != nil {
                        stopOnQueue()
                    }
                    return
                }
            case .failure(let diagnostic):
                pendingUpdates.append(.failure(diagnostic))
                stopOnQueue()
                return
            }
        }
    }

    private func takePendingUpdatesOnQueue() -> [AlanHelperManagedUserPtyPendingOutputUpdate] {
        let updates = pendingUpdates
        pendingUpdates.removeAll()
        publishScheduled = false
        return updates
    }

    private func stopOnQueue() {
        timer?.cancel()
        timer = nil
    }
}

fileprivate final class AlanHelperManagedUserPtyRendererProxy {
    fileprivate struct OutputRoute {
        let accepted: Bool
        let healthy: Bool
    }

    private weak var ptyHandle: AlanHelperManagedUserPtyHandle?
    private let helperClient: AlanPrivilegedHelperClienting
    private let sessionID: String
    private let hostFileDescriptor: Int32
    private let ioQueue: DispatchQueue
    private let ioQueueKey = DispatchSpecificKey<UInt8>()
    private let invalidationLock = NSLock()
    private var rendererInputSource: DispatchSourceRead?
    private var rendererOutputWriteSource: DispatchSourceWrite?
    private var isInvalidated = false
    private var pendingRendererOutput = Data()
    private var invalidationHandoffOutput = Data()

    init(
        ptyHandle: AlanHelperManagedUserPtyHandle,
        helperClient: AlanPrivilegedHelperClienting,
        sessionID: String,
        hostFileDescriptor: Int32,
        ioQueue: DispatchQueue
    ) {
        self.ptyHandle = ptyHandle
        self.helperClient = helperClient
        self.sessionID = sessionID
        self.hostFileDescriptor = hostFileDescriptor
        self.ioQueue = ioQueue
        ioQueue.setSpecific(key: ioQueueKey, value: 1)
    }

    deinit {
        invalidate()
    }

    fileprivate func startOnOutputPumpQueue() {
        dispatchPrecondition(condition: .onQueue(ioQueue))
        guard !invalidated else { return }
        let inputSource = DispatchSource.makeReadSource(
            fileDescriptor: hostFileDescriptor,
            queue: ioQueue
        )
        inputSource.setEventHandler { [weak self] in
            self?.drainRendererInput()
        }
        inputSource.setCancelHandler { [hostFileDescriptor] in
            close(hostFileDescriptor)
        }
        rendererInputSource = inputSource
        inputSource.resume()
    }

    func invalidate() {
        guard markInvalidated() else { return }
        performOnIOQueue {
            rendererInputSource?.cancel()
            rendererInputSource = nil
            rendererOutputWriteSource?.cancel()
            rendererOutputWriteSource = nil
            invalidationHandoffOutput.append(pendingRendererOutput)
            pendingRendererOutput.removeAll()
        }
        Task { @MainActor [weak self, weak ptyHandle] in
            guard let self else { return }
            ptyHandle?.rendererProxyDidInvalidate(self)
        }
    }

    fileprivate var invalidated: Bool {
        invalidationLock.lock()
        defer { invalidationLock.unlock() }
        return isInvalidated
    }

    private func markInvalidated() -> Bool {
        invalidationLock.lock()
        defer { invalidationLock.unlock() }
        guard !isInvalidated else { return false }
        isInvalidated = true
        return true
    }

    private func performOnIOQueue(_ operation: () -> Void) {
        if DispatchQueue.getSpecific(key: ioQueueKey) != nil {
            operation()
        } else {
            ioQueue.sync(execute: operation)
        }
    }

    fileprivate func takeInvalidationHandoffOutput() -> Data {
        var output = Data()
        performOnIOQueue {
            output = invalidationHandoffOutput
            invalidationHandoffOutput.removeAll()
        }
        return output
    }

    private func drainRendererInput() {
        guard !invalidated else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = Darwin.read(hostFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                let data = Data(buffer.prefix(count))
                guard writeHelperInput(data) else {
                    invalidate()
                    return
                }
                continue
            }
            if count == 0 {
                invalidate()
                return
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return
            }
            invalidate()
            return
        }
    }

    private func writeHelperInput(_ data: Data) -> Bool {
        guard !data.isEmpty, !invalidated else { return false }
        let result = helperClient.writeManagedUserPTY(
            AlanManagedUserPTYInputRequest(sessionID: sessionID, data: data)
        )
        guard result.accepted else { return false }
        Task { @MainActor [weak self] in
            self?.ptyHandle?.recordHelperAcceptedInput(byteCount: data.count)
        }
        return true
    }

    fileprivate func routeRendererOutputFromOutputPump(_ data: Data) -> OutputRoute {
        guard !data.isEmpty else {
            return OutputRoute(accepted: true, healthy: true)
        }
        guard !invalidated else {
            return OutputRoute(accepted: false, healthy: false)
        }
        pendingRendererOutput.append(data)
        return OutputRoute(
            accepted: true,
            healthy: drainPendingRendererOutput()
        )
    }

    private func drainPendingRendererOutput() -> Bool {
        guard !invalidated else { return false }

        while !pendingRendererOutput.isEmpty {
            let result = pendingRendererOutput.withUnsafeBytes { rawBuffer -> Int in
                guard let baseAddress = rawBuffer.baseAddress else { return -1 }
                return Darwin.write(hostFileDescriptor, baseAddress, rawBuffer.count)
            }

            if result > 0 {
                pendingRendererOutput.removeFirst(result)
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                ensureRendererOutputWriteSource()
                break
            }
            return false
        }

        if pendingRendererOutput.isEmpty {
            cancelRendererOutputWriteSource()
        }
        return true
    }

    private func ensureRendererOutputWriteSource() {
        guard !invalidated, rendererOutputWriteSource == nil else { return }
        let writeSource = DispatchSource.makeWriteSource(
            fileDescriptor: hostFileDescriptor,
            queue: ioQueue
        )
        writeSource.setEventHandler { [weak self] in
            guard let self else { return }
            guard self.drainPendingRendererOutput() else {
                self.invalidate()
                return
            }
        }
        writeSource.resume()
        rendererOutputWriteSource = writeSource
    }

    private func cancelRendererOutputWriteSource() {
        rendererOutputWriteSource?.cancel()
        rendererOutputWriteSource = nil
    }
}

private func helperPTYRejectionCode(
    _ result: AlanManagedUserPTYControlResult,
    fallback: String
) -> String {
    result.diagnostic.code?.rawValue ?? fallback
}

private func setNonBlockingFileDescriptor(_ fileDescriptor: Int32) {
    let flags = fcntl(fileDescriptor, F_GETFL)
    guard flags >= 0 else { return }
    _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
}

private extension AlanManagedUserPTYSignal {
    init(_ signal: AlanTerminalPtySignal) {
        switch signal {
        case .interrupt:
            self = .interrupt
        case .terminate:
            self = .terminate
        case .kill:
            self = .kill
        }
    }
}

#endif
