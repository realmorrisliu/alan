#if os(macOS)
import Darwin
import Foundation

@_silgen_name("alan_darwin_pty_spawn")
private func alanDarwinPtySpawn(
    _ executablePath: UnsafePointer<CChar>,
    _ argv: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ envp: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ workingDirectory: UnsafePointer<CChar>,
    _ rows: UInt16,
    _ columns: UInt16,
    _ masterFileDescriptor: UnsafeMutablePointer<Int32>,
    _ processID: UnsafeMutablePointer<pid_t>
) -> Int32

@MainActor
final class AlanDarwinTerminalPtyHandle: AlanTerminalPtyHandle {
    private struct PendingDirectPtyInputChunk {
        var data: Data
        var countedBytesRemaining: Int
    }

    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    private(set) var processID: pid_t?
    private(set) var processGroupID: pid_t?
    private(set) var launchError: String?
    private(set) var phase: AlanTerminalPtyRuntimePhase = .pending
    private(set) var inputClosed = false
    private(set) var exitStatus: AlanTerminalProcessExitStatus?
    private(set) var resizeRequests: [AlanTerminalPtyDimensions] = []
    private(set) var signalRequests: [AlanTerminalPtySignal] = []
    fileprivate var masterFileDescriptor: Int32 = -1
    private var transcriptRingBufferLines: [String] = []
    private var acceptedInputBytes = 0
    private let outputProcessor = AlanTerminalPtyControlSequenceProcessor()
    private var rendererProxy: AlanDarwinTerminalPtyRendererProxy?
    private var pendingDirectPtyInputChunks: [PendingDirectPtyInputChunk] = []
    private var directPtyInputWriteSource: DispatchSourceWrite?
    private var rendererlessOutputSource: DispatchSourceRead?
    private var processGroupTimer: DispatchSourceTimer?
    private var semanticShellState: AlanTerminalPtySemanticShellState?
    private var processGroupShellActivityState: AlanTerminalPtyShellActivityState?
    private var idleProcessGroupTracker: AlanTerminalPtyIdleProcessGroupTracker?
    private var pendingRendererReplay = AlanTerminalPtyBoundedReplayBuffer(
        maxBytes: 1024 * 1024
    )
    private(set) var shellActivityState: AlanTerminalPtyShellActivityState = .unknown
    var onShellActivityStateChange: ((AlanTerminalPtyShellActivityState) -> Void)?

    init(contentID: String, bootRequest: AlanTerminalBootRequest) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        if !bootRequest.strategy.launchesInteractiveShell {
            shellActivityState = .foregroundCommand
        }
        launch()
    }

    deinit {
        rendererlessOutputSource?.cancel()
        processGroupTimer?.cancel()
        rendererProxy?.invalidate()
        directPtyInputWriteSource?.cancel()
        if masterFileDescriptor >= 0 {
            close(masterFileDescriptor)
        }
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        refreshExitStatus()
        if let rendererProxy, rendererProxy.invalidated {
            rendererProxyDidInvalidate(rendererProxy)
        }
        if rendererProxy == nil {
            _ = drainAvailableOutput()
        }
        return AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: phase,
            dimensions: resizeRequests.last,
            acceptedInputBytes: acceptedInputBytes,
            inputClosed: inputClosed,
            lastSignal: signalRequests.last,
            exitStatus: exitStatus,
            transcriptLines: transcriptRingBufferLines
        )
    }

    var isInputReady: Bool {
        phase == .running && !inputClosed && exitStatus == nil && masterFileDescriptor >= 0
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        refreshExitStatus()
        guard exitStatus == nil else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: phase.rawValue
            )
        }
        guard !inputClosed, masterFileDescriptor >= 0 else {
            return .rejected(
                errorCode: "terminal_pty_input_closed",
                errorMessage: "The terminal PTY input stream is closed.",
                runtimePhase: phase.rawValue
            )
        }

        let data = Data(text.utf8)
        guard enqueueDirectPtyInput(data, countedBytes: data.count) else {
            return .rejected(
                errorCode: "terminal_pty_write_failed",
                errorMessage: String(cString: strerror(errno)),
                runtimePhase: phase.rawValue
            )
        }

        return .accepted(byteCount: data.count, runtimePhase: phase.rawValue)
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        guard masterFileDescriptor >= 0 else {
            return .rejected(
                "terminal_pty_closed",
                message: "The terminal PTY file descriptor is closed."
            )
        }

        let dimensions = AlanTerminalPtyDimensions(
            columns: max(0, columns),
            rows: max(0, rows)
        )
        var size = winsize(
            ws_row: UInt16(clamping: dimensions.rows),
            ws_col: UInt16(clamping: dimensions.columns),
            ws_xpixel: 0,
            ws_ypixel: 0
        )
        guard ioctl(masterFileDescriptor, TIOCSWINSZ, &size) == 0 else {
            return .rejected(
                "terminal_pty_resize_failed",
                message: String(cString: strerror(errno))
            )
        }
        resizeRequests.append(dimensions)
        return .accepted("resized")
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        guard masterFileDescriptor >= 0 else {
            return .rejected(
                "terminal_pty_closed",
                message: "The terminal PTY file descriptor is closed."
            )
        }
        let eof = Data([UInt8(4)])
        guard enqueueDirectPtyInput(eof, countedBytes: 0) else {
            return .rejected(
                "terminal_pty_eof_failed",
                message: String(cString: strerror(errno))
            )
        }
        inputClosed = true
        phase = .inputClosed
        return .accepted("input_closed")
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        refreshExitStatus()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        guard let processID else {
            return .rejected(
                "terminal_child_missing",
                message: "No terminal child process is available."
            )
        }

        let rawSignal: Int32
        switch signal {
        case .interrupt:
            rawSignal = SIGINT
        case .terminate:
            rawSignal = SIGTERM
        case .kill:
            rawSignal = SIGKILL
        }

        let foregroundProcessGroupID = currentForegroundProcessGroupID()
        let targetProcessGroupID = foregroundProcessGroupID ?? processGroupID
        let target = targetProcessGroupID.map { -$0 } ?? processID
        var result = Darwin.kill(target, rawSignal)
        if result != 0,
           let processGroupID,
           processGroupID != targetProcessGroupID
        {
            result = Darwin.kill(-processGroupID, rawSignal)
        }
        if result != 0 {
            result = Darwin.kill(processID, rawSignal)
        }
        guard result == 0 else {
            return .rejected(
                "terminal_signal_failed",
                message: String(cString: strerror(errno))
            )
        }
        signalRequests.append(signal)
        return .accepted(signal.rawValue)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        if let rendererProxy {
            rendererProxy.invalidate()
            rendererProxyDidInvalidate(rendererProxy)
        }
        refreshExitStatus()
        guard exitStatus == nil else {
            return .rejected(
                .rejected(
                    "terminal_child_exited",
                    message: "The terminal process has exited."
                )
            )
        }
        guard masterFileDescriptor >= 0 else {
            return .rejected(
                .rejected(
                    "terminal_pty_closed",
                    message: "The terminal PTY file descriptor is closed."
                )
            )
        }

        var descriptors = [Int32](repeating: -1, count: 2)
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            return .rejected(
                .rejected(
                    "terminal_renderer_socketpair_failed",
                    message: String(cString: strerror(errno))
                )
            )
        }

        setNonBlocking(descriptors[0])
        setNonBlocking(descriptors[1])
        setNoSigpipeSocketOption(descriptors[0])
        setNoSigpipeSocketOption(descriptors[1])

        stopRendererlessOutputDraining()
        _ = drainAvailableOutput()
        let proxy = AlanDarwinTerminalPtyRendererProxy(
            ptyHandle: self,
            hostFileDescriptor: descriptors[0],
            ptyFileDescriptor: masterFileDescriptor,
            outputProcessor: outputProcessor
        )
        rendererProxy = proxy
        let replayChunks = pendingRendererReplay.takeChunks()
        proxy.start(initialRendererOutput: replayChunks)

        return .attached(
            AlanTerminalPtyRendererAttachment(
                readFileDescriptor: descriptors[1],
                writeFileDescriptor: descriptors[1],
                closeFileDescriptors: true
            )
        )
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        refreshExitStatus()
        guard exitStatus == nil else { return .accepted("already_exited") }
        return sendSignal(.terminate)
    }

    @discardableResult
    func drainAvailableOutput() -> [String] {
        guard rendererProxy == nil, masterFileDescriptor >= 0 else { return [] }
        var collected = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)

        while collected.count < 64 * 1024 {
            let count = Darwin.read(masterFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                collected.append(buffer, count: count)
                continue
            }
            if count == 0 {
                _ = refreshExitStatus()
                break
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                break
            }
            break
        }

        guard !collected.isEmpty else { return [] }
        let response = outputProcessor.process(collected)
        if let transition = response.semanticShellStateTransition {
            recordSemanticShellState(transition)
        }
        if response.didRespond {
            _ = writePtyProtocolResponse(response.ptyResponse)
        }
        guard !response.rendererOutput.isEmpty else { return [] }
        appendPendingRendererReplay(response.rendererOutput)
        recordPtyOutput(response.rendererOutput)
        return transcriptLines(
            from: String(decoding: response.rendererOutput, as: UTF8.self)
        )
    }

    private func appendPendingRendererReplay(_ data: Data) {
        pendingRendererReplay.append(data)
    }

    @discardableResult
    fileprivate func writePtyProtocolResponse(_ data: Data) -> Int {
        enqueueDirectPtyInput(data, countedBytes: 0) ? data.count : -1
    }

    private func enqueueDirectPtyInput(_ data: Data, countedBytes: Int) -> Bool {
        guard !data.isEmpty else { return true }
        pendingDirectPtyInputChunks.append(
            PendingDirectPtyInputChunk(
                data: data,
                countedBytesRemaining: max(0, min(countedBytes, data.count))
            )
        )
        return drainPendingDirectPtyInput()
    }

    private func drainPendingDirectPtyInput() -> Bool {
        guard masterFileDescriptor >= 0 else { return false }

        while !pendingDirectPtyInputChunks.isEmpty {
            var chunk = pendingDirectPtyInputChunks.removeFirst()
            let result = chunk.data.withUnsafeBytes { rawBuffer -> Int in
                guard let baseAddress = rawBuffer.baseAddress else { return -1 }
                return Darwin.write(masterFileDescriptor, baseAddress, rawBuffer.count)
            }

            if result > 0 {
                let countedBytes = min(chunk.countedBytesRemaining, result)
                acceptedInputBytes += countedBytes
                if result < chunk.data.count {
                    chunk.data.removeFirst(result)
                    chunk.countedBytesRemaining -= countedBytes
                    pendingDirectPtyInputChunks.insert(chunk, at: 0)
                    ensureDirectPtyInputWriteSource()
                    break
                }
                continue
            }

            if errno == EAGAIN || errno == EWOULDBLOCK {
                pendingDirectPtyInputChunks.insert(chunk, at: 0)
                ensureDirectPtyInputWriteSource()
                break
            }
            return false
        }

        if pendingDirectPtyInputChunks.isEmpty {
            cancelDirectPtyInputWriteSource()
        }
        return true
    }

    private func ensureDirectPtyInputWriteSource() {
        guard masterFileDescriptor >= 0, directPtyInputWriteSource == nil else { return }
        let writeSource = DispatchSource.makeWriteSource(
            fileDescriptor: masterFileDescriptor,
            queue: .main
        )
        writeSource.setEventHandler { [weak self] in
            Task { @MainActor [weak self] in
                guard let self else { return }
                guard self.drainPendingDirectPtyInput() else {
                    self.failPendingDirectPtyInput()
                    return
                }
            }
        }
        writeSource.resume()
        directPtyInputWriteSource = writeSource
    }

    private func cancelDirectPtyInputWriteSource() {
        directPtyInputWriteSource?.cancel()
        directPtyInputWriteSource = nil
    }

    private func failPendingDirectPtyInput() {
        pendingDirectPtyInputChunks.removeAll()
        cancelDirectPtyInputWriteSource()
        inputClosed = true
        phase = .failed
        launchError = String(cString: strerror(errno))
    }

    fileprivate func recordRendererInputBytes(_ byteCount: Int) {
        guard byteCount > 0 else { return }
        acceptedInputBytes += byteCount
    }

    fileprivate func recordPtyOutput(_ data: Data) {
        guard !data.isEmpty else { return }
        let text = String(decoding: data, as: UTF8.self)
        let lines = transcriptLines(from: text)
        transcriptRingBufferLines.append(contentsOf: lines)
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    fileprivate func recordSemanticShellState(
        _ state: AlanTerminalPtySemanticShellState
    ) {
        semanticShellState = state
        reconcileShellActivityState()
    }

    fileprivate func recordProcessGroupShellActivityState(
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

    private func startRendererlessOutputDraining() {
        guard rendererlessOutputSource == nil, masterFileDescriptor >= 0 else { return }
        let source = DispatchSource.makeReadSource(
            fileDescriptor: masterFileDescriptor,
            queue: .main
        )
        source.setEventHandler { [weak self] in
            _ = self?.drainAvailableOutput()
        }
        source.resume()
        rendererlessOutputSource = source
    }

    private func stopRendererlessOutputDraining() {
        rendererlessOutputSource?.cancel()
        rendererlessOutputSource = nil
    }

    private func startProcessGroupObservation() {
        guard processGroupTimer == nil, processGroupID != nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: .milliseconds(30))
        timer.setEventHandler { [weak self] in
            self?.observeForegroundProcessGroup()
        }
        timer.resume()
        processGroupTimer = timer
    }

    private func observeForegroundProcessGroup() {
        guard masterFileDescriptor >= 0, var idleProcessGroupTracker else { return }
        guard let foregroundProcessGroupID = currentForegroundProcessGroupID() else { return }
        let state = idleProcessGroupTracker.observe(
            foregroundProcessGroupID: foregroundProcessGroupID,
            semanticState: semanticShellState
        )
        self.idleProcessGroupTracker = idleProcessGroupTracker
        recordProcessGroupShellActivityState(state)
    }

    private func currentForegroundProcessGroupID() -> pid_t? {
        guard masterFileDescriptor >= 0 else { return nil }
        let foregroundProcessGroupID = tcgetpgrp(masterFileDescriptor)
        return foregroundProcessGroupID > 0 ? foregroundProcessGroupID : nil
    }

    private func recordShellActivityState(_ state: AlanTerminalPtyShellActivityState) {
        guard state != shellActivityState else { return }
        shellActivityState = state
        onShellActivityStateChange?(state)
    }

    fileprivate func rendererProxyDidInvalidate(
        _ invalidatedProxy: AlanDarwinTerminalPtyRendererProxy
    ) {
        guard rendererProxy === invalidatedProxy else { return }
        appendPendingRendererReplay(
            invalidatedProxy.takeInvalidationHandoffOutput()
        )
        rendererProxy = nil
        startRendererlessOutputDraining()
    }

    @discardableResult
    func refreshExitStatus() -> AlanTerminalProcessExitStatus? {
        guard exitStatus == nil, let processID else {
            return exitStatus
        }

        var status: Int32 = 0
        let result = waitpid(processID, &status, WNOHANG)
        if result < 0, errno == ECHILD {
            let probe = Darwin.kill(processID, 0)
            if probe != 0, errno == ESRCH {
                exitStatus = .unknown
                phase = .exited
            }
            return exitStatus
        }
        guard result == processID else {
            return exitStatus
        }

        if waitStatusExited(status) {
            exitStatus = .exitCode(waitStatusExitCode(status))
        } else if waitStatusSignaled(status) {
            exitStatus = .signal(waitStatusTermSignal(status))
        } else {
            exitStatus = .exitCode(status)
        }
        phase = .exited
        processGroupTimer?.cancel()
        processGroupTimer = nil
        recordShellActivityState(.shellInput)
        return exitStatus
    }

    private func launch() {
        var master: Int32 = -1
        var spawnedPid: pid_t = 0
        let environment = ProcessInfo.processInfo.environment.merging(bootRequest.environment) {
            _, newValue in newValue
        }
        let shellLaunch = bootRequest.strategy.launchesInteractiveShell
            ? AlanTerminalShellLaunch.integratingGhostty(
                executablePath: bootRequest.executablePath,
                arguments: bootRequest.arguments,
                environment: environment,
                resourcesPath: environment["GHOSTTY_RESOURCES_DIR"]
            )
            : AlanTerminalShellLaunch(
                argumentZero: bootRequest.executablePath,
                arguments: bootRequest.arguments,
                environment: environment
            )
        let arguments = [shellLaunch.argumentZero] + shellLaunch.arguments

        let spawnResult = bootRequest.executablePath.withCString { executablePath in
            bootRequest.workingDirectory.withCString { workingDirectory in
                withCStringArray(arguments) { argv in
                    withCStringArray(shellLaunch.environment.map {
                        "\($0.key)=\($0.value)"
                    }.sorted()) { envp in
                        alanDarwinPtySpawn(
                            executablePath,
                            argv,
                            envp,
                            workingDirectory,
                            24,
                            80,
                            &master,
                            &spawnedPid
                        )
                    }
                }
            }
        }

        guard spawnResult == 0 else {
            if master >= 0 {
                close(master)
            }
            masterFileDescriptor = -1
            launchError = String(cString: strerror(spawnResult))
            phase = .failed
            return
        }

        guard master >= 0 else {
            masterFileDescriptor = -1
            launchError = "forkpty did not return a master PTY file descriptor."
            phase = .failed
            return
        }

        setNonBlocking(master)
        processID = spawnedPid
        processGroupID = spawnedPid
        idleProcessGroupTracker = AlanTerminalPtyIdleProcessGroupTracker(
            initialIdleProcessGroupID: spawnedPid,
            allowsIdleProcessGroupRebase:
                bootRequest.strategy.allowsInteractiveShellProcessGroupRebase
        )
        masterFileDescriptor = master
        preseedFishPrimaryDeviceAttributesResponseIfNeeded()
        phase = .running
        resizeRequests.append(AlanTerminalPtyDimensions(columns: 80, rows: 24))
        startRendererlessOutputDraining()
        if bootRequest.strategy.launchesInteractiveShell {
            startProcessGroupObservation()
        }
    }

    private func preseedFishPrimaryDeviceAttributesResponseIfNeeded() {
        let executableName = URL(fileURLWithPath: bootRequest.executablePath).lastPathComponent
        guard executableName == "fish" else { return }
        let response = AlanTerminalPtyControlSequenceResponder.primaryDeviceAttributesResponseData
        let written = response.withUnsafeBytes { buffer -> Int in
            guard let baseAddress = buffer.baseAddress else { return -1 }
            return Darwin.write(masterFileDescriptor, baseAddress, buffer.count)
        }
        guard written == response.count else { return }
        outputProcessor.suppressNextPrimaryDeviceAttributesResponse()
    }

    private func setNonBlocking(_ fileDescriptor: Int32) {
        let flags = fcntl(fileDescriptor, F_GETFL)
        guard flags >= 0 else { return }
        _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
    }
}

private final class AlanDarwinTerminalPtyRendererProxy {
    private struct PendingPtyInputChunk {
        var data: Data
        var countedBytesRemaining: Int
    }

    private weak var ptyHandle: AlanDarwinTerminalPtyHandle?
    private let hostFileDescriptor: Int32
    private let ptyFileDescriptor: Int32
    private let outputProcessor: AlanTerminalPtyControlSequenceProcessor
    private let ioQueue = DispatchQueue(
        label: "dev.alan.terminal.pty.renderer",
        qos: .userInitiated
    )
    private let ioQueueKey = DispatchSpecificKey<UInt8>()
    private let invalidationLock = NSLock()
    private var rendererInputSource: DispatchSourceRead?
    private var ptyOutputSource: DispatchSourceRead?
    private var ptyInputWriteSource: DispatchSourceWrite?
    private var rendererOutputWriteSource: DispatchSourceWrite?
    private var pendingPtyInputChunks: [PendingPtyInputChunk] = []
    private var pendingRendererOutput = Data()
    private var invalidationHandoffOutput = Data()
    private var isInvalidated = false

    init(
        ptyHandle: AlanDarwinTerminalPtyHandle,
        hostFileDescriptor: Int32,
        ptyFileDescriptor: Int32,
        outputProcessor: AlanTerminalPtyControlSequenceProcessor
    ) {
        self.ptyHandle = ptyHandle
        self.hostFileDescriptor = hostFileDescriptor
        self.ptyFileDescriptor = ptyFileDescriptor
        self.outputProcessor = outputProcessor
        ioQueue.setSpecific(key: ioQueueKey, value: 1)
    }

    deinit {
        invalidate()
    }

    func start(initialRendererOutput: [Data]) {
        performOnIOQueue {
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

            let outputSource = DispatchSource.makeReadSource(
                fileDescriptor: ptyFileDescriptor,
                queue: ioQueue
            )
            outputSource.setEventHandler { [weak self] in
                self?.drainPtyOutput()
            }
            ptyOutputSource = outputSource
            outputSource.resume()

            for chunk in initialRendererOutput {
                guard forwardPtyOutputOnQueue(chunk) else {
                    invalidate()
                    return
                }
            }
        }
    }

    func invalidate() {
        guard markInvalidated() else { return }
        performOnIOQueue {
            rendererInputSource?.cancel()
            rendererInputSource = nil
            ptyOutputSource?.cancel()
            ptyOutputSource = nil
            ptyInputWriteSource?.cancel()
            ptyInputWriteSource = nil
            rendererOutputWriteSource?.cancel()
            rendererOutputWriteSource = nil
            pendingPtyInputChunks.removeAll()
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

    private func forwardPtyOutputOnQueue(_ data: Data) -> Bool {
        guard !invalidated else { return false }
        guard !data.isEmpty else { return true }
        pendingRendererOutput.append(data)
        return drainPendingRendererOutput()
    }

    private func drainRendererInput() {
        guard !invalidated else { return }
        guard drainPendingPtyInput() else {
            invalidate()
            return
        }
        guard pendingPtyInputChunks.isEmpty else { return }

        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = Darwin.read(hostFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                let data = Data(buffer.prefix(count))
                guard enqueuePtyInput(data, countedBytes: data.count) else {
                    invalidate()
                    return
                }
                guard pendingPtyInputChunks.isEmpty else { return }
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

    private func drainPtyOutput() {
        guard !invalidated else { return }
        var collected = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)

        while true {
            let count = Darwin.read(ptyFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                collected.append(buffer, count: count)
                continue
            }
            if count == 0 {
                Task { @MainActor [weak self] in
                    _ = self?.ptyHandle?.refreshExitStatus()
                }
                break
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                break
            }
            break
        }

        guard !collected.isEmpty else { return }
        let response = outputProcessor.process(collected)
        if let transition = response.semanticShellStateTransition {
            Task { @MainActor [weak self] in
                self?.ptyHandle?.recordSemanticShellState(transition)
            }
        }
        if response.didRespond {
            guard enqueuePtyInput(response.ptyResponse, countedBytes: 0) else {
                invalidate()
                return
            }
        }

        guard !response.rendererOutput.isEmpty else { return }
        Task { @MainActor [weak self] in
            self?.ptyHandle?.recordPtyOutput(response.rendererOutput)
        }
        guard forwardPtyOutputOnQueue(response.rendererOutput) else {
            invalidate()
            return
        }
    }

    private func enqueuePtyInput(_ data: Data, countedBytes: Int) -> Bool {
        guard !data.isEmpty else { return true }
        pendingPtyInputChunks.append(
            PendingPtyInputChunk(
                data: data,
                countedBytesRemaining: max(0, min(countedBytes, data.count))
            )
        )
        return drainPendingPtyInput()
    }

    private func drainPendingPtyInput() -> Bool {
        guard !invalidated else { return false }
        var acceptedInputBytes = 0

        while !pendingPtyInputChunks.isEmpty {
            var chunk = pendingPtyInputChunks.removeFirst()
            let result = chunk.data.withUnsafeBytes { rawBuffer -> Int in
                guard let baseAddress = rawBuffer.baseAddress else { return -1 }
                return Darwin.write(ptyFileDescriptor, baseAddress, rawBuffer.count)
            }

            if result > 0 {
                let countedBytes = min(chunk.countedBytesRemaining, result)
                acceptedInputBytes += countedBytes
                if result < chunk.data.count {
                    chunk.data.removeFirst(result)
                    chunk.countedBytesRemaining -= countedBytes
                    pendingPtyInputChunks.insert(chunk, at: 0)
                    ensurePtyInputWriteSource()
                    break
                }
                continue
            }

            pendingPtyInputChunks.insert(chunk, at: 0)
            if errno == EAGAIN || errno == EWOULDBLOCK {
                ensurePtyInputWriteSource()
                break
            }
            return false
        }

        if pendingPtyInputChunks.isEmpty {
            cancelPtyInputWriteSource()
        }
        if acceptedInputBytes > 0 {
            Task { @MainActor [weak self] in
                self?.ptyHandle?.recordRendererInputBytes(acceptedInputBytes)
            }
        }
        return true
    }

    private func ensurePtyInputWriteSource() {
        guard !invalidated, ptyInputWriteSource == nil else { return }
        let writeSource = DispatchSource.makeWriteSource(
            fileDescriptor: ptyFileDescriptor,
            queue: ioQueue
        )
        writeSource.setEventHandler { [weak self] in
            guard let self else { return }
            guard self.drainPendingPtyInput() else {
                self.invalidate()
                return
            }
        }
        writeSource.resume()
        ptyInputWriteSource = writeSource
    }

    private func cancelPtyInputWriteSource() {
        ptyInputWriteSource?.cancel()
        ptyInputWriteSource = nil
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

private func withCStringArray<Result>(
    _ values: [String],
    _ body: (UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Result
) -> Result {
    let cStrings = values.map { strdup($0) }
    let argv = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>.allocate(
        capacity: cStrings.count + 1
    )
    for index in cStrings.indices {
        argv[index] = cStrings[index]
    }
    argv[cStrings.count] = nil
    defer {
        for cString in cStrings {
            free(cString)
        }
        argv.deallocate()
    }
    return body(argv)
}

private func waitStatusTermSignal(_ status: Int32) -> Int32 {
    status & 0x7f
}

private func waitStatusExited(_ status: Int32) -> Bool {
    waitStatusTermSignal(status) == 0
}

private func waitStatusSignaled(_ status: Int32) -> Bool {
    let signal = waitStatusTermSignal(status)
    return signal != 0 && signal != 0x7f
}

private func waitStatusExitCode(_ status: Int32) -> Int32 {
    (status >> 8) & 0xff
}

#endif
