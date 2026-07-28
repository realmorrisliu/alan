import Darwin
import Foundation

final class AlanPrivilegedHelperPTYSessionStore {
    private static let maxPendingInputBytes = 4 * 1024 * 1024
    private let identity: AlanPrivilegedHelperXPCIdentity
    private var sessions: [String: AlanPrivilegedHelperPTYSession] = [:]

    init(identity: AlanPrivilegedHelperXPCIdentity) {
        self.identity = identity
    }

    func start(
        request: AlanXPCManagedUserPTYStartRequest,
        managedUserService: AlanPrivilegedHelperManagedUserService
    ) -> Result<AlanXPCManagedUserPTYSession, AlanXPCPrivilegedHelperDiagnostic> {
        switch managedUserService.accountReadyForPTY(
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            shell: request.shell,
            channelID: request.channelID
        ) {
        case .failure(let diagnostic):
            return .failure(diagnostic)
        case .success(let account):
            var master: Int32 = -1
            var pid: pid_t = 0
            let shellLaunch = AlanTerminalShellLaunch.integratingGhostty(
                executablePath: request.shell,
                arguments: ["-l"],
                environment: AlanPrivilegedHelperPTYSupport.environment(
                    accountName: request.accountName,
                    home: request.homeDirectory,
                    shell: request.shell
                ),
                resourcesPath: request.shellIntegrationResourcesPath
            )
            let argvValues = [shellLaunch.argumentZero] + shellLaunch.arguments
            let envValues = shellLaunch.environment.map {
                "\($0.key)=\($0.value)"
            }.sorted()
            let workingDirectory = request.workingDirectory.isEmpty
                ? request.homeDirectory
                : request.workingDirectory
            let spawnResult = request.shell.withCString { executable in
                workingDirectory.withCString { workingDirectory in
                    request.accountName.withCString { accountName in
                        AlanPrivilegedHelperPTYSupport.withCStringArray(argvValues) { argv in
                            AlanPrivilegedHelperPTYSupport.withCStringArray(envValues) { envp in
                                alanDarwinPtySpawnAsUser(
                                    executable,
                                    argv,
                                    envp,
                                    workingDirectory,
                                    accountName,
                                    account.uid,
                                    account.gid,
                                    UInt16(max(1, min(request.rows, Int(UInt16.max)))),
                                    UInt16(max(1, min(request.columns, Int(UInt16.max)))),
                                    &master,
                                    &pid
                                )
                            }
                        }
                    }
                }
            }
            guard spawnResult == 0, master >= 0 else {
                if master >= 0 {
                    close(master)
                }
                return .failure(
                    diagnostic(
                        operation: .startManagedUserPTY,
                        accountName: request.accountName,
                        code: .ptySpawnFailed,
                        message: "Privileged helper could not start the managed-user PTY."
                    )
                )
            }
            setNonBlocking(master)
            let sessionID = UUID().uuidString
            sessions[sessionID] = AlanPrivilegedHelperPTYSession(
                sessionID: sessionID,
                accountName: request.accountName,
                contentID: request.contentID,
                masterFileDescriptor: master,
                processID: pid
            )
            return .success(
                AlanXPCManagedUserPTYSession(
                    sessionID: sessionID,
                    accountName: request.accountName,
                    contentID: request.contentID,
                    helperOwnsChildProcess: true,
                    sanitizedMessage: "Privileged helper PTY session started."
                )
            )
        }
    }

    func read(
        _ request: AlanXPCManagedUserPTYReadRequest
    ) -> Result<AlanXPCManagedUserPTYOutputChunk, AlanXPCPrivilegedHelperDiagnostic> {
        guard let session = sessions[request.sessionID] else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: nil,
                    code: .helperUnavailable,
                    message: "Managed User PTY session is missing."
                )
            )
        }
        guard session.masterFileDescriptor >= 0 else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: session.accountName,
                    code: .helperUnavailable,
                    message: "Managed User PTY output stream is closed."
                )
            )
        }
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return .failure(diagnostic)
        }

        let maxBytes = max(1, min(request.maxBytes, 64 * 1024))
        let foregroundProcessGroupState = foregroundProcessGroupState(for: session)
        var buffer = [UInt8](repeating: 0, count: maxBytes)
        let count = Darwin.read(session.masterFileDescriptor, &buffer, maxBytes)
        if count > 0 {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(buffer.prefix(count)),
                    final: false,
                    foregroundProcessGroupState: foregroundProcessGroupState,
                    sanitizedMessage: "Privileged helper read Managed User PTY output."
                )
            )
        }
        if count == 0 {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(),
                    final: true,
                    foregroundProcessGroupState: foregroundProcessGroupState,
                    sanitizedMessage: "Managed User PTY output stream ended."
                )
            )
        }
        if errno == EAGAIN || errno == EWOULDBLOCK {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(),
                    final: false,
                    foregroundProcessGroupState: foregroundProcessGroupState,
                    sanitizedMessage: nil
                )
            )
        }
        return .failure(
            diagnostic(
                operation: .readManagedUserPTY,
                accountName: session.accountName,
                code: .helperUnavailable,
                message: "Managed User PTY output read failed."
            )
        )
    }

    func write(_ request: AlanXPCManagedUserPTYInputRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        let data = request.data
        guard data.count <= Self.maxPendingInputBytes,
              session.pendingInput.count <= Self.maxPendingInputBytes - data.count
        else {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: "Managed User PTY input queue is full.")
        }
        session.pendingInput.append(data)
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: diagnostic.sanitizedMessage)
        }
        return accepted(.writeManagedUserPTY, session: session, message: "Privileged helper accepted PTY input.")
    }

    func resize(_ request: AlanXPCManagedUserPTYResizeRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.resizeManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        var size = winsize(
            ws_row: UInt16(max(1, min(request.rows, Int(UInt16.max)))),
            ws_col: UInt16(max(1, min(request.columns, Int(UInt16.max)))),
            ws_xpixel: 0,
            ws_ypixel: 0
        )
        guard ioctl(session.masterFileDescriptor, TIOCSWINSZ, &size) == 0 else {
            return rejected(.resizeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: "Managed User PTY resize failed.")
        }
        return accepted(.resizeManagedUserPTY, session: session, message: "Privileged helper resized PTY session.")
    }

    func closeInput(sessionID: String) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[sessionID] else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, message: "Managed User PTY session is missing.")
        }
        guard session.masterFileDescriptor >= 0 else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: "Managed User PTY input stream is closed.")
        }
        let eof = Data([UInt8(4)])
        guard session.pendingInput.count <= Self.maxPendingInputBytes - eof.count else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: "Managed User PTY input queue is full.")
        }
        session.pendingInput.append(eof)
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: diagnostic.sanitizedMessage)
        }
        return accepted(.closeManagedUserPTYInput, session: session, message: "Privileged helper sent PTY EOF.")
    }

    func signal(_ request: AlanXPCManagedUserPTYSignalRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.signalManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        let signalNumber: Int32
        switch request.signal {
        case .interrupt:
            signalNumber = SIGINT
        case .terminate:
            signalNumber = SIGTERM
        case .kill:
            signalNumber = SIGKILL
        }
        let foregroundProcessGroupID = foregroundProcessGroupID(for: session)
        let targetProcessGroupID = foregroundProcessGroupID ?? session.processID
        var result = kill(-targetProcessGroupID, signalNumber)
        if result != 0, targetProcessGroupID != session.processID {
            result = kill(-session.processID, signalNumber)
        }
        if result != 0 {
            result = kill(session.processID, signalNumber)
        }
        guard result == 0 else {
            return rejected(
                .signalManagedUserPTY,
                sessionID: request.sessionID,
                accountName: session.accountName,
                message: "Managed User PTY signal delivery failed."
            )
        }
        return accepted(.signalManagedUserPTY, session: session, message: "Privileged helper signaled PTY session.")
    }

    func observeExit(sessionID: String) -> AlanXPCManagedUserPTYExitObservation? {
        guard let session = sessions[sessionID] else { return nil }
        if let observation = session.finalObservation {
            return observation
        }
        var status: Int32 = 0
        let result = waitpid(session.processID, &status, WNOHANG)
        guard result == session.processID else { return nil }
        let observation = exitObservation(sessionID: sessionID, status: status)
        session.finalObservation = observation
        closeIfNeeded(session)
        sessions.removeValue(forKey: sessionID)
        return observation
    }

    func terminate(sessionID: String) -> AlanXPCPrivilegedHelperDiagnostic {
        guard let session = sessions[sessionID] else {
            return diagnostic(
                operation: .terminatePTY,
                accountName: nil,
                code: nil,
                message: "Managed User PTY session was already absent."
            )
        }
        if let foregroundProcessGroupID = foregroundProcessGroupID(for: session),
           foregroundProcessGroupID != session.processID
        {
            _ = kill(-foregroundProcessGroupID, SIGTERM)
        }
        _ = kill(-session.processID, SIGTERM)
        _ = kill(session.processID, SIGTERM)
        closeIfNeeded(session)
        var status: Int32 = 0
        _ = waitpid(session.processID, &status, WNOHANG)
        session.finalObservation = AlanXPCManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Privileged helper terminated PTY session."
        )
        sessions.removeValue(forKey: sessionID)
        return diagnostic(
            operation: .terminatePTY,
            accountName: session.accountName,
            code: nil,
            message: "Privileged helper terminated PTY session."
        )
    }

    func terminateAll() {
        for sessionID in Array(sessions.keys) {
            _ = terminate(sessionID: sessionID)
        }
    }

    private func closeIfNeeded(_ session: AlanPrivilegedHelperPTYSession) {
        if session.masterFileDescriptor >= 0 {
            close(session.masterFileDescriptor)
            session.masterFileDescriptor = -1
        }
    }

    private func foregroundProcessGroupState(
        for session: AlanPrivilegedHelperPTYSession
    ) -> AlanXPCManagedUserPTYForegroundProcessGroupState {
        guard let foregroundProcessGroupID = foregroundProcessGroupID(for: session) else {
            return .unavailable
        }
        return foregroundProcessGroupID == session.processID ? .shell : .foreground
    }

    private func foregroundProcessGroupID(
        for session: AlanPrivilegedHelperPTYSession
    ) -> pid_t? {
        let foregroundProcessGroupID = tcgetpgrp(session.masterFileDescriptor)
        return foregroundProcessGroupID > 0 ? foregroundProcessGroupID : nil
    }

    private func setNonBlocking(_ fileDescriptor: Int32) {
        let flags = fcntl(fileDescriptor, F_GETFL)
        guard flags >= 0 else { return }
        _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
    }

    private func drainPendingInput(
        _ session: AlanPrivilegedHelperPTYSession
    ) -> Result<Void, AlanXPCPrivilegedHelperDiagnostic> {
        while !session.pendingInput.isEmpty {
            let written = session.pendingInput.withUnsafeBytes { buffer -> Int in
                guard let base = buffer.baseAddress else { return 0 }
                return Darwin.write(session.masterFileDescriptor, base, buffer.count)
            }
            if written > 0 {
                session.pendingInput.removeFirst(written)
                continue
            }
            if written == 0 {
                return .success(())
            }
            if errno == EINTR {
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return .success(())
            }
            return .failure(
                diagnostic(
                    operation: .writeManagedUserPTY,
                    accountName: session.accountName,
                    code: .helperUnavailable,
                    message: "Managed User PTY input failed."
                )
            )
        }
        return .success(())
    }

    private func accepted(
        _ operation: AlanPrivilegedHelperXPCOperation,
        session: AlanPrivilegedHelperPTYSession,
        message: String
    ) -> AlanXPCManagedUserPTYControlResult {
        AlanXPCManagedUserPTYControlResult(
            accepted: true,
            diagnostic: diagnostic(
                operation: operation,
                accountName: session.accountName,
                code: nil,
                message: message
            )
        )
    }

    private func rejected(
        _ operation: AlanPrivilegedHelperXPCOperation,
        sessionID: String,
        accountName: String? = nil,
        message: String
    ) -> AlanXPCManagedUserPTYControlResult {
        AlanXPCManagedUserPTYControlResult(
            accepted: false,
            diagnostic: diagnostic(
                operation: operation,
                accountName: accountName,
                code: .helperUnavailable,
                message: message
            )
        )
    }

    private func exitObservation(
        sessionID: String,
        status: Int32
    ) -> AlanXPCManagedUserPTYExitObservation {
        if AlanPrivilegedHelperPTYSupport.waitStatusExited(status) {
            return AlanXPCManagedUserPTYExitObservation(
                sessionID: sessionID,
                final: true,
                exitCode: AlanPrivilegedHelperPTYSupport.waitStatusExitCode(status),
                terminatingSignal: nil,
                sanitizedMessage: "Managed User PTY child exited."
            )
        }
        if AlanPrivilegedHelperPTYSupport.waitStatusSignaled(status) {
            return AlanXPCManagedUserPTYExitObservation(
                sessionID: sessionID,
                final: true,
                exitCode: nil,
                terminatingSignal: AlanPrivilegedHelperPTYSupport.waitStatusTermSignal(status),
                sanitizedMessage: "Managed User PTY child exited after signal."
            )
        }
        return AlanXPCManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Managed User PTY child exited."
        )
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperXPCOperation,
        accountName: String?,
        code: AlanPrivilegedHelperXPCErrorCode?,
        message: String
    ) -> AlanXPCPrivilegedHelperDiagnostic {
        AlanXPCPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: identity.channelID,
            accountName: accountName,
            operation: operation.diagnosticOperationName,
            code: AlanPrivilegedHelperPTYSupport.mappedAppErrorCode(code),
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message)
        )
    }
}

private final class AlanPrivilegedHelperPTYSession {
    let sessionID: String
    let accountName: String
    let contentID: String
    var masterFileDescriptor: Int32
    let processID: pid_t
    var finalObservation: AlanXPCManagedUserPTYExitObservation?
    var pendingInput = Data()

    init(
        sessionID: String,
        accountName: String,
        contentID: String,
        masterFileDescriptor: Int32,
        processID: pid_t
    ) {
        self.sessionID = sessionID
        self.accountName = accountName
        self.contentID = contentID
        self.masterFileDescriptor = masterFileDescriptor
        self.processID = processID
    }

    deinit {
        if masterFileDescriptor >= 0 {
            close(masterFileDescriptor)
        }
    }
}
