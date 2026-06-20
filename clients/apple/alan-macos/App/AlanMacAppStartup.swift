#if os(macOS)
import Darwin
import Foundation

enum AlanMacAppStartup {
    private static let devHelperInstallAndExitArgument =
        "--alan-dev-privileged-helper-install-and-exit"
    private static let devHelperRestartAndExitArgument =
        "--alan-dev-privileged-helper-restart-and-exit"
    private static let devHelperSmokeAndExitArgument =
        "--alan-dev-privileged-helper-smoke-and-exit"

    static func handleDevPrivilegedHelperCommandIfRequested() {
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains(devHelperInstallAndExitArgument) {
            installDevPrivilegedHelperAndExit()
        }
        if arguments.contains(devHelperRestartAndExitArgument) {
            restartDevPrivilegedHelperAndExit()
        }
        if arguments.contains(devHelperSmokeAndExitArgument) {
            smokeDevPrivilegedHelperAndExit()
        }
    }

    private static func installDevPrivilegedHelperAndExit() -> Never {
        guard AlanInstallChannel.current() == .dev else {
            fputs("Alan privileged helper smoke command is dev-channel only.\n", stderr)
            Darwin.exit(2)
        }

        let manager = AlanPrivilegedHelperAppServiceManager(channel: .dev)
        let result = manager.installOrUpdate()
        let state = result.status.state.rawValue
        if let diagnostic = result.diagnostic {
            fputs(
                "Alan Dev privileged helper \(result.action.rawValue) failed: \(diagnostic.sanitizedMessage)\n",
                stderr
            )
            fputs("state=\(state)\n", stderr)
            Darwin.exit(1)
        }
        print("Alan Dev privileged helper \(result.action.rawValue) requested.")
        print("state=\(state)")
        if let message = result.status.sanitizedMessage {
            print("message=\(message)")
        }
        Darwin.exit(result.status.isHealthy ? 0 : 3)
    }

    private static func restartDevPrivilegedHelperAndExit() -> Never {
        guard AlanInstallChannel.current() == .dev else {
            fputs("Alan privileged helper smoke command is dev-channel only.\n", stderr)
            Darwin.exit(2)
        }

        let manager = AlanPrivilegedHelperAppServiceManager(channel: .dev)
        let uninstall = manager.uninstall()
        if let diagnostic = uninstall.diagnostic {
            fputs(
                "Alan Dev privileged helper uninstall failed: \(diagnostic.sanitizedMessage)\n",
                stderr
            )
            fputs("state=\(uninstall.status.state.rawValue)\n", stderr)
            Darwin.exit(1)
        }
        print("Alan Dev privileged helper uninstall requested.")
        print("uninstallState=\(uninstall.status.state.rawValue)")

        let install = manager.installOrUpdate()
        if let diagnostic = install.diagnostic {
            fputs(
                "Alan Dev privileged helper \(install.action.rawValue) failed: \(diagnostic.sanitizedMessage)\n",
                stderr
            )
            fputs("state=\(install.status.state.rawValue)\n", stderr)
            Darwin.exit(1)
        }
        print("Alan Dev privileged helper \(install.action.rawValue) requested.")
        print("state=\(install.status.state.rawValue)")
        Darwin.exit(install.status.isHealthy ? 0 : 3)
    }

    private static func smokeDevPrivilegedHelperAndExit() -> Never {
        guard AlanInstallChannel.current() == .dev else {
            fputs("Alan privileged helper smoke command is dev-channel only.\n", stderr)
            Darwin.exit(2)
        }
        let environment = ProcessInfo.processInfo.environment
        let client = AlanPrivilegedHelperAppClient(channel: .dev)
        let status = client.status()
        print("channel=dev")
        print("helperStatus.state=\(status.state.rawValue)")
        if let message = status.sanitizedMessage {
            print("helperStatus.message=\(message)")
        }
        guard status.isHealthy else {
            Darwin.exit(1)
        }

        guard let accountName = normalized(environment["ALAN_PRIVILEGED_HELPER_SMOKE_ACCOUNT"]) else {
            print("status smoke passed; set ALAN_PRIVILEGED_HELPER_SMOKE_ACCOUNT to run managed-user diagnosis")
            Darwin.exit(0)
        }

        let request = ManagedTerminalAccountRequest(
            accountName: accountName,
            guiUserName: normalized(environment["ALAN_PRIVILEGED_HELPER_SMOKE_GUI_USER"]) ?? NSUserName()
        )
        var diagnosis = client.diagnoseManagedUser(request)
        print("diagnose.readiness=\(diagnosis.readinessState.rawValue)")
        print("diagnose.ptySmokeVerified=\(diagnosis.ptySmokeVerified)")
        if let diagnostic = diagnosis.diagnostic {
            print("diagnose.message=\(diagnostic.sanitizedMessage)")
        }
        if diagnosis.readinessState != .ready || !diagnosis.ptySmokeVerified {
            guard envFlag("ALAN_PRIVILEGED_HELPER_SMOKE_APPLY_REPAIR") else {
                Darwin.exit(4)
            }
            let plan = ManagedTerminalAccountPlanner.plan(request: request, diagnosis: diagnosis)
            let repairSteps = plan.steps
                .map { String(describing: $0.kind) }
                .joined(separator: ",")
            print("repairPlan.status=\(String(describing: plan.status))")
            print("repairPlan.steps=\(repairSteps)")
            let executor = ManagedTerminalAccountHelperExecutor(
                channel: .dev,
                helperClient: client
            )
            let result = executor.apply(plan)
            let completedSteps = result.completedSteps
                .map { String(describing: $0) }
                .joined(separator: ",")
            print("repair.completedSteps=\(completedSteps)")
            if let failedStep = result.failedStep {
                fputs("repair failed at \(String(describing: failedStep))\n", stderr)
                for message in result.visibleDiagnostics {
                    fputs("\(message)\n", stderr)
                }
                Darwin.exit(6)
            }
            diagnosis = client.diagnoseManagedUser(request)
            print("postRepair.readiness=\(diagnosis.readinessState.rawValue)")
            print("postRepair.ptySmokeVerified=\(diagnosis.ptySmokeVerified)")
            guard diagnosis.readinessState == .ready, diagnosis.ptySmokeVerified else {
                Darwin.exit(4)
            }
        }

        guard envFlag("ALAN_PRIVILEGED_HELPER_SMOKE_START_PTY") else {
            print("diagnosis smoke passed; set ALAN_PRIVILEGED_HELPER_SMOKE_START_PTY=1 to start and terminate a live PTY")
            Darwin.exit(0)
        }

        let startRequest = AlanManagedUserPTYStartRequest(
            operationID: UUID().uuidString,
            channelID: "dev",
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            workingDirectory: request.homeDirectory,
            shell: request.shell,
            contentID: "privileged-helper-live-smoke-\(UUID().uuidString)",
            columns: 80,
            rows: 24
        )
        switch client.startManagedUserPTY(startRequest) {
        case .success(let session):
            print("startManagedUserPTY.sessionID=\(session.sessionID)")
            print("startManagedUserPTY.helperOwnsChildProcess=\(session.helperOwnsChildProcess)")
            let diagnostic = client.terminatePTY(sessionID: session.sessionID)
            print("terminatePTY.message=\(diagnostic.sanitizedMessage)")
            print("pty smoke passed")
            Darwin.exit(0)
        case .failure(let diagnostic):
            fputs("startManagedUserPTY failed: \(diagnostic.sanitizedMessage)\n", stderr)
            Darwin.exit(5)
        }
    }

    private static func envFlag(_ name: String) -> Bool {
        let value = ProcessInfo.processInfo.environment[name]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return value == "1" || value == "true" || value == "yes"
    }

    private static func normalized(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty
        else {
            return nil
        }
        return value
    }

    static func acquireSingletonOrTerminate() -> AlanAppSingletonGuard {
        do {
            switch try AlanAppSingletonGuard.acquire() {
            case .acquired(let guardHandle):
                return guardHandle
            case .alreadyRunning:
                AlanAppSingletonGuard.activateExistingInstance()
                Darwin.exit(0)
            }
        } catch {
            fatalError("alan could not acquire the macOS app singleton lock: \(error)")
        }
    }
}
#endif
