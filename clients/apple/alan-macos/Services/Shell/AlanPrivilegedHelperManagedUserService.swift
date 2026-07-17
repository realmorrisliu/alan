import Darwin
import Foundation

final class AlanPrivilegedHelperManagedUserService {
    private let identity: AlanPrivilegedHelperXPCIdentity
    private let fileManager: FileManager

    init(identity: AlanPrivilegedHelperXPCIdentity, fileManager: FileManager = .default) {
        self.identity = identity
        self.fileManager = fileManager
    }

    func diagnose(
        request: AlanXPCManagedTerminalAccountRequest,
        verifyPTY: Bool
    ) -> AlanXPCManagedUserDiagnosis {
        guard validate(request).isEmpty else {
            return diagnosis(
                request: request,
                ownershipState: .missing,
                readinessState: .repairable,
                account: nil,
                ptySmokeVerified: false,
                diagnostic: diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .invalidAccountIdentifier,
                    message: "Managed User request is invalid."
                )
            )
        }

        let account = accountRecord(for: request.accountName)
        guard let account else {
            return diagnosis(
                request: request,
                ownershipState: .missing,
                readinessState: .accountMissing,
                account: nil,
                ptySmokeVerified: false,
                diagnostic: nil
            )
        }

        let markerExists = fileManager.fileExists(atPath: ownershipMarkerPath(for: request))
        let ownershipState: AlanXPCManagedUserOwnershipState
        if markerExists {
            ownershipState = .alanManaged
        } else {
            ownershipState = .notAlanManaged
        }

        guard ownershipState == .alanManaged else {
            return diagnosis(
                request: request,
                ownershipState: ownershipState,
                readinessState: .accountNotAlanManaged,
                account: account,
                ptySmokeVerified: false,
                diagnostic: diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .accountNotAlanManaged,
                    message: "Existing account is not Alan managed."
                )
            )
        }

        let repairable = account.isAdmin
            || account.homeDirectory != request.homeDirectory
            || !fileManager.fileExists(atPath: request.homeDirectory)
            || account.shell != request.shell
            || (request.hideFromLoginWindow && !account.hidden)
        guard !repairable else {
            return diagnosis(
                request: request,
                ownershipState: ownershipState,
                readinessState: .repairable,
                account: account,
                ptySmokeVerified: false,
                diagnostic: nil
            )
        }

        let ptySmokeVerified = verifyPTY ? verifyManagedUserPTY(request: request) : true
        return diagnosis(
            request: request,
            ownershipState: ownershipState,
            readinessState: ptySmokeVerified ? .ready : .ptySpawnFailed,
            account: account,
            ptySmokeVerified: ptySmokeVerified,
            diagnostic: ptySmokeVerified
                ? nil
                : diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .ptySpawnFailed,
                    message: "Managed User PTY smoke verification failed."
                )
        )
    }

    func apply(
        plan: AlanXPCManagedUserHelperPlan,
        ptySessions: AlanPrivilegedHelperPTYSessionStore
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        guard plan.channelID == identity.channelID else {
            return failedApply(
                completed: [],
                failed: plan.steps.first?.kind,
                message: "Privileged helper rejected a channel-mismatched Managed User plan."
            )
        }
        let validationErrors = validate(plan.request)
        guard validationErrors.isEmpty else {
            return failedApply(
                completed: [],
                failed: plan.steps.first?.kind,
                message: "Privileged helper rejected an invalid Managed User plan."
            )
        }

        var completed: [AlanXPCManagedUserHelperPlanStepKind] = []
        var destructiveAccountRecord: AlanManagedUserAccountRecord?
        for step in plan.steps {
            switch step.kind {
            case .deleteAccount:
                let revalidation = managedAccountRecordForDestructiveDeletion(plan.request)
                guard let account = revalidation.account else {
                    return failedApply(
                        completed: completed,
                        failed: step.kind,
                        message: revalidation.message
                    )
                }
                destructiveAccountRecord = account
            case .deleteHomeDirectory:
                let revalidation = validateHomeDeletionStillManaged(
                    plan.request,
                    originalAccount: destructiveAccountRecord
                )
                guard revalidation.succeeded else {
                    return failedApply(
                        completed: completed,
                        failed: step.kind,
                        message: revalidation.message
                    )
                }
            case .createStandardAccount, .repairAccountType, .repairHomeDirectory, .repairShell,
                    .hideAccount, .writeOwnershipMarker, .verifyAccount,
                    .verifyManagedUserPTY, .removeManagedUserIntegration:
                break
            }

            let result = apply(step: step, request: plan.request, ptySessions: ptySessions)
            guard result.succeeded else {
                return failedApply(
                    completed: completed,
                    failed: step.kind,
                    message: result.message
                )
            }
            completed.append(step.kind)
        }
        return AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: completed.map(\.rawValue),
            failedHelperStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper applied the Managed User plan. Credentials redacted."]
        )
    }

    private func managedAccountRecordForDestructiveDeletion(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (account: AlanManagedUserAccountRecord?, message: String) {
        guard let account = accountRecord(for: request.accountName) else {
            return (nil, "Privileged helper refused destructive deletion for a missing Managed User.")
        }
        guard destructiveOwnershipEvidenceExists(for: request) else {
            return (
                nil,
                "Privileged helper refused destructive deletion because Alan ownership could not be revalidated."
            )
        }
        return (account, "Managed User destructive ownership revalidated.")
    }

    private func validateHomeDeletionStillManaged(
        _ request: AlanXPCManagedTerminalAccountRequest,
        originalAccount: AlanManagedUserAccountRecord?
    ) -> (succeeded: Bool, message: String) {
        guard destructiveOwnershipEvidenceExists(for: request) else {
            return (
                false,
                "Privileged helper refused home deletion because Alan ownership could not be revalidated."
            )
        }
        if let currentAccount = accountRecord(for: request.accountName),
           let originalAccount,
           currentAccount.uid != originalAccount.uid
        {
            return (
                false,
                "Privileged helper refused home deletion because the Managed User identity changed."
            )
        }
        return (true, "Managed User home deletion ownership revalidated.")
    }

    private func destructiveOwnershipEvidenceExists(
        for request: AlanXPCManagedTerminalAccountRequest
    ) -> Bool {
        fileManager.fileExists(atPath: ownershipMarkerPath(for: request))
    }

    func removeIntegration(
        request: AlanXPCManagedTerminalAccountRequest
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        let markerPath = ownershipMarkerPath(for: request)
        if fileManager.fileExists(atPath: markerPath) {
            try? fileManager.removeItem(atPath: markerPath)
        }
        return AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: [AlanXPCManagedUserHelperPlanStepKind.removeManagedUserIntegration.rawValue],
            failedHelperStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper removed Managed User integration. Credentials redacted."]
        )
    }

    func accountReadyForPTY(
        accountName: String,
        homeDirectory: String,
        shell: String,
        channelID: String
    ) -> Result<AlanManagedUserAccountRecord, AlanXPCPrivilegedHelperDiagnostic> {
        let request = AlanXPCManagedTerminalAccountRequest(
            accountName: accountName,
            fullName: nil,
            shell: shell,
            homeDirectory: homeDirectory,
            hideFromLoginWindow: true
        )
        guard channelID == identity.channelID else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: accountName,
                    code: .channelMismatch,
                    message: "Privileged helper rejected a channel-mismatched PTY request."
                )
            )
        }
        guard validate(request).isEmpty, let account = accountRecord(for: accountName) else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: accountName,
                    code: .invalidAccountIdentifier,
                    message: "Privileged helper rejected an invalid PTY request."
                )
            )
        }
        let diagnosis = diagnose(request: request, verifyPTY: false)
        guard diagnosis.readinessState == .ready else {
            return .failure(
                diagnosis.diagnostic
                    ?? diagnostic(
                        operation: .startManagedUserPTY,
                        accountName: accountName,
                        code: .accountNotAlanManaged,
                        message: "Managed User is not ready for PTY launch."
                    )
            )
        }
        return .success(account)
    }

    private func apply(
        step: AlanXPCManagedUserHelperPlanStep,
        request: AlanXPCManagedTerminalAccountRequest,
        ptySessions: AlanPrivilegedHelperPTYSessionStore
    ) -> (succeeded: Bool, message: String) {
        switch step.kind {
        case .createStandardAccount:
            return createStandardAccount(request)
        case .repairAccountType:
            return removeAdminMembership(request.accountName)
        case .repairHomeDirectory:
            return repairHomeDirectory(request)
        case .repairShell:
            return runFixed("/usr/bin/dscl", [".", "-create", "/Users/\(request.accountName)", "UserShell", request.shell])
        case .hideAccount:
            return runFixed("/usr/bin/dscl", [".", "-create", "/Users/\(request.accountName)", "IsHidden", "1"])
        case .writeOwnershipMarker:
            return writeOwnershipMarker(request)
        case .verifyAccount:
            let diagnosis = diagnose(request: request, verifyPTY: false)
            return diagnosis.readinessState == .ready || diagnosis.readinessState == .ptySpawnFailed
                ? (true, "Managed User account state verified. Credentials redacted.")
                : (false, diagnosis.diagnostic?.sanitizedMessage ?? "Managed User verification failed.")
        case .verifyManagedUserPTY:
            return verifyManagedUserPTY(request: request)
                ? (true, "Managed User PTY smoke verified. Credentials redacted.")
                : (false, "Managed User PTY smoke verification failed. Credentials redacted.")
        case .removeManagedUserIntegration:
            _ = removeIntegration(request: request)
            return (true, "Managed User integration removed. Credentials redacted.")
        case .deleteAccount:
            return runFixed("/usr/bin/dscl", [".", "-delete", "/Users/\(request.accountName)"])
        case .deleteHomeDirectory:
            guard request.homeDirectory == request.canonicalHomeDirectory else {
                return (false, "Privileged helper refused non-canonical home deletion.")
            }
            do {
                if fileManager.fileExists(atPath: request.homeDirectory) {
                    try fileManager.removeItem(atPath: request.homeDirectory)
                }
                return (true, "Managed User home directory deleted. Credentials redacted.")
            } catch {
                return (false, "Managed User home directory deletion failed. Credentials redacted.")
            }
        }
    }

    private func createStandardAccount(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        if accountRecord(for: request.accountName) != nil {
            return (true, "Managed User already exists. Credentials redacted.")
        }
        let password = UUID().uuidString + UUID().uuidString
        let fullName = request.fullName?.trimmingCharacters(in: .whitespacesAndNewlines)
        let result = runFixed(
            "/usr/sbin/sysadminctl",
            [
                "-addUser",
                request.accountName,
                "-fullName",
                fullName?.isEmpty == false ? fullName! : request.accountName,
                "-home",
                request.homeDirectory,
                "-shell",
                request.shell,
                "-password",
                password,
            ]
        )
        guard result.succeeded else { return result }
        return repairHomeDirectory(request)
    }

    private func removeAdminMembership(_ accountName: String) -> (succeeded: Bool, message: String) {
        runFixed("/usr/sbin/dseditgroup", ["-o", "edit", "-d", accountName, "-t", "user", "admin"])
    }

    private func repairHomeDirectory(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        guard let account = accountRecord(for: request.accountName) else {
            return (false, "Managed User account is missing. Credentials redacted.")
        }
        if account.homeDirectory != request.homeDirectory {
            let result = runFixed(
                "/usr/bin/dscl",
                [
                    ".",
                    "-create",
                    "/Users/\(request.accountName)",
                    "NFSHomeDirectory",
                    request.homeDirectory,
                ]
            )
            guard result.succeeded else {
                return result
            }
        }
        do {
            try fileManager.createDirectory(
                atPath: request.homeDirectory,
                withIntermediateDirectories: true
            )
            _ = chown(request.homeDirectory, account.uid, account.gid)
            return (true, "Managed User home directory repaired. Credentials redacted.")
        } catch {
            return (false, "Managed User home directory repair failed. Credentials redacted.")
        }
    }

    private func writeOwnershipMarker(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        let markerPath = ownershipMarkerPath(for: request)
        let markerURL = URL(fileURLWithPath: markerPath)
        let marker: [String: String] = [
            "managed_by": "alan",
            "channel_id": identity.channelID,
            "account_name": request.accountName,
            "home_directory": request.homeDirectory,
            "shell": request.shell,
        ]
        do {
            try fileManager.createDirectory(
                at: markerURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONSerialization.data(withJSONObject: marker, options: [.sortedKeys])
            try data.write(to: markerURL, options: .atomic)
            return (true, "Managed User ownership marker written. Credentials redacted.")
        } catch {
            return (false, "Managed User ownership marker write failed. Credentials redacted.")
        }
    }

    private func verifyManagedUserPTY(request: AlanXPCManagedTerminalAccountRequest) -> Bool {
        guard let account = accountRecord(for: request.accountName) else { return false }
        var master: Int32 = -1
        var pid: pid_t = 0
        let argvValues = [request.shell, "-lc", "exit 0"]
        let envValues = AlanPrivilegedHelperPTYSupport.environment(
            accountName: request.accountName,
            home: request.homeDirectory,
            shell: request.shell
        )
        let result = request.shell.withCString { executable in
            request.homeDirectory.withCString { workingDirectory in
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
                                24,
                                80,
                                &master,
                                &pid
                            )
                        }
                    }
                }
            }
        }
        defer {
            if master >= 0 {
                close(master)
            }
        }
        guard result == 0 else { return false }
        var status: Int32 = 0
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline {
            let waitResult = waitpid(pid, &status, WNOHANG)
            if waitResult == pid {
                return AlanPrivilegedHelperPTYSupport.waitStatusExited(status)
                    && AlanPrivilegedHelperPTYSupport.waitStatusExitCode(status) == 0
            }
            Thread.sleep(forTimeInterval: 0.05)
        }
        _ = kill(pid, SIGTERM)
        _ = waitpid(pid, &status, 0)
        return false
    }

    private func validate(_ request: AlanXPCManagedTerminalAccountRequest) -> [AlanPrivilegedHelperXPCErrorCode] {
        var errors: [AlanPrivilegedHelperXPCErrorCode] = []
        let pattern = #"^[A-Za-z_][A-Za-z0-9_-]{0,31}$"#
        if request.accountName.range(of: pattern, options: .regularExpression) == nil
            || ["root", "daemon", "nobody"].contains(request.accountName.lowercased())
        {
            errors.append(.invalidAccountIdentifier)
        }
        if request.homeDirectory != request.canonicalHomeDirectory {
            errors.append(.invalidHomePath)
        }
        if request.shell != "/bin/zsh" {
            errors.append(.shellNotAllowed)
        }
        return errors
    }

    private func diagnosis(
        request: AlanXPCManagedTerminalAccountRequest,
        ownershipState: AlanXPCManagedUserOwnershipState,
        readinessState: AlanXPCManagedUserReadinessState,
        account: AlanManagedUserAccountRecord?,
        ptySmokeVerified: Bool,
        diagnostic: AlanXPCPrivilegedHelperDiagnostic?
    ) -> AlanXPCManagedUserDiagnosis {
        AlanXPCManagedUserDiagnosis(
            request: request,
            ownershipState: ownershipState,
            readinessState: readinessState,
            accountExists: account != nil,
            isAdmin: account?.isAdmin == true,
            homeDirectoryExists: fileManager.fileExists(atPath: request.homeDirectory),
            shellMatches: account?.shell == request.shell,
            hiddenFromLoginWindow: account?.hidden == true,
            terminalProfileID: nil,
            ptySmokeVerified: ptySmokeVerified,
            diagnostic: diagnostic
        )
    }

    private func accountRecord(for accountName: String) -> AlanManagedUserAccountRecord? {
        let result = runCommand(
            "/usr/bin/dscl",
            [
                ".",
                "-read",
                "/Users/\(accountName)",
                "UniqueID",
                "PrimaryGroupID",
                "NFSHomeDirectory",
                "UserShell",
                "IsHidden",
            ]
        )
        guard result.succeeded,
              let uidString = propertyValue("UniqueID", in: result.stdout),
              let gidString = propertyValue("PrimaryGroupID", in: result.stdout),
              let uid = uid_t(uidString),
              let gid = gid_t(gidString)
        else {
            return nil
        }
        let home = propertyValue("NFSHomeDirectory", in: result.stdout) ?? "/Users/\(accountName)"
        let shell = propertyValue("UserShell", in: result.stdout) ?? "/bin/zsh"
        let hidden = propertyValue("IsHidden", in: result.stdout) == "1"
        return AlanManagedUserAccountRecord(
            name: accountName,
            uid: uid,
            gid: gid,
            homeDirectory: home,
            shell: shell,
            hidden: hidden,
            isAdmin: isAdmin(accountName)
        )
    }

    private func isAdmin(_ accountName: String) -> Bool {
        let result = runCommand("/usr/sbin/dseditgroup", ["-o", "checkmember", "-m", accountName, "admin"])
        guard result.succeeded else { return false }
        let output = (result.stdout + "\n" + result.stderr).lowercased()
        return output.contains("yes") || (output.contains("is a member") && !output.contains("not a member"))
    }

    private func propertyValue(_ key: String, in output: String) -> String? {
        output
            .split(separator: "\n", omittingEmptySubsequences: false)
            .compactMap { line -> String? in
                let prefixes = ["\(key):", "dsAttrTypeNative:\(key):"]
                guard let prefix = prefixes.first(where: { line.hasPrefix($0) }) else {
                    return nil
                }
                let value = line.dropFirst(prefix.count)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return value.isEmpty ? nil : value
            }
            .first
    }

    private func ownershipMarkerPath(for request: AlanXPCManagedTerminalAccountRequest) -> String {
        "\(identity.dataRootPath)/managed-users/\(request.accountName)/ownership.json"
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

    private func failedApply(
        completed: [AlanXPCManagedUserHelperPlanStepKind],
        failed: AlanXPCManagedUserHelperPlanStepKind?,
        message: String
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: completed.map(\.rawValue),
            failedHelperStep: failed?.rawValue,
            cancelled: false,
            visibleDiagnostics: [AlanPrivilegedHelperSanitizer.sanitizedMessage(message)]
        )
    }

    private func runFixed(
        _ executable: String,
        _ arguments: [String]
    ) -> (succeeded: Bool, message: String) {
        let result = runCommand(executable, arguments)
        return result.succeeded
            ? (true, "Privileged helper operation completed. Credentials redacted.")
            : (false, "Privileged helper operation failed. Credentials redacted.")
    }

    private func runCommand(
        _ executable: String,
        _ arguments: [String]
    ) -> (succeeded: Bool, stdout: String, stderr: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return (false, "", "\(error)")
        }
        let stdout = String(data: outputPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderr = String(data: errorPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return (process.terminationStatus == 0, stdout, stderr)
    }
}

struct AlanManagedUserAccountRecord {
    let name: String
    let uid: uid_t
    let gid: gid_t
    let homeDirectory: String
    let shell: String
    let hidden: Bool
    let isAdmin: Bool
}
