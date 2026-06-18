import Foundation

#if os(macOS)
private enum SmokeFailure: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let message):
            return message
        }
    }
}

private struct SessionRequest: Codable {
    let sessionID: String
}

private func fail(_ message: String) throws -> Never {
    throw SmokeFailure.message(message)
}

private func envFlag(_ name: String) -> Bool {
    let value = ProcessInfo.processInfo.environment[name]?
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
    return value == "1" || value == "true" || value == "yes"
}

@main
struct PrivilegedHelperLiveSmoke {
    static func main() {
        do {
            try run()
        } catch {
            fputs("Privileged helper live smoke failed: \(error)\n", stderr)
            exit(1)
        }
    }

    private static func run() throws {
        let environment = ProcessInfo.processInfo.environment
        let channel = AlanInstallChannel.current(environment: environment)
        guard channel == .dev || envFlag("ALAN_PRIVILEGED_HELPER_LIVE_SMOKE_ALLOW_STABLE") else {
            try fail("live smoke refuses stable helper unless ALAN_PRIVILEGED_HELPER_LIVE_SMOKE_ALLOW_STABLE=1")
        }

        let identity = channel.privilegedHelperIdentity
        let xpcClient = AlanPrivilegedHelperXPCClient(
            identity: identity.xpcIdentity,
            timeoutSeconds: timeoutSeconds(environment)
        )
        let encoder = JSONEncoder()
        let decoder = JSONDecoder()

        let status = xpcClient.helperStatus()
        print("channel=\(identity.channelID)")
        print("machService=\(identity.machServiceName)")
        print("helperStatus.accepted=\(status.accepted)")
        if !status.accepted {
            try fail(status.sanitizedMessage)
        }

        guard let accountName = normalized(environment["ALAN_PRIVILEGED_HELPER_SMOKE_ACCOUNT"]) else {
            print("status smoke passed; set ALAN_PRIVILEGED_HELPER_SMOKE_ACCOUNT to run managed-user diagnosis")
            return
        }

        let request = ManagedTerminalAccountRequest(
            accountName: accountName,
            guiUserName: normalized(environment["ALAN_PRIVILEGED_HELPER_SMOKE_GUI_USER"]) ?? NSUserName(),
            fullName: nil
        )
        let diagnosisResponse = xpcClient.perform(
            operation: .diagnoseManagedUser,
            payload: try encoder.encode(request)
        )
        print("diagnose.accepted=\(diagnosisResponse.accepted)")
        if !diagnosisResponse.accepted {
            try fail(diagnosisResponse.sanitizedMessage)
        }
        guard let diagnosisPayload = diagnosisResponse.payload else {
            try fail("diagnoseManagedUser returned no payload")
        }
        let diagnosis = try decoder.decode(AlanManagedUserDiagnosis.self, from: diagnosisPayload)
        print("diagnose.readiness=\(diagnosis.readinessState.rawValue)")
        print("diagnose.ptySmokeVerified=\(diagnosis.ptySmokeVerified)")
        if diagnosis.readinessState != .ready || !diagnosis.ptySmokeVerified {
            try fail(diagnosis.diagnostic?.sanitizedMessage ?? "managed user is not ready")
        }

        guard envFlag("ALAN_PRIVILEGED_HELPER_SMOKE_START_PTY") else {
            print("diagnosis smoke passed; set ALAN_PRIVILEGED_HELPER_SMOKE_START_PTY=1 to start and terminate a live PTY")
            return
        }

        let startRequest = AlanManagedUserPTYStartRequest(
            operationID: UUID().uuidString,
            channelID: identity.channelID,
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            shell: request.shell,
            contentID: "privileged-helper-live-smoke-\(UUID().uuidString)",
            columns: 80,
            rows: 24
        )
        let startResponse = xpcClient.perform(
            operation: .startManagedUserPTY,
            payload: try encoder.encode(startRequest)
        )
        print("startManagedUserPTY.accepted=\(startResponse.accepted)")
        if !startResponse.accepted {
            try fail(startResponse.sanitizedMessage)
        }
        guard let startPayload = startResponse.payload else {
            try fail("startManagedUserPTY returned no payload")
        }
        let session = try decoder.decode(AlanManagedUserPTYSession.self, from: startPayload)
        print("startManagedUserPTY.sessionID=\(session.sessionID)")
        print("startManagedUserPTY.helperOwnsChildProcess=\(session.helperOwnsChildProcess)")

        let terminateResponse = xpcClient.perform(
            operation: .terminatePTY,
            payload: try encoder.encode(SessionRequest(sessionID: session.sessionID))
        )
        print("terminatePTY.accepted=\(terminateResponse.accepted)")
        if !terminateResponse.accepted {
            try fail(terminateResponse.sanitizedMessage)
        }
        print("pty smoke passed")
    }

    private static func timeoutSeconds(_ environment: [String: String]) -> TimeInterval {
        guard let raw = environment["ALAN_PRIVILEGED_HELPER_SMOKE_TIMEOUT_SECONDS"],
              let parsed = TimeInterval(raw),
              parsed > 0
        else {
            return 5
        }
        return parsed
    }

    private static func normalized(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty
        else {
            return nil
        }
        return value
    }
}
#else
@main
struct PrivilegedHelperLiveSmoke {
    static func main() {
        fputs("Privileged helper live smoke is macOS-only.\n", stderr)
        exit(1)
    }
}
#endif
