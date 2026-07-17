#if os(macOS)
import Foundation

enum AlanTerminalSurfaceUnreadyReason: String, Equatable {
    case missingSurface = "missing_surface"
    case inputNotReady = "input_not_ready"
    case rendererFailed = "renderer_failed"
    case childExited = "child_exited"
    case readonly
}

enum AlanTerminalSurfaceReadiness: Equatable {
    case ready
    case unready(reason: AlanTerminalSurfaceUnreadyReason)
}

struct AlanTerminalOverlayState: Equatable {
    let title: String
    let message: String
    let badge: String
    let action: String?
    let debugDetail: String?
}

struct AlanTerminalSurfaceStateSnapshot: Equatable {
    let readiness: AlanTerminalSurfaceReadiness
    let terminalMode: AlanTerminalMode
    let scrollback: AlanTerminalScrollbackState
    let search: AlanTerminalSearchState?
    let semanticCommands: AlanTerminalSemanticCommandState
    let readonly: Bool
    let secureInput: Bool
    let inputReady: Bool
    let rendererHealth: String
    let childExited: Bool
    let lastUpdatedAt: Date

    init(
        readiness: AlanTerminalSurfaceReadiness,
        terminalMode: AlanTerminalMode,
        scrollback: AlanTerminalScrollbackState,
        search: AlanTerminalSearchState?,
        semanticCommands: AlanTerminalSemanticCommandState,
        readonly: Bool,
        secureInput: Bool,
        inputReady: Bool,
        rendererHealth: String,
        childExited: Bool,
        lastUpdatedAt: Date
    ) {
        self.readiness = readiness
        self.terminalMode = terminalMode
        self.scrollback = scrollback
        self.search = search
        self.semanticCommands = semanticCommands
        self.readonly = readonly
        self.secureInput = secureInput
        self.inputReady = inputReady
        self.rendererHealth = rendererHealth
        self.childExited = childExited
        self.lastUpdatedAt = lastUpdatedAt
    }

    static let placeholder = AlanTerminalSurfaceStateSnapshot(
        readiness: .unready(reason: .missingSurface),
        terminalMode: .normalBuffer,
        scrollback: .empty,
        search: nil,
        semanticCommands: .placeholder,
        readonly: false,
        secureInput: false,
        inputReady: false,
        rendererHealth: "pending",
        childExited: false,
        lastUpdatedAt: .now
    )

    func equalsIgnoringTimestamp(_ other: AlanTerminalSurfaceStateSnapshot) -> Bool {
        readiness == other.readiness
            && terminalMode == other.terminalMode
            && scrollback == other.scrollback
            && search == other.search
            && semanticCommands == other.semanticCommands
            && readonly == other.readonly
            && secureInput == other.secureInput
            && inputReady == other.inputReady
            && rendererHealth == other.rendererHealth
            && childExited == other.childExited
    }
}

#endif
