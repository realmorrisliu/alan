import Darwin
import Foundation

@_silgen_name("alan_darwin_pty_spawn_as_user")
func alanDarwinPtySpawnAsUser(
    _ executablePath: UnsafePointer<CChar>,
    _ argv: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ envp: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ workingDirectory: UnsafePointer<CChar>,
    _ accountName: UnsafePointer<CChar>,
    _ uid: uid_t,
    _ gid: gid_t,
    _ rows: UInt16,
    _ columns: UInt16,
    _ masterFileDescriptor: UnsafeMutablePointer<Int32>,
    _ processID: UnsafeMutablePointer<pid_t>
) -> Int32

enum AlanPrivilegedHelperPTYSupport {
    static func environment(
        accountName: String,
        home: String,
        shell: String
    ) -> [String] {
        [
            "HOME=\(home)",
            "USER=\(accountName)",
            "LOGNAME=\(accountName)",
            "SHELL=\(shell)",
            "TERM=xterm-256color",
            "PATH=/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        ]
    }

    static func mappedAppErrorCode(
        _ code: AlanPrivilegedHelperXPCErrorCode?
    ) -> String? {
        guard let code else { return nil }
        switch code {
        case .invalidRequest, .invalidAccountIdentifier:
            return "invalid_account_identifier"
        case .unsupportedOperation:
            return "unsupported_operation"
        case .channelMismatch:
            return "channel_mismatch"
        case .clientRequirementFailed:
            return "client_requirement_failed"
        case .connectionFailed, .helperUnavailable, .timeout:
            return "helper_unavailable"
        case .invalidHomePath:
            return "invalid_home_path"
        case .shellNotAllowed:
            return "shell_not_allowed"
        case .accountNotAlanManaged:
            return "account_not_alan_managed"
        case .ptySpawnFailed:
            return "pty_spawn_failed"
        }
    }

    static func withCStringArray<Result>(
        _ strings: [String],
        _ body: (UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Result
    ) -> Result {
        let cStrings = strings.map { strdup($0) }
        let argv = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>.allocate(
            capacity: cStrings.count + 1
        )
        for (index, value) in cStrings.enumerated() {
            argv[index] = value
        }
        argv[cStrings.count] = nil
        defer {
            for value in cStrings {
                free(value)
            }
            argv.deallocate()
        }
        return body(argv)
    }

    static func waitStatusTermSignal(_ status: Int32) -> Int32 {
        status & 0x7f
    }

    static func waitStatusExited(_ status: Int32) -> Bool {
        waitStatusTermSignal(status) == 0
    }

    static func waitStatusSignaled(_ status: Int32) -> Bool {
        let signal = waitStatusTermSignal(status)
        return signal != 0 && signal != 0x7f
    }

    static func waitStatusExitCode(_ status: Int32) -> Int32 {
        (status >> 8) & 0xff
    }
}
