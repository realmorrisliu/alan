#if os(macOS)
import Foundation

enum AlanTerminalPtyRuntimePhase: String, Equatable {
    case pending
    case running
    case inputClosed = "input_closed"
    case exited
    case failed
}

enum AlanTerminalPtySignal: String, Equatable {
    case interrupt = "interrupt"
    case terminate = "terminate"
    case kill = "kill"
}

struct AlanTerminalPtyDimensions: Equatable {
    let columns: Int
    let rows: Int
}

enum AlanTerminalProcessExitStatus: Equatable {
    case exitCode(Int32)
    case signal(Int32)
    case unknown

    var diagnosticsValue: String {
        switch self {
        case .exitCode(let code):
            return "exit:\(code)"
        case .signal(let signal):
            return "signal:\(signal)"
        case .unknown:
            return "unknown"
        }
    }
}

struct AlanTerminalPtyOperationResult: Equatable {
    let accepted: Bool
    let code: String
    let message: String?

    static func accepted(_ code: String) -> AlanTerminalPtyOperationResult {
        AlanTerminalPtyOperationResult(accepted: true, code: code, message: nil)
    }

    static func rejected(
        _ code: String,
        message: String
    ) -> AlanTerminalPtyOperationResult {
        AlanTerminalPtyOperationResult(accepted: false, code: code, message: message)
    }
}

struct AlanTerminalPtyRendererAttachment: Equatable {
    let readFileDescriptor: Int32
    let writeFileDescriptor: Int32
    let closeFileDescriptors: Bool
}

struct AlanTerminalPtyControlSequenceResponse: Equatable {
    let rendererOutput: Data
    let ptyResponse: Data
    let shellActivityTransition: AlanTerminalPtyShellActivityState?

    var didRespond: Bool {
        !ptyResponse.isEmpty
    }
}

enum AlanTerminalPtyShellActivityState: Equatable {
    /// No prompt marker has been observed; consumers must preserve this uncertainty conservatively.
    case unknown
    case shellInput
    case foregroundCommand
}

extension AlanLaunchStrategy {
    var launchesInteractiveShell: Bool {
        switch self {
        case .loginShellOverride,
             .loginShellEnv,
             .loginShellFallback,
             .terminalProfileSudoUser,
             .terminalProfileSudoRoot,
             .terminalProfileManagedUser:
            return true
        case .shellCommandEnv,
             .terminalProfileCustomCommand:
            return false
        }
    }
}

enum AlanTerminalPtyRendererAttachmentResult: Equatable {
    case attached(AlanTerminalPtyRendererAttachment)
    case rejected(AlanTerminalPtyOperationResult)
}

struct AlanTerminalPtyRuntimeSnapshot: Equatable {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    let phase: AlanTerminalPtyRuntimePhase
    let dimensions: AlanTerminalPtyDimensions?
    let acceptedInputBytes: Int
    let inputClosed: Bool
    let lastSignal: AlanTerminalPtySignal?
    let exitStatus: AlanTerminalProcessExitStatus?
    let transcriptLines: [String]
}

@MainActor
protocol AlanTerminalPtyHandle: AnyObject {
    var contentID: String { get }
    var bootRequest: AlanTerminalBootRequest { get }
    var snapshot: AlanTerminalPtyRuntimeSnapshot { get }
    var isInputReady: Bool { get }
    var shellActivityState: AlanTerminalPtyShellActivityState { get }
    var onShellActivityStateChange: ((AlanTerminalPtyShellActivityState) -> Void)? { get set }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult
    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult
    func closeInput() -> AlanTerminalPtyOperationResult
    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult
    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult
    func terminateForCleanup() -> AlanTerminalPtyOperationResult
}

@MainActor
protocol AlanTerminalPtyRuntime: AnyObject {
    var registeredContentIDs: Set<String> { get }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle
    func existingHandle(forTerminalContentID contentID: String) -> AlanTerminalPtyHandle?
    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalPtyRuntimeSnapshot?
    func unregisterHandle(forTerminalContentID contentID: String)
}

@MainActor
protocol AlanManagedUserPtyProviding: AnyObject {
    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle
}

#endif
