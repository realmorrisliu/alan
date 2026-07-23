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
    }

    deinit {
        rendererProxy?.invalidate()
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        applyPendingProxyOutput()
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

        rendererProxy?.invalidate()
        let proxy = AlanHelperManagedUserPtyRendererProxy(
            ptyHandle: self,
            helperClient: helperClient,
            sessionID: session.sessionID,
            hostFileDescriptor: descriptors[0],
            ioQueue: helperQueue
        )
        rendererProxy = proxy
        proxy.start()

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
            invalidateRendererProxy()
        }
    }

    @discardableResult
    fileprivate func drainAvailableOutput(maxBytes: Int = 4096) -> Data {
        guard exitStatus == nil else { return Data() }
        var collected = Data()

        while true {
            switch helperQueue.sync(execute: {
                helperClient.readManagedUserPTY(
                    AlanManagedUserPTYReadRequest(
                        sessionID: session.sessionID,
                        maxBytes: maxBytes
                    )
                )
            }) {
            case .success(let chunk):
                applyHelperOutputChunk(chunk)
                guard !chunk.data.isEmpty else { return collected }
                collected.append(chunk.data)
            case .failure(let diagnostic):
                applyHelperOutputFailure(diagnostic)
                return collected
            }
        }
    }

    @MainActor
    fileprivate func applyPendingProxyOutput() {
        guard let rendererProxy else { return }
        let updates = rendererProxy.drainPendingOutputUpdates()
        updates.chunks.forEach(applyHelperOutputChunk)
        updates.failures.forEach(applyHelperOutputFailure)
        updates.shellActivityStates.forEach(recordShellActivityState)
    }

    @MainActor
    fileprivate func applyHelperOutputChunk(_ chunk: AlanManagedUserPTYOutputChunk) {
        if chunk.final {
            observedFinalOutputChunk = true
            inputClosed = true
            phase = .exited
            invalidateRendererProxy()
        }
        guard !chunk.data.isEmpty else { return }
        let text = String(decoding: chunk.data, as: UTF8.self)
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
        invalidateRendererProxy()
        transcriptRingBufferLines.append(diagnostic.sanitizedMessage)
    }

    @MainActor
    fileprivate func recordHelperAcceptedInput(byteCount: Int) {
        acceptedInputBytes += byteCount
    }

    fileprivate func recordShellActivityState(_ state: AlanTerminalPtyShellActivityState) {
        guard state != shellActivityState else { return }
        shellActivityState = state
        onShellActivityStateChange?(state)
    }

    private func invalidateRendererProxy() {
        rendererProxy?.invalidate()
        rendererProxy = nil
    }

}

private final class AlanHelperManagedUserPtyRendererProxy {
    private weak var ptyHandle: AlanHelperManagedUserPtyHandle?
    private let helperClient: AlanPrivilegedHelperClienting
    private let sessionID: String
    private let hostFileDescriptor: Int32
    private let ioQueue: DispatchQueue
    private let invalidationLock = NSLock()
    private let pendingOutputLock = NSLock()
    private var rendererInputSource: DispatchSourceRead?
    private var helperOutputTimer: DispatchSourceTimer?
    private var rendererOutputWriteSource: DispatchSourceWrite?
    private var controlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
    private var isInvalidated = false
    private var pendingOutputChunks: [AlanManagedUserPTYOutputChunk] = []
    private var pendingOutputFailures: [AlanPrivilegedHelperDiagnostic] = []
    private var pendingShellActivityStates: [AlanTerminalPtyShellActivityState] = []
    private var pendingRendererOutput = Data()

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
    }

    deinit {
        invalidate()
    }

    func start() {
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
        inputSource.resume()
        rendererInputSource = inputSource

        let timer = DispatchSource.makeTimerSource(queue: ioQueue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(30))
        timer.setEventHandler { [weak self] in
            self?.pollHelperOutput()
        }
        timer.resume()
        helperOutputTimer = timer
    }

    func invalidate() {
        guard markInvalidated() else { return }
        rendererInputSource?.cancel()
        rendererInputSource = nil
        helperOutputTimer?.cancel()
        helperOutputTimer = nil
        rendererOutputWriteSource?.cancel()
        rendererOutputWriteSource = nil
        pendingRendererOutput.removeAll()
    }

    private var invalidated: Bool {
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

    fileprivate func drainPendingOutputUpdates() -> (
        chunks: [AlanManagedUserPTYOutputChunk],
        failures: [AlanPrivilegedHelperDiagnostic],
        shellActivityStates: [AlanTerminalPtyShellActivityState]
    ) {
        pendingOutputLock.lock()
        defer { pendingOutputLock.unlock() }
        let chunks = pendingOutputChunks
        let failures = pendingOutputFailures
        let shellActivityStates = pendingShellActivityStates
        pendingOutputChunks.removeAll()
        pendingOutputFailures.removeAll()
        pendingShellActivityStates.removeAll()
        return (chunks, failures, shellActivityStates)
    }

    private func enqueueOutputChunk(_ chunk: AlanManagedUserPTYOutputChunk) {
        pendingOutputLock.lock()
        pendingOutputChunks.append(chunk)
        pendingOutputLock.unlock()
        Task { @MainActor [weak self] in
            self?.ptyHandle?.applyPendingProxyOutput()
        }
    }

    private func enqueueOutputFailure(_ diagnostic: AlanPrivilegedHelperDiagnostic) {
        pendingOutputLock.lock()
        pendingOutputFailures.append(diagnostic)
        pendingOutputLock.unlock()
        Task { @MainActor [weak self] in
            self?.ptyHandle?.applyPendingProxyOutput()
        }
    }

    private func enqueueShellActivityState(_ state: AlanTerminalPtyShellActivityState) {
        pendingOutputLock.lock()
        pendingShellActivityStates.append(state)
        pendingOutputLock.unlock()
        Task { @MainActor [weak self] in
            self?.ptyHandle?.applyPendingProxyOutput()
        }
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

    private func pollHelperOutput() {
        guard !invalidated else { return }
        while true {
            let readResult = helperClient.readManagedUserPTY(
                AlanManagedUserPTYReadRequest(
                    sessionID: sessionID,
                    maxBytes: 4096
                )
            )
            let output: Data
            let isFinal: Bool
            switch readResult {
            case .success(let chunk):
                output = chunk.data
                isFinal = chunk.final
                enqueueOutputChunk(chunk)
            case .failure(let diagnostic):
                enqueueOutputFailure(diagnostic)
                invalidate()
                return
            }
            guard !output.isEmpty else {
                if isFinal {
                    invalidate()
                }
                return
            }

            let response = controlSequenceResponder.process(output)
            if let transition = response.shellActivityTransition {
                enqueueShellActivityState(transition)
            }
            if response.didRespond, !writeHelperInput(response.ptyResponse) {
                invalidate()
                return
            }

            if !response.rendererOutput.isEmpty, !forwardRendererOutput(response.rendererOutput) {
                invalidate()
                return
            }
            if isFinal {
                invalidate()
                return
            }
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

    private func forwardRendererOutput(_ data: Data) -> Bool {
        guard !data.isEmpty, !invalidated else { return false }
        pendingRendererOutput.append(data)
        return drainPendingRendererOutput()
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
