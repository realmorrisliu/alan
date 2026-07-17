#if os(macOS)
import Darwin
import Foundation
#if canImport(GhosttyKit)
import GhosttyKit
#endif

enum AlanGhosttyBootstrapPhase: String, Equatable {
    case pending
    case ready
    case failed
}

struct AlanGhosttyBootstrapDiagnostics: Equatable {
    let phase: AlanGhosttyBootstrapPhase
    let summary: String
    let detail: String?
    let failureReason: String?
    let dependencies: GhosttyIntegrationStatus
    let lastUpdatedAt: Date

    var isReady: Bool {
        phase == .ready
    }

    static func pending(
        dependencies: GhosttyIntegrationStatus = GhosttyIntegrationStatus.discover()
    ) -> AlanGhosttyBootstrapDiagnostics {
        AlanGhosttyBootstrapDiagnostics(
            phase: .pending,
            summary: "Ghostty process bootstrap has not started.",
            detail: nil,
            failureReason: nil,
            dependencies: dependencies,
            lastUpdatedAt: .now
        )
    }
}

@MainActor
protocol AlanGhosttyProcessBootstrap: AnyObject {
    var diagnostics: AlanGhosttyBootstrapDiagnostics { get }
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics
}

@MainActor
final class AlanDefaultGhosttyProcessBootstrap: AlanGhosttyProcessBootstrap {
    static let shared = AlanDefaultGhosttyProcessBootstrap()

    private var cachedDiagnostics = AlanGhosttyBootstrapDiagnostics.pending()

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        cachedDiagnostics
    }

    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        if cachedDiagnostics.phase == .ready || cachedDiagnostics.phase == .failed {
            return cachedDiagnostics
        }

        let dependencies = GhosttyIntegrationStatus.discover()
#if canImport(GhosttyKit)
        scrubInheritedTerminalEnvironment()
        configureGhosttyProcessEnvironment(from: dependencies)

        let result = ghostty_init(UInt(CommandLine.argc), CommandLine.unsafeArgv)
        guard result == GHOSTTY_SUCCESS else {
            cachedDiagnostics = AlanGhosttyBootstrapDiagnostics(
                phase: .failed,
                summary: "ghostty_init failed.",
                detail: "libghostty returned \(result).",
                failureReason: "Ghostty library initialization failed.",
                dependencies: dependencies,
                lastUpdatedAt: .now
            )
            return cachedDiagnostics
        }

        cachedDiagnostics = AlanGhosttyBootstrapDiagnostics(
            phase: .ready,
            summary: "Ghostty process bootstrap initialized.",
            detail: dependencies.summary,
            failureReason: nil,
            dependencies: dependencies,
            lastUpdatedAt: .now
        )
#else
        cachedDiagnostics = AlanGhosttyBootstrapDiagnostics(
            phase: .failed,
            summary: "GhosttyKit is not linked into this build.",
            detail: dependencies.summary,
            failureReason: "GhosttyKit framework is unavailable at compile time.",
            dependencies: dependencies,
            lastUpdatedAt: .now
        )
#endif
        return cachedDiagnostics
    }

#if canImport(GhosttyKit)
    private func configureGhosttyProcessEnvironment(from integration: GhosttyIntegrationStatus) {
        guard let resourcesPath = integration.resourcesPath else { return }
        let shouldOverride = getenv("ALAN_GHOSTTY_RESOURCES_DIR") != nil
            || getenv("GHOSTTY_RESOURCES_DIR") == nil
        guard shouldOverride else { return }
        _ = resourcesPath.withCString { path in
            setenv("GHOSTTY_RESOURCES_DIR", path, 1)
        }
    }

    private func scrubInheritedTerminalEnvironment() {
        let exactKeys = [
            "TERM",
            "TERM_PROGRAM",
            "TERM_PROGRAM_VERSION",
            "COLORTERM",
            "TERMINFO",
            "TERMINFO_DIRS",
            "VTE_VERSION",
            "PWD",
            "SHLVL",
            "_",
            "STARSHIP_SHELL",
            "STARSHIP_SESSION_KEY",
            "RBENV_SHELL",
            "GHOSTTY_SURFACE_ID",
            "GHOSTTY_SHELL_FEATURES",
            "GHOSTTY_SHELL_INTEGRATION_XDG_DIR",
            "GHOSTTY_BIN_DIR",
            "NO_COLOR",
        ]
        exactKeys.forEach { unsetenv($0) }

        for key in ProcessInfo.processInfo.environment.keys {
            if key.hasPrefix("WARP_") || key.hasPrefix("CODEX_") {
                unsetenv(key)
            }
        }
    }
#endif
}

#endif
