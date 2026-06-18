import Foundation
import Darwin

#if os(macOS)
final class AlanShellSocketServer {
    private static let maxRequestBytes = 1_048_576
    private static let readTimeoutSeconds = 5
    private static let commandResponseTimeoutSeconds: TimeInterval = 5
    private static let maxConcurrentClients = 4

    private let socketURL: URL
    private let queue = DispatchQueue(label: "dev.alan.shell.control.socket", qos: .userInitiated)
    private let clientQueue = DispatchQueue(
        label: "dev.alan.shell.control.socket.clients",
        qos: .userInitiated,
        attributes: .concurrent
    )
    private let clientSemaphore = DispatchSemaphore(value: AlanShellSocketServer.maxConcurrentClients)
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private let commandHandler: (AlanShellControlCommand) -> AlanShellControlResponse
    private let stateAdoptionHandler: (ShellStateSnapshot) -> Void
    private let sideEffectHandler: (AlanShellLocalCommandSideEffect) -> Void
    private let debugEnabled = ProcessInfo.processInfo.environment["ALAN_SHELL_DEBUG_SOCKET"] == "1"
    private let stateLock = NSLock()
    private var listeningFileDescriptor: Int32 = -1
    private var isRunning = false
    private var lastCachedState: ShellStateSnapshot?
    private var lastPublishedState: ShellStateSnapshot?

    init(
        socketURL: URL,
        commandHandler: @escaping (AlanShellControlCommand) -> AlanShellControlResponse,
        stateAdoptionHandler: @escaping (ShellStateSnapshot) -> Void,
        sideEffectHandler: @escaping (AlanShellLocalCommandSideEffect) -> Void
    ) {
        self.socketURL = socketURL
        self.commandHandler = commandHandler
        self.stateAdoptionHandler = stateAdoptionHandler
        self.sideEffectHandler = sideEffectHandler
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    }

    deinit {
        stop()
    }

    @discardableResult
    func mergePublishedState(
        _ state: ShellStateSnapshot
    ) -> (previous: ShellStateSnapshot?, merged: ShellStateSnapshot) {
        stateLock.lock()
        let previousState = lastPublishedState
        let mergedState = AlanShellPublishedStateMerger.merge(
            authoritative: lastCachedState ?? lastPublishedState,
            incoming: state
        )
        lastCachedState = mergedState
        lastPublishedState = mergedState
        stateLock.unlock()
        return (previousState, mergedState)
    }

    func start() {
        stop()
        try? FileManager.default.createDirectory(
            at: socketURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        unlink(socketURL.path)

        let socketFD = socket(AF_UNIX, SOCK_STREAM, 0)
        guard socketFD >= 0 else { return }
        guard bindSocket(fileDescriptor: socketFD) else {
            close(socketFD)
            unlink(socketURL.path)
            return
        }
        guard listen(socketFD, 16) == 0 else {
            close(socketFD)
            unlink(socketURL.path)
            return
        }

        debugLog("socket start path=\(socketURL.path) fd=\(socketFD)")
        listeningFileDescriptor = socketFD
        isRunning = true
        queue.async { [weak self] in
            self?.acceptLoop(fileDescriptor: socketFD)
        }
    }

    func stop() {
        isRunning = false
        if listeningFileDescriptor >= 0 {
            debugLog("socket stop fd=\(listeningFileDescriptor)")
            close(listeningFileDescriptor)
            unlink(socketURL.path)
        }
        listeningFileDescriptor = -1
    }

    private func bindSocket(fileDescriptor: Int32) -> Bool {
        var address = sockaddr_un()
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        address.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketURL.path.utf8CString
        let maxPathLength = MemoryLayout.size(ofValue: address.sun_path)
        guard pathBytes.count <= maxPathLength else { return false }

        withUnsafeMutablePointer(to: &address.sun_path.0) { pointer in
            pointer.initialize(repeating: 0, count: maxPathLength)
            pathBytes.withUnsafeBufferPointer { bytes in
                guard let source = bytes.baseAddress else { return }
                pointer.update(from: source, count: bytes.count)
            }
        }

        var bindResult: Int32 = -1
        withUnsafePointer(to: &address) { addressPointer in
            addressPointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
                bindResult = Darwin.bind(
                    fileDescriptor,
                    socketAddress,
                    socklen_t(MemoryLayout<sockaddr_un>.size)
                )
            }
        }

        return bindResult == 0
    }

    private func acceptLoop(fileDescriptor: Int32) {
        while isRunning {
            let clientFD = accept(fileDescriptor, nil, nil)
            if clientFD == -1 {
                if errno == EINTR {
                    continue
                }
                if !isRunning || errno == EBADF || errno == EINVAL {
                    debugLog("socket accept loop exit errno=\(errno)")
                    return
                }
                debugLog("socket accept retry errno=\(errno)")
                continue
            }

            debugLog("socket accepted client fd=\(clientFD)")
            configureClient(fileDescriptor: clientFD)
            clientQueue.async { [weak self] in
                guard let self else {
                    AlanShellSocketServer.closeClient(clientFD)
                    return
                }
                guard self.clientSemaphore.wait(timeout: .now() + 5) == .success else {
                    self.debugLog("socket client rejected by concurrency limit fd=\(clientFD)")
                    AlanShellSocketServer.closeClient(clientFD)
                    return
                }
                defer { self.clientSemaphore.signal() }
                self.handleClient(fileDescriptor: clientFD)
            }
        }
    }

    private func configureClient(fileDescriptor: Int32) {
        var noSigPipe: Int32 = 1
        setsockopt(
            fileDescriptor,
            SOL_SOCKET,
            SO_NOSIGPIPE,
            &noSigPipe,
            socklen_t(MemoryLayout<Int32>.size)
        )

        var timeout = timeval(tv_sec: Self.readTimeoutSeconds, tv_usec: 0)
        setsockopt(
            fileDescriptor,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &timeout,
            socklen_t(MemoryLayout<timeval>.size)
        )
        setsockopt(
            fileDescriptor,
            SOL_SOCKET,
            SO_SNDTIMEO,
            &timeout,
            socklen_t(MemoryLayout<timeval>.size)
        )
    }

    private func handleClient(fileDescriptor: Int32) {
        guard let requestData = readRequest(from: fileDescriptor),
              let command = try? decoder.decode(AlanShellControlCommand.self, from: requestData)
        else {
            debugLog("socket decode failed fd=\(fileDescriptor)")
            AlanShellSocketServer.closeClient(fileDescriptor)
            return
        }

        debugLog("socket command=\(command.command) fd=\(fileDescriptor)")
        if let localResponse = handleLocally(command) {
            let responseData = (try? encoder.encode(localResponse)) ?? Data()
            debugLog("socket cached response bytes=\(responseData.count) fd=\(fileDescriptor)")
            AlanShellSocketServer.write(responseData, to: fileDescriptor)
            AlanShellSocketServer.write(Data([0x0A]), to: fileDescriptor)
            AlanShellSocketServer.closeClient(fileDescriptor)
            return
        }

        let semaphore = DispatchSemaphore(value: 0)
        var response: AlanShellControlResponse?
        DispatchQueue.main.async { [weak self] in
            guard let self else {
                semaphore.signal()
                return
            }
            response = self.commandHandler(command)
            semaphore.signal()
        }
        if semaphore.wait(timeout: .now() + Self.commandResponseTimeoutSeconds) != .success {
            debugLog("socket response timeout fd=\(fileDescriptor)")
            let timeoutResponse = AlanShellControlResponse(
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
                deliveryCode: TerminalRuntimeDeliveryCode.timeout.rawValue,
                runtimePhase: nil,
                latestEventID: nil,
                errorCode: "command_timeout",
                errorMessage: "alan terminal workspace control command timed out."
            )
            let responseData = (try? encoder.encode(timeoutResponse)) ?? Data()
            AlanShellSocketServer.write(responseData, to: fileDescriptor)
            AlanShellSocketServer.write(Data([0x0A]), to: fileDescriptor)
            AlanShellSocketServer.closeClient(fileDescriptor)
            return
        }

        let resolvedResponse =
            response
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
        let responseData = (try? encoder.encode(resolvedResponse)) ?? Data()
        debugLog("socket response bytes=\(responseData.count) fd=\(fileDescriptor)")
        AlanShellSocketServer.write(responseData, to: fileDescriptor)
        AlanShellSocketServer.write(Data([0x0A]), to: fileDescriptor)
        AlanShellSocketServer.closeClient(fileDescriptor)
    }

    private func readRequest(from fileDescriptor: Int32) -> Data? {
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)

        while true {
            let bytesRead = read(fileDescriptor, &buffer, buffer.count)
            if bytesRead > 0 {
                data.append(buffer, count: bytesRead)
                guard data.count <= Self.maxRequestBytes else {
                    debugLog("socket request too large fd=\(fileDescriptor) bytes=\(data.count)")
                    return nil
                }
                if data.contains(0x0A) {
                    debugLog("socket read newline fd=\(fileDescriptor) bytes=\(data.count)")
                    break
                }
                continue
            }

            if bytesRead == 0 {
                debugLog("socket read eof fd=\(fileDescriptor) bytes=\(data.count)")
                break
            }

            if errno == EINTR {
                continue
            }

            debugLog("socket read error fd=\(fileDescriptor) errno=\(errno)")
            return nil
        }

        if let newlineIndex = data.firstIndex(of: 0x0A) {
            data = data.prefix(upTo: newlineIndex)
        }

        return data.isEmpty ? nil : data
    }

    private static func write(_ data: Data, to fileDescriptor: Int32) {
        data.withUnsafeBytes { bytes in
            guard let baseAddress = bytes.baseAddress else { return }
            var offset = 0
            while offset < bytes.count {
                let written = Darwin.write(
                    fileDescriptor,
                    baseAddress.advanced(by: offset),
                    bytes.count - offset
                )
                if written > 0 {
                    offset += written
                    continue
                }
                if written == -1 && errno == EINTR {
                    continue
                }
                break
            }
        }
    }

    private static func closeClient(_ fileDescriptor: Int32) {
        shutdown(fileDescriptor, SHUT_RDWR)
        close(fileDescriptor)
    }

    func handleLocally(_ command: AlanShellControlCommand) -> AlanShellControlResponse? {
        guard !command.command.requiresHostHandler else { return nil }

        let localResult: AlanShellLocalCommandResult? = {
            stateLock.lock()
            defer { stateLock.unlock() }
            guard let state = lastCachedState ?? lastPublishedState else { return nil }
            let result = AlanShellLocalCommandExecutor.execute(command: command, state: state)
            if let updatedState = result?.updatedState {
                lastCachedState = updatedState
            }
            return result
        }()

        guard let localResult else { return nil }
        if let updatedState = localResult.updatedState {
            if Thread.isMainThread {
                stateAdoptionHandler(updatedState)
            } else {
                DispatchQueue.main.sync {
                    stateAdoptionHandler(updatedState)
                }
            }
        }
        if let sideEffect = localResult.sideEffect {
            sideEffectHandler(sideEffect)
        }
        return localResult.response
    }

    private func debugLog(_ message: String) {
        guard debugEnabled else { return }
        let logURL = FileManager.default.temporaryDirectory.appendingPathComponent("alan-shell-socket-debug.log")
        let line = "[\(ISO8601DateFormatter().string(from: .now))] \(message)\n"
        if FileManager.default.fileExists(atPath: logURL.path) {
            if let handle = try? FileHandle(forWritingTo: logURL) {
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: Data(line.utf8))
                try? handle.close()
            }
        } else {
            try? Data(line.utf8).write(to: logURL, options: .atomic)
        }
    }
}

private extension AlanShellControlCommandKind {
    var requiresHostHandler: Bool {
        switch self {
        case .spaceCreate, .tabOpen, .tabPin, .tabReorder, .paneSplit,
            .paneMove, .paneMoveWithinTab, .paneSpatialFocus, .paneResizeSplit,
            .paneEqualizeSplits, .paneZoom, .paneUnzoom, .terminalRenderMetrics,
            .quickTerminalFocus, .terminalSendText, .terminalSendKey, .agentActivity,
            .performanceDiagnosticsSetEnabled,
            .performanceDiagnosticsExportRecent, .performanceDiagnosticsRecordChildPressure:
            return true
        default:
            return false
        }
    }
}

#endif
