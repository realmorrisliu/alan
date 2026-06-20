import Darwin
import Foundation

#if os(macOS)
import AppKit
#if canImport(GhosttyKit)
import GhosttyKit
#endif

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

enum TerminalRuntimeDeliveryCode: String, Codable, Equatable {
    case accepted
    case queued
    case rejected
    case missingTarget = "missing_target"
    case unavailableRuntime = "unavailable_runtime"
    case timeout
}

struct TerminalRuntimeDeliveryResult: Codable, Equatable {
    let code: TerminalRuntimeDeliveryCode
    let acceptedBytes: Int
    let runtimePhase: String?
    let errorCode: String?
    let errorMessage: String?

    var applied: Bool {
        code == .accepted
    }

    static func accepted(
        byteCount: Int,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .accepted,
            acceptedBytes: byteCount,
            runtimePhase: runtimePhase,
            errorCode: nil,
            errorMessage: nil
        )
    }

    static func queued(
        byteCount: Int,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .queued,
            acceptedBytes: byteCount,
            runtimePhase: runtimePhase,
            errorCode: nil,
            errorMessage: nil
        )
    }

    static func rejected(
        errorCode: String,
        errorMessage: String,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .rejected,
            acceptedBytes: 0,
            runtimePhase: runtimePhase,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    static func missingTarget(
        errorMessage: String,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .missingTarget,
            acceptedBytes: 0,
            runtimePhase: runtimePhase,
            errorCode: "terminal_runtime_missing",
            errorMessage: errorMessage
        )
    }

    static func unavailable(
        errorMessage: String,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .unavailableRuntime,
            acceptedBytes: 0,
            runtimePhase: runtimePhase,
            errorCode: "terminal_runtime_unavailable",
            errorMessage: errorMessage
        )
    }

    static func timeout(
        errorMessage: String,
        runtimePhase: String? = nil
    ) -> TerminalRuntimeDeliveryResult {
        TerminalRuntimeDeliveryResult(
            code: .timeout,
            acceptedBytes: 0,
            runtimePhase: runtimePhase,
            errorCode: "terminal_runtime_timeout",
            errorMessage: errorMessage
        )
    }
}

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

private extension AlanTerminalPtyDimensions {
    var terminalGridDimensions: TerminalGridDimensions {
        TerminalGridDimensions(columns: columns, rows: rows)
    }
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

    var didRespond: Bool {
        !ptyResponse.isEmpty
    }
}

struct AlanTerminalPtyControlSequenceResponder: Equatable {
    private enum ParserState: Equatable {
        case normal
        case escape
        case csi
        case osc
        case oscEscape
    }

    private static let escapeByte: UInt8 = 0x1B
    private static let bellByte: UInt8 = 0x07
    private static let csiByte: UInt8 = 0x9B
    private static let oscByte: UInt8 = 0x9D
    private static let leftBracketByte: UInt8 = 0x5B
    private static let rightBracketByte: UInt8 = 0x5D
    private static let backslashByte: UInt8 = 0x5C
    private static let zeroByte: UInt8 = 0x30
    private static let maxBufferedControlSequenceBytes = 512
    private static let primaryDeviceAttributesResponse = Array("\u{1B}[?62;22c".utf8)
    private static let cursorPositionReportResponse = Array("\u{1B}[1;1R".utf8)
    private static let backgroundColorResponse = Array("\u{1B}]11;rgb:0a0a/0c0c/1010\u{1B}\\".utf8)

    private var state: ParserState = .normal
    private var pendingControlSequence: [UInt8] = []
    private var suppressedPrimaryDeviceAttributesResponses: Int

    init(suppressedPrimaryDeviceAttributesResponses: Int = 0) {
        self.suppressedPrimaryDeviceAttributesResponses = max(
            0,
            suppressedPrimaryDeviceAttributesResponses
        )
    }

    static var primaryDeviceAttributesResponseData: Data {
        Data(primaryDeviceAttributesResponse)
    }

    mutating func suppressNextPrimaryDeviceAttributesResponse() {
        suppressedPrimaryDeviceAttributesResponses += 1
    }

    mutating func process(_ data: Data) -> AlanTerminalPtyControlSequenceResponse {
        var rendererOutput: [UInt8] = []
        var ptyResponse: [UInt8] = []

        for byte in data {
            switch state {
            case .normal:
                if byte == Self.escapeByte {
                    pendingControlSequence = [byte]
                    state = .escape
                } else if byte == Self.csiByte {
                    pendingControlSequence = [byte]
                    state = .csi
                } else if byte == Self.oscByte {
                    pendingControlSequence = [byte]
                    state = .osc
                } else {
                    rendererOutput.append(byte)
                }

            case .escape:
                if byte == Self.leftBracketByte {
                    pendingControlSequence.append(byte)
                    state = .csi
                } else if byte == Self.rightBracketByte {
                    pendingControlSequence.append(byte)
                    state = .osc
                } else {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    rendererOutput.append(byte)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .csi:
                pendingControlSequence.append(byte)
                if Self.isCSIFinalByte(byte) {
                    if Self.isPrimaryDeviceAttributesQuery(pendingControlSequence) {
                        if suppressedPrimaryDeviceAttributesResponses > 0 {
                            suppressedPrimaryDeviceAttributesResponses -= 1
                        } else {
                            ptyResponse.append(contentsOf: Self.primaryDeviceAttributesResponse)
                        }
                    } else if Self.isCursorPositionReportQuery(pendingControlSequence) {
                        ptyResponse.append(contentsOf: Self.cursorPositionReportResponse)
                    } else {
                        rendererOutput.append(contentsOf: pendingControlSequence)
                    }
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .osc:
                pendingControlSequence.append(byte)
                if byte == Self.bellByte {
                    Self.completeOSCSequence(
                        pendingControlSequence,
                        rendererOutput: &rendererOutput,
                        ptyResponse: &ptyResponse
                    )
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if byte == Self.escapeByte {
                    state = .oscEscape
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .oscEscape:
                pendingControlSequence.append(byte)
                if byte == Self.backslashByte {
                    Self.completeOSCSequence(
                        pendingControlSequence,
                        rendererOutput: &rendererOutput,
                        ptyResponse: &ptyResponse
                    )
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else {
                    state = .osc
                }
            }
        }

        return AlanTerminalPtyControlSequenceResponse(
            rendererOutput: Data(rendererOutput),
            ptyResponse: Data(ptyResponse)
        )
    }

    private static func isCSIFinalByte(_ byte: UInt8) -> Bool {
        (0x40...0x7E).contains(byte)
    }

    private static func isPrimaryDeviceAttributesQuery(_ bytes: [UInt8]) -> Bool {
        guard bytes.last == UInt8(ascii: "c") else { return false }

        let parameterStartIndex: Int
        if bytes.first == escapeByte {
            guard bytes.count >= 3, bytes[1] == leftBracketByte else { return false }
            parameterStartIndex = 2
        } else if bytes.first == csiByte {
            guard bytes.count >= 2 else { return false }
            parameterStartIndex = 1
        } else {
            return false
        }

        let parameters = bytes[parameterStartIndex..<(bytes.count - 1)]
        return parameters.isEmpty || (parameters.count == 1 && parameters.first == zeroByte)
    }

    private static func isCursorPositionReportQuery(_ bytes: [UInt8]) -> Bool {
        guard bytes.last == UInt8(ascii: "n") else { return false }

        let parameterStartIndex: Int
        if bytes.first == escapeByte {
            guard bytes.count >= 4, bytes[1] == leftBracketByte else { return false }
            parameterStartIndex = 2
        } else if bytes.first == csiByte {
            guard bytes.count >= 3 else { return false }
            parameterStartIndex = 1
        } else {
            return false
        }

        let parameters = bytes[parameterStartIndex..<(bytes.count - 1)]
        return parameters.count == 1 && parameters.first == UInt8(ascii: "6")
    }

    private static func completeOSCSequence(
        _ bytes: [UInt8],
        rendererOutput: inout [UInt8],
        ptyResponse: inout [UInt8]
    ) {
        if isBackgroundColorQuery(bytes) {
            ptyResponse.append(contentsOf: backgroundColorResponse)
        } else {
            rendererOutput.append(contentsOf: bytes)
        }
    }

    private static func isBackgroundColorQuery(_ bytes: [UInt8]) -> Bool {
        let payloadRange: Range<Int>
        if bytes.first == escapeByte {
            guard bytes.count >= 6, bytes[1] == rightBracketByte else { return false }
            if bytes.last == bellByte {
                payloadRange = 2..<(bytes.count - 1)
            } else if bytes.count >= 7,
                bytes[bytes.count - 2] == escapeByte,
                bytes.last == backslashByte
            {
                payloadRange = 2..<(bytes.count - 2)
            } else {
                return false
            }
        } else if bytes.first == oscByte {
            guard bytes.count >= 5, bytes.last == bellByte else { return false }
            payloadRange = 1..<(bytes.count - 1)
        } else {
            return false
        }

        let payload = String(decoding: bytes[payloadRange], as: UTF8.self)
        return payload == "11;?"
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
}

@MainActor
protocol AlanManagedUserPtyProviding: AnyObject {
    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle
}

@MainActor
final class AlanDarwinTerminalPtyRuntime: AlanTerminalPtyRuntime {
    private var handlesByContentID: [String: AlanTerminalPtyHandle] = [:]
    private let managedUserPtyProvider: AlanManagedUserPtyProviding

    init(
        managedUserPtyProvider: AlanManagedUserPtyProviding? = nil
    ) {
        self.managedUserPtyProvider = managedUserPtyProvider ?? AlanUnavailableManagedUserPtyProvider()
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        if let existing = handlesByContentID[contentID] {
            return existing
        }
        let handle: AlanTerminalPtyHandle
        if bootRequest.strategy == .terminalProfileManagedUser {
            handle = managedUserPtyProvider.handle(
                forTerminalContentID: contentID,
                bootRequest: bootRequest
            )
        } else {
            handle = AlanDarwinTerminalPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest
            )
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingHandle(forTerminalContentID contentID: String) -> AlanTerminalPtyHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalPtyRuntimeSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }
}

@MainActor
final class AlanUnavailableManagedUserPtyProvider: AlanManagedUserPtyProviding {
    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        AlanUnavailableManagedUserPtyHandle(
            contentID: contentID,
            bootRequest: bootRequest,
            reason: "Managed User helper PTY provider is unavailable."
        )
    }
}

@MainActor
final class AlanHelperManagedUserPtyProvider: AlanManagedUserPtyProviding {
    let helperClient: AlanPrivilegedHelperClienting
    let defaultDimensions: AlanTerminalPtyDimensions
    let shell: String

    init(
        helperClient: AlanPrivilegedHelperClienting,
        defaultDimensions: AlanTerminalPtyDimensions = AlanTerminalPtyDimensions(columns: 80, rows: 24),
        shell: String = "/bin/zsh"
    ) {
        self.helperClient = helperClient
        self.defaultDimensions = defaultDimensions
        self.shell = shell
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        guard let accountName = bootRequest.managedUserAccountName?.trimmingCharacters(
            in: .whitespacesAndNewlines
        ), !accountName.isEmpty else {
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: "Managed User helper PTY request is missing an account."
            )
        }

        let status = helperClient.status()
        guard status.isHealthy else {
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: status.sanitizedMessage ?? "Managed User helper is unavailable."
            )
        }

        let request = AlanManagedUserPTYStartRequest(
            operationID: UUID().uuidString,
            channelID: status.identity.channelID,
            accountName: accountName,
            homeDirectory: bootRequest.workingDirectory,
            shell: shell,
            contentID: contentID,
            columns: defaultDimensions.columns,
            rows: defaultDimensions.rows
        )
        switch helperClient.startManagedUserPTY(request) {
        case .success(let session):
            return AlanHelperManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                helperClient: helperClient,
                session: session,
                initialDimensions: defaultDimensions
            )
        case .failure(let diagnostic):
            return AlanUnavailableManagedUserPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest,
                reason: diagnostic.sanitizedMessage
            )
        }
    }
}

@MainActor
final class AlanUnavailableManagedUserPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    private let reason: String

    init(
        contentID: String,
        bootRequest: AlanTerminalBootRequest,
        reason: String
    ) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        self.reason = reason
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: .failed,
            dimensions: nil,
            acceptedInputBytes: 0,
            inputClosed: true,
            lastSignal: nil,
            exitStatus: .unknown,
            transcriptLines: [reason]
        )
    }

    var isInputReady: Bool {
        false
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        .unavailable(errorMessage: reason, runtimePhase: AlanTerminalPtyRuntimePhase.failed.rawValue)
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        .rejected("managed_user_helper_unavailable", message: reason)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        .rejected(.rejected("managed_user_helper_unavailable", message: reason))
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        .accepted("managed_user_helper_unavailable")
    }
}

@MainActor
final class AlanHelperManagedUserPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    let helperClient: AlanPrivilegedHelperClienting
    let session: AlanManagedUserPTYSession
    private(set) var dimensions: AlanTerminalPtyDimensions
    private(set) var phase: AlanTerminalPtyRuntimePhase = .running
    private(set) var inputClosed = false
    private(set) var lastSignal: AlanTerminalPtySignal?
    private(set) var exitStatus: AlanTerminalProcessExitStatus?
    private var acceptedInputBytes = 0
    private var cleanupRequested = false
    private var transcriptRingBufferLines: [String] = []
    private var rendererProxy: AlanHelperManagedUserPtyRendererProxy?
    private let helperQueue: DispatchQueue

    init(
        contentID: String,
        bootRequest: AlanTerminalBootRequest,
        helperClient: AlanPrivilegedHelperClienting,
        session: AlanManagedUserPTYSession,
        initialDimensions: AlanTerminalPtyDimensions
    ) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        self.helperClient = helperClient
        self.session = session
        self.dimensions = initialDimensions
        self.helperQueue = DispatchQueue(
            label: "dev.alan.terminal.managed-user-pty.\(contentID)",
            qos: .userInteractive
        )
    }

    deinit {
        rendererProxy?.invalidate()
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        applyPendingProxyOutput()
        refreshExitObservation()
        _ = drainAvailableOutput()
        return AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: phase,
            dimensions: dimensions,
            acceptedInputBytes: acceptedInputBytes,
            inputClosed: inputClosed,
            lastSignal: lastSignal,
            exitStatus: exitStatus,
            transcriptLines: transcriptRingBufferLines.isEmpty
                ? [session.sanitizedMessage]
                : transcriptRingBufferLines
        )
    }

    var isInputReady: Bool {
        phase == .running && !inputClosed && exitStatus == nil
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The managed-user terminal process has exited.",
                runtimePhase: phase.rawValue
            )
        }
        guard !inputClosed else {
            return .rejected(
                errorCode: "terminal_pty_input_closed",
                errorMessage: "The managed-user terminal PTY input stream is closed.",
                runtimePhase: phase.rawValue
            )
        }

        let result = helperClient.writeManagedUserPTY(
            AlanManagedUserPTYInputRequest(sessionID: session.sessionID, text: text)
        )
        guard result.accepted else {
            return .rejected(
                errorCode: helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_input_rejected"),
                errorMessage: result.diagnostic.sanitizedMessage,
                runtimePhase: phase.rawValue
            )
        }
        let byteCount = text.lengthOfBytes(using: .utf8)
        acceptedInputBytes += byteCount
        return .accepted(byteCount: byteCount, runtimePhase: phase.rawValue)
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        let nextDimensions = AlanTerminalPtyDimensions(columns: max(0, columns), rows: max(0, rows))
        let result = helperClient.resizeManagedUserPTY(
            AlanManagedUserPTYResizeRequest(
                sessionID: session.sessionID,
                columns: nextDimensions.columns,
                rows: nextDimensions.rows
            )
        )
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_resize_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        dimensions = nextDimensions
        return .accepted("resized")
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        guard !inputClosed else {
            return .accepted("input_closed")
        }
        let result = helperClient.closeManagedUserPTYInput(sessionID: session.sessionID)
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_eof_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        inputClosed = true
        phase = .inputClosed
        return .accepted("input_closed")
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The managed-user terminal process has exited."
            )
        }
        let result = helperClient.signalManagedUserPTY(
            AlanManagedUserPTYSignalRequest(
                sessionID: session.sessionID,
                signal: AlanManagedUserPTYSignal(signal)
            )
        )
        guard result.accepted else {
            return .rejected(
                helperPTYRejectionCode(result, fallback: "managed_user_helper_pty_signal_rejected"),
                message: result.diagnostic.sanitizedMessage
            )
        }
        lastSignal = signal
        refreshExitObservation()
        return .accepted(signal.rawValue)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        refreshExitObservation()
        guard exitStatus == nil else {
            return .rejected(
                .rejected(
                    "terminal_child_exited",
                    message: "The managed-user terminal process has exited."
                )
            )
        }
        guard phase == .running || phase == .inputClosed else {
            return .rejected(
                .rejected(
                    "managed_user_helper_pty_unavailable",
                    message: "Managed User helper PTY session is unavailable."
                )
            )
        }

        var descriptors = [Int32](repeating: -1, count: 2)
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
            return .rejected(
                .rejected(
                    "managed_user_helper_renderer_socketpair_failed",
                    message: String(cString: strerror(errno))
                )
            )
        }

        setNonBlockingFileDescriptor(descriptors[0])
        setNonBlockingFileDescriptor(descriptors[1])
        setNoSigpipeSocketOption(descriptors[0])
        setNoSigpipeSocketOption(descriptors[1])

        rendererProxy?.invalidate()
        let proxy = AlanHelperManagedUserPtyRendererProxy(
            ptyHandle: self,
            helperClient: helperClient,
            sessionID: session.sessionID,
            hostFileDescriptor: descriptors[0],
            ioQueue: helperQueue
        )
        rendererProxy = proxy
        proxy.start()

        return .attached(
            AlanTerminalPtyRendererAttachment(
                readFileDescriptor: descriptors[1],
                writeFileDescriptor: descriptors[1],
                closeFileDescriptors: true
            )
        )
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        guard !cleanupRequested else { return .accepted("terminated") }
        cleanupRequested = true
        if exitStatus != nil {
            return .accepted("already_exited")
        }
        let diagnostic = helperClient.terminatePTY(sessionID: session.sessionID)
        guard diagnostic.code == nil else {
            return .rejected(
                diagnostic.code?.rawValue ?? "managed_user_helper_pty_terminate_rejected",
                message: diagnostic.sanitizedMessage
            )
        }
        refreshExitObservation()
        if exitStatus == nil {
            inputClosed = true
            phase = .exited
            exitStatus = .unknown
        }
        return .accepted("terminated")
    }

    private func refreshExitObservation() {
        guard exitStatus == nil else { return }
        guard let observation = helperClient.observeManagedUserPTYExit(sessionID: session.sessionID) else {
            return
        }
        if let code = observation.exitCode {
            exitStatus = .exitCode(code)
        } else if let signal = observation.terminatingSignal {
            exitStatus = .signal(signal)
        } else if observation.final {
            exitStatus = .unknown
        }
        if exitStatus != nil {
            inputClosed = true
            phase = .exited
        }
    }

    @discardableResult
    fileprivate func drainAvailableOutput(maxBytes: Int = 4096) -> Data {
        guard exitStatus == nil else { return Data() }
        switch helperQueue.sync(execute: {
            helperClient.readManagedUserPTY(
                AlanManagedUserPTYReadRequest(
                    sessionID: session.sessionID,
                    maxBytes: maxBytes
                )
            )
        }) {
        case .success(let chunk):
            applyHelperOutputChunk(chunk)
            return chunk.data
        case .failure(let diagnostic):
            applyHelperOutputFailure(diagnostic)
            return Data()
        }
    }

    @MainActor
    fileprivate func applyPendingProxyOutput() {
        guard let rendererProxy else { return }
        let updates = rendererProxy.drainPendingOutputUpdates()
        updates.chunks.forEach(applyHelperOutputChunk)
        updates.failures.forEach(applyHelperOutputFailure)
    }

    @MainActor
    fileprivate func applyHelperOutputChunk(_ chunk: AlanManagedUserPTYOutputChunk) {
        if chunk.final {
            inputClosed = true
            phase = .exited
            exitStatus = .unknown
        }
        guard !chunk.data.isEmpty else { return }
        let text = String(decoding: chunk.data, as: UTF8.self)
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    @MainActor
    fileprivate func applyHelperOutputFailure(_ diagnostic: AlanPrivilegedHelperDiagnostic) {
        inputClosed = true
        phase = .failed
        exitStatus = .unknown
        transcriptRingBufferLines.append(diagnostic.sanitizedMessage)
    }

    @MainActor
    fileprivate func recordHelperAcceptedInput(byteCount: Int) {
        acceptedInputBytes += byteCount
    }

}

private final class AlanHelperManagedUserPtyRendererProxy {
    private weak var ptyHandle: AlanHelperManagedUserPtyHandle?
    private let helperClient: AlanPrivilegedHelperClienting
    private let sessionID: String
    private let hostFileDescriptor: Int32
    private let ioQueue: DispatchQueue
    private let invalidationLock = NSLock()
    private let pendingOutputLock = NSLock()
    private var rendererInputSource: DispatchSourceRead?
    private var helperOutputTimer: DispatchSourceTimer?
    private var controlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
    private var isInvalidated = false
    private var pendingOutputChunks: [AlanManagedUserPTYOutputChunk] = []
    private var pendingOutputFailures: [AlanPrivilegedHelperDiagnostic] = []

    init(
        ptyHandle: AlanHelperManagedUserPtyHandle,
        helperClient: AlanPrivilegedHelperClienting,
        sessionID: String,
        hostFileDescriptor: Int32,
        ioQueue: DispatchQueue
    ) {
        self.ptyHandle = ptyHandle
        self.helperClient = helperClient
        self.sessionID = sessionID
        self.hostFileDescriptor = hostFileDescriptor
        self.ioQueue = ioQueue
    }

    deinit {
        invalidate()
    }

    func start() {
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
        inputSource.resume()
        rendererInputSource = inputSource

        let timer = DispatchSource.makeTimerSource(queue: ioQueue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(30))
        timer.setEventHandler { [weak self] in
            self?.pollHelperOutput()
        }
        timer.resume()
        helperOutputTimer = timer
    }

    func invalidate() {
        guard markInvalidated() else { return }
        rendererInputSource?.cancel()
        rendererInputSource = nil
        helperOutputTimer?.cancel()
        helperOutputTimer = nil
    }

    private var invalidated: Bool {
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

    fileprivate func drainPendingOutputUpdates() -> (
        chunks: [AlanManagedUserPTYOutputChunk],
        failures: [AlanPrivilegedHelperDiagnostic]
    ) {
        pendingOutputLock.lock()
        defer { pendingOutputLock.unlock() }
        let chunks = pendingOutputChunks
        let failures = pendingOutputFailures
        pendingOutputChunks.removeAll()
        pendingOutputFailures.removeAll()
        return (chunks, failures)
    }

    private func enqueueOutputChunk(_ chunk: AlanManagedUserPTYOutputChunk) {
        pendingOutputLock.lock()
        pendingOutputChunks.append(chunk)
        pendingOutputLock.unlock()
        Task { @MainActor [weak self] in
            self?.ptyHandle?.applyPendingProxyOutput()
        }
    }

    private func enqueueOutputFailure(_ diagnostic: AlanPrivilegedHelperDiagnostic) {
        pendingOutputLock.lock()
        pendingOutputFailures.append(diagnostic)
        pendingOutputLock.unlock()
        Task { @MainActor [weak self] in
            self?.ptyHandle?.applyPendingProxyOutput()
        }
    }

    private func drainRendererInput() {
        guard !invalidated else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = Darwin.read(hostFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                let data = Data(buffer.prefix(count))
                guard writeHelperInput(data) else {
                    invalidate()
                    return
                }
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

    private func pollHelperOutput() {
        guard !invalidated else { return }
        let readResult = helperClient.readManagedUserPTY(
            AlanManagedUserPTYReadRequest(
                sessionID: sessionID,
                maxBytes: 4096
            )
        )
        let output: Data
        switch readResult {
        case .success(let chunk):
            output = chunk.data
            enqueueOutputChunk(chunk)
        case .failure(let diagnostic):
            enqueueOutputFailure(diagnostic)
            invalidate()
            return
        }
        guard !output.isEmpty else { return }

        let response = controlSequenceResponder.process(output)
        if response.didRespond, !writeHelperInput(response.ptyResponse) {
            invalidate()
            return
        }

        guard !response.rendererOutput.isEmpty else { return }
        response.rendererOutput.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress else { return }
            var offset = 0
            while offset < buffer.count {
                let written = Darwin.write(
                    hostFileDescriptor,
                    baseAddress.advanced(by: offset),
                    buffer.count - offset
                )
                if written > 0 {
                    offset += written
                    continue
                }
                if errno == EAGAIN || errno == EWOULDBLOCK {
                    return
                }
                invalidate()
                return
            }
        }
    }

    private func writeHelperInput(_ data: Data) -> Bool {
        guard !data.isEmpty, !invalidated else { return false }
        let text = String(decoding: data, as: UTF8.self)
        let result = helperClient.writeManagedUserPTY(
            AlanManagedUserPTYInputRequest(sessionID: sessionID, text: text)
        )
        guard result.accepted else { return false }
        Task { @MainActor [weak self] in
            self?.ptyHandle?.recordHelperAcceptedInput(byteCount: data.count)
        }
        return true
    }
}

private func helperPTYRejectionCode(
    _ result: AlanManagedUserPTYControlResult,
    fallback: String
) -> String {
    result.diagnostic.code?.rawValue ?? fallback
}

private func setNonBlockingFileDescriptor(_ fileDescriptor: Int32) {
    let flags = fcntl(fileDescriptor, F_GETFL)
    guard flags >= 0 else { return }
    _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
}

private func setNoSigpipeSocketOption(_ fileDescriptor: Int32) {
    var enabled: Int32 = 1
    _ = setsockopt(
        fileDescriptor,
        SOL_SOCKET,
        SO_NOSIGPIPE,
        &enabled,
        socklen_t(MemoryLayout<Int32>.size)
    )
}

private extension AlanManagedUserPTYSignal {
    init(_ signal: AlanTerminalPtySignal) {
        switch signal {
        case .interrupt:
            self = .interrupt
        case .terminate:
            self = .terminate
        case .kill:
            self = .kill
        }
    }
}

@MainActor
final class AlanDarwinTerminalPtyHandle: AlanTerminalPtyHandle {
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
    private var controlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
    private var rendererProxy: AlanDarwinTerminalPtyRendererProxy?

    init(contentID: String, bootRequest: AlanTerminalBootRequest) {
        self.contentID = contentID
        self.bootRequest = bootRequest
        launch()
    }

    deinit {
        rendererProxy?.invalidate()
        if masterFileDescriptor >= 0 {
            close(masterFileDescriptor)
        }
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        refreshExitStatus()
        drainAvailableOutput()
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

        let bytes = Array(text.utf8)
        let written = bytes.withUnsafeBytes { buffer in
            writeRawInput(buffer)
        }
        guard written >= 0 else {
            return .rejected(
                errorCode: "terminal_pty_write_failed",
                errorMessage: String(cString: strerror(errno)),
                runtimePhase: phase.rawValue
            )
        }

        return .accepted(byteCount: written, runtimePhase: phase.rawValue)
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
        let eof = [UInt8(4)]
        let written = eof.withUnsafeBytes { buffer in
            Darwin.write(masterFileDescriptor, buffer.baseAddress, buffer.count)
        }
        guard written >= 0 else {
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

        let target = processGroupID.map { -$0 } ?? processID
        var result = Darwin.kill(target, rawSignal)
        if result != 0, processGroupID != nil {
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

        rendererProxy?.invalidate()
        let rendererControlSequenceResponder = controlSequenceResponder
        controlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
        let proxy = AlanDarwinTerminalPtyRendererProxy(
            ptyHandle: self,
            hostFileDescriptor: descriptors[0],
            ptyFileDescriptor: masterFileDescriptor,
            controlSequenceResponder: rendererControlSequenceResponder
        )
        rendererProxy = proxy
        proxy.start()

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
        guard masterFileDescriptor >= 0 else { return [] }
        var collected = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)

        while true {
            let count = Darwin.read(masterFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                collected.append(buffer, count: count)
                continue
            }
            if count == 0 {
                refreshExitStatus()
                break
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                break
            }
            break
        }

        guard !collected.isEmpty else { return [] }
        let response = controlSequenceResponder.process(collected)
        if response.didRespond {
            _ = writePtyProtocolResponse(response.ptyResponse)
        }

        guard !response.rendererOutput.isEmpty else { return [] }
        rendererProxy?.forwardPtyOutput(response.rendererOutput)
        let text = String(decoding: response.rendererOutput, as: UTF8.self)
        let lines = transcriptLines(from: text)
        transcriptRingBufferLines.append(contentsOf: lines)
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
        return lines
    }

    @discardableResult
    fileprivate func writePtyProtocolResponse(_ data: Data) -> Int {
        data.withUnsafeBytes { buffer -> Int in
            guard masterFileDescriptor >= 0, let baseAddress = buffer.baseAddress else {
                return -1
            }
            return Darwin.write(masterFileDescriptor, baseAddress, buffer.count)
        }
    }

    @discardableResult
    fileprivate func writeRawInput(_ buffer: UnsafeRawBufferPointer) -> Int {
        guard masterFileDescriptor >= 0, let baseAddress = buffer.baseAddress else {
            return -1
        }
        let written = Darwin.write(masterFileDescriptor, baseAddress, buffer.count)
        if written > 0 {
            acceptedInputBytes += written
        }
        return written
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
        return exitStatus
    }

    private func launch() {
        var master: Int32 = -1
        var spawnedPid: pid_t = 0
        let arguments = [bootRequest.executablePath] + bootRequest.arguments
        let environment = ProcessInfo.processInfo.environment.merging(bootRequest.environment) {
            _, newValue in newValue
        }

        let spawnResult = bootRequest.executablePath.withCString { executablePath in
            bootRequest.workingDirectory.withCString { workingDirectory in
                withCStringArray(arguments) { argv in
                    withCStringArray(environment.map { "\($0.key)=\($0.value)" }.sorted()) { envp in
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
        masterFileDescriptor = master
        preseedFishPrimaryDeviceAttributesResponseIfNeeded()
        phase = .running
        resizeRequests.append(AlanTerminalPtyDimensions(columns: 80, rows: 24))
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
        controlSequenceResponder.suppressNextPrimaryDeviceAttributesResponse()
    }

    private func setNonBlocking(_ fileDescriptor: Int32) {
        let flags = fcntl(fileDescriptor, F_GETFL)
        guard flags >= 0 else { return }
        _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
    }
}

private final class AlanDarwinTerminalPtyRendererProxy {
    private weak var ptyHandle: AlanDarwinTerminalPtyHandle?
    private let hostFileDescriptor: Int32
    private let ptyFileDescriptor: Int32
    private let ioQueue = DispatchQueue(
        label: "dev.alan.terminal.pty.renderer",
        qos: .userInitiated
    )
    private var rendererInputSource: DispatchSourceRead?
    private var ptyOutputSource: DispatchSourceRead?
    private var controlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
    private var isInvalidated = false

    init(
        ptyHandle: AlanDarwinTerminalPtyHandle,
        hostFileDescriptor: Int32,
        ptyFileDescriptor: Int32,
        controlSequenceResponder: AlanTerminalPtyControlSequenceResponder = AlanTerminalPtyControlSequenceResponder()
    ) {
        self.ptyHandle = ptyHandle
        self.hostFileDescriptor = hostFileDescriptor
        self.ptyFileDescriptor = ptyFileDescriptor
        self.controlSequenceResponder = controlSequenceResponder
    }

    deinit {
        invalidate()
    }

    func start() {
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
        inputSource.resume()
        rendererInputSource = inputSource

        let outputSource = DispatchSource.makeReadSource(
            fileDescriptor: ptyFileDescriptor,
            queue: ioQueue
        )
        outputSource.setEventHandler { [weak self] in
            self?.drainPtyOutput()
        }
        outputSource.resume()
        ptyOutputSource = outputSource
    }

    func invalidate() {
        guard !isInvalidated else { return }
        isInvalidated = true
        rendererInputSource?.cancel()
        rendererInputSource = nil
        ptyOutputSource?.cancel()
        ptyOutputSource = nil
    }

    func forwardPtyOutput(_ data: Data) {
        guard !isInvalidated, !data.isEmpty else { return }
        data.withUnsafeBytes { buffer in
            guard let baseAddress = buffer.baseAddress else { return }
            var offset = 0
            while offset < buffer.count {
                let written = Darwin.write(
                    hostFileDescriptor,
                    baseAddress.advanced(by: offset),
                    buffer.count - offset
                )
                if written > 0 {
                    offset += written
                    continue
                }
                if errno == EAGAIN || errno == EWOULDBLOCK {
                    return
                }
                invalidate()
                return
            }
        }
    }

    private func drainRendererInput() {
        guard !isInvalidated else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = Darwin.read(hostFileDescriptor, &buffer, buffer.count)
            if count > 0 {
                let written = buffer.withUnsafeBytes { rawBuffer in
                    writePtyInput(
                        UnsafeRawBufferPointer(
                            start: rawBuffer.baseAddress,
                            count: count
                        )
                    )
                }
                if written < 0 {
                    invalidate()
                    return
                }
                if written > 0 {
                    Task { @MainActor [weak self] in
                        self?.ptyHandle?.recordRendererInputBytes(written)
                    }
                }
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
        guard !isInvalidated else { return }
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
        let response = controlSequenceResponder.process(collected)
        if response.didRespond {
            let written = response.ptyResponse.withUnsafeBytes { rawBuffer in
                writePtyInput(
                    UnsafeRawBufferPointer(
                        start: rawBuffer.baseAddress,
                        count: rawBuffer.count
                    )
                )
            }
            if written < 0 {
                invalidate()
                return
            }
        }

        guard !response.rendererOutput.isEmpty else { return }
        forwardPtyOutput(response.rendererOutput)
        Task { @MainActor [weak self] in
            self?.ptyHandle?.recordPtyOutput(response.rendererOutput)
        }
    }

    private func writePtyInput(_ buffer: UnsafeRawBufferPointer) -> Int {
        guard let baseAddress = buffer.baseAddress else { return -1 }
        var offset = 0
        while offset < buffer.count {
            let written = Darwin.write(
                ptyFileDescriptor,
                baseAddress.advanced(by: offset),
                buffer.count - offset
            )
            if written > 0 {
                offset += written
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return offset
            }
            return offset > 0 ? offset : -1
        }
        return offset
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

enum TerminalTranscriptCaptureFailureCode: String, Equatable {
    case missingRuntime = "missing_runtime"
    case emptyTranscript = "empty_transcript"
}

struct TerminalTranscriptCaptureFailure: Equatable {
    let contentID: String
    let code: TerminalTranscriptCaptureFailureCode
    let message: String
}

enum TerminalTranscriptCaptureResult: Equatable {
    case captured(TerminalTranscriptSnapshot)
    case failed(TerminalTranscriptCaptureFailure)
}

enum TerminalRuntimeGracefulShutdownReason: String, Codable, Equatable {
    case paneClose = "pane_close"
    case tabClose = "tab_close"
    case windowClose = "window_close"
    case appQuit = "app_quit"
}

enum TerminalRuntimeGracefulShutdownRequestCode: String, Codable, Equatable {
    case requested
    case alreadyExited = "already_exited"
    case missingRuntime = "missing_runtime"
    case unavailable
    case rejected
}

struct TerminalRuntimeGracefulShutdownRequestResult: Codable, Equatable {
    let contentID: String
    let reason: TerminalRuntimeGracefulShutdownReason
    let code: TerminalRuntimeGracefulShutdownRequestCode
    let delivery: TerminalRuntimeDeliveryResult?
    let message: String?

    var wasRequested: Bool {
        code == .requested
    }
}

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

enum AlanTerminalSurfaceLifecyclePhase: String, Equatable {
    case pending
    case bootstrapping
    case attachable
    case attached
    case closing
    case closed
    case failed
}

enum AlanTerminalSurfaceTeardownStatus: String, Equatable {
    case notStarted = "not_started"
    case closing
    case completed
    case interrupted
}

struct AlanTerminalSurfaceSnapshot: Equatable {
    let contentID: String
    let paneID: String
    let lifecyclePhase: AlanTerminalSurfaceLifecyclePhase
    let renderer: TerminalRendererSnapshot
    let metadata: TerminalPaneMetadataSnapshot
    let lastDelivery: TerminalRuntimeDeliveryResult?
    let teardownStatus: AlanTerminalSurfaceTeardownStatus
    let attachedViewCount: Int
    let lastUpdatedAt: Date

    var runtimePhase: String {
        renderer.phase.rawValue
    }

    static func pending(contentID: String, paneID: String) -> AlanTerminalSurfaceSnapshot {
        AlanTerminalSurfaceSnapshot(
            contentID: contentID,
            paneID: paneID,
            lifecyclePhase: .pending,
            renderer: .placeholder,
            metadata: .placeholder,
            lastDelivery: nil,
            teardownStatus: .notStarted,
            attachedViewCount: 0,
            lastUpdatedAt: .now
        )
    }
}

@MainActor
protocol AlanTerminalSurfaceHandle: AnyObject {
    var contentID: String { get }
    var paneID: String { get }
    var snapshot: AlanTerminalSurfaceSnapshot { get }
    var isSurfaceReady: Bool { get }
    var renderPriority: TerminalRuntimeRenderPriority { get }
    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? { get }
    var fallbackTranscriptLines: [String] { get }
    var terminalDimensions: AlanTerminalPtyDimensions? { get }
    var seededTranscriptSnapshot: TerminalTranscriptSnapshot? { get }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?)
    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    )
    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    )
    func detach()
    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot)
    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String?
    func seedRestoredTranscriptSnapshot(_ snapshot: TerminalTranscriptSnapshot)
    func clearRestoredTranscriptSnapshot()
    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult
    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult
    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult
    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus
}

#if canImport(GhosttyKit)
@MainActor
protocol AlanGhosttyEventSurfaceHandle:
    AlanTerminalSurfaceHandle,
    AlanTerminalSearchEngine,
    AlanTerminalScrollbackEngine,
    AlanTerminalSelectionEngine,
    AlanTerminalCommandBufferEngine
{
    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e
    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool
    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool
    func sendProgrammaticText(_ text: String)
    func sendPreedit(_ text: String?)
    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e)
    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool
    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t)
    func sendMousePressure(stage: UInt32, pressure: Double)
    func readSelectionText() -> String?
    func hasSelection() -> Bool
    func readText(in range: AlanTerminalBufferRange) -> String?
    func imeRect(in view: NSView) -> NSRect?
}
#endif

@MainActor
final class AlanGhosttySurfaceHandle: AlanTerminalSurfaceHandle {
    let contentID: String
    private(set) var paneID: String
    private(set) var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground

    private let bootstrap: AlanGhosttyProcessBootstrap
    private let ptyRuntime: AlanTerminalPtyRuntime
    private var ptyHandle: AlanTerminalPtyHandle?
    private var bootProfile: AlanShellBootProfile?
    private var currentSnapshot: AlanTerminalSurfaceSnapshot
    private var latestHostRuntime: TerminalHostRuntimeSnapshot?
    private var lastAppliedPtyGrid: AlanTerminalPtyDimensions?
    private var transcriptRingBufferLines: [String] = []
    private(set) var seededTranscriptSnapshot: TerminalTranscriptSnapshot?
#if canImport(GhosttyKit)
    private let liveHost = AlanGhosttyLiveHost()
#endif

    init(
        contentID: String,
        paneID: String,
        bootstrap: AlanGhosttyProcessBootstrap,
        ptyRuntime: AlanTerminalPtyRuntime,
        renderCoordinator: TerminalRenderCoordinator? = nil
    ) {
        self.contentID = contentID
        self.paneID = paneID
        self.bootstrap = bootstrap
        self.ptyRuntime = ptyRuntime
        self.currentSnapshot = .pending(contentID: contentID, paneID: paneID)
#if canImport(GhosttyKit)
        self.liveHost.renderCoordinator = renderCoordinator
#endif
    }

    var snapshot: AlanTerminalSurfaceSnapshot {
        currentSnapshot
    }

    var isSurfaceReady: Bool {
#if canImport(GhosttyKit)
        return currentSnapshot.teardownStatus != .completed && liveHost.isSurfaceReady
#else
        return false
#endif
    }

    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? {
        latestHostRuntime
    }

    private var ptyRuntimePhase: String? {
        ptyHandle?.snapshot.phase.rawValue ?? currentSnapshot.runtimePhase
    }

    var fallbackTranscriptLines: [String] {
        if let ptyLines = ptyHandle?.snapshot.transcriptLines, !ptyLines.isEmpty {
            return ptyLines
        }
        return transcriptRingBufferLines
    }

    var terminalDimensions: AlanTerminalPtyDimensions? {
        ptyHandle?.snapshot.dimensions
    }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?) {
        self.paneID = paneID
        self.bootProfile = bootProfile
        if let bootProfile {
            ptyHandle = ptyRuntime.handle(
                forTerminalContentID: contentID,
                bootRequest: bootProfile.bootRequest
            )
            lastAppliedPtyGrid = nil
        }
        guard currentSnapshot.teardownStatus != .completed else { return }
        updateSnapshot(
            lifecyclePhase: bootProfile == nil ? .pending : .attachable,
            metadata: metadataWithBootProfile(bootProfile)
        )
    }

    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    ) {
        let previousPriority = renderPriority
        renderPriority = priority
#if canImport(GhosttyKit)
        liveHost.updateRenderPriority(priority)
        if forceCatchUp || (previousPriority == .hiddenBackground && priority.isVisible) {
            liveHost.requestRenderCatchUp()
        }
#endif
    }

    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        guard currentSnapshot.teardownStatus != .completed else {
            onDiagnosticsChange(currentSnapshot.renderer)
            onMetadataChange(currentSnapshot.metadata)
            return
        }

        updateSnapshot(lifecyclePhase: .bootstrapping, attachedViewCount: 1)
        let diagnostics = bootstrap.ensureReady()
        guard diagnostics.isReady else {
            let renderer = TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .failed,
                summary: diagnostics.summary,
                detail: diagnostics.detail,
                failureReason: diagnostics.failureReason,
                recentEvents: currentSnapshot.renderer.recentEvents
            )
            updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
            onDiagnosticsChange(renderer)
            onMetadataChange(currentSnapshot.metadata)
            return
        }

#if canImport(GhosttyKit)
        guard let canvasView = canvasView as? AlanGhosttyCanvasView else {
            let renderer = TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Ghostty surface cannot attach to this canvas.",
                detail: nil,
                failureReason: "Expected AlanGhosttyCanvasView.",
                recentEvents: currentSnapshot.renderer.recentEvents
            )
            updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
            onDiagnosticsChange(renderer)
            return
        }

        liveHost.onDiagnosticsChange = { [weak self] snapshot in
            guard let self else { return }
            updateSnapshot(
                lifecyclePhase: snapshot.phase == .failed ? .failed : .attached,
                renderer: snapshot
            )
            onDiagnosticsChange(snapshot)
        }
        liveHost.onMetadataChange = { [weak self] metadata in
            guard let self else { return }
            updateSnapshot(metadata: metadata)
            onMetadataChange(metadata)
        }
        liveHost.onCloseRequest = { requiresConfirmation in
            onCloseRequest(requiresConfirmation)
        }
        updateRenderPriority(renderPriority, forceCatchUp: false)
        liveHost.attach(
            to: canvasView,
            bootProfile: bootProfile,
            ptyAttachmentProvider: { [weak self] in
                guard let ptyHandle = self?.ptyHandle else {
                    return .rejected(
                        .rejected(
                            "terminal_pty_runtime_missing",
                            message: "Alan-owned PTY runtime is required before renderer attachment."
                        )
                    )
                }
                return ptyHandle.makeRendererAttachment()
            },
            focused: focused,
            renderPriority: renderPriority
        )
        resizePtyToRendererGridIfAvailable()
        updateSnapshot(
            lifecyclePhase: liveHost.isSurfaceReady ? .attached : .attachable,
            metadata: liveHost.latestMetadata
        )
#else
        let renderer = TerminalRendererSnapshot(
            kind: .scaffold,
            phase: .failed,
            summary: "GhosttyKit is not linked into this build.",
            detail: nil,
            failureReason: "GhosttyKit framework is unavailable at compile time.",
            recentEvents: currentSnapshot.renderer.recentEvents
        )
        updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
        onDiagnosticsChange(renderer)
#endif
    }

    func detach() {
        updateSnapshot(attachedViewCount: 0)
    }

    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) {
        latestHostRuntime = snapshot
        resizePtyToRendererGridIfAvailable()
    }

    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String? {
#if canImport(GhosttyKit)
        liveHost.readText(in: range)
#else
        nil
#endif
    }

    func seedRestoredTranscriptSnapshot(_ snapshot: TerminalTranscriptSnapshot) {
        let bounded = snapshot.boundedForManifest()
        seededTranscriptSnapshot = bounded
        transcriptRingBufferLines = bounded.transcriptLines
    }

    func clearRestoredTranscriptSnapshot() {
        seededTranscriptSnapshot = nil
        transcriptRingBufferLines = []
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return recordDelivery(.accepted(byteCount: 0, runtimePhase: ptyRuntimePhase))
        }
        guard currentSnapshot.teardownStatus != .completed else {
            return recordDelivery(
                .rejected(
                    errorCode: "terminal_runtime_closed",
                    errorMessage: "The requested pane runtime has already closed.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }
        guard let ptyHandle else {
            return recordDelivery(
                .unavailable(
                    errorMessage: "The requested pane does not have an Alan-owned PTY runtime.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }

        return recordDelivery(ptyHandle.writeInput(text))
    }

    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        guard currentSnapshot.teardownStatus != .completed else {
            return recordDelivery(
                .rejected(
                    errorCode: "terminal_runtime_closed",
                    errorMessage: "The requested pane runtime has already closed.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }
        guard let ptyHandle else {
            return recordDelivery(
                .unavailable(
                    errorMessage: "The requested pane does not have an Alan-owned PTY runtime.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }

        if key == .endOfTransmission {
            let eof = ptyHandle.closeInput()
            let delivery: TerminalRuntimeDeliveryResult = eof.accepted
                ? .accepted(byteCount: 0, runtimePhase: ptyHandle.snapshot.phase.rawValue)
                : .rejected(
                    errorCode: eof.code,
                    errorMessage: eof.message ?? "Alan-owned PTY EOF delivery failed.",
                    runtimePhase: ptyHandle.snapshot.phase.rawValue
                )
            return recordDelivery(delivery)
        }

        let text: String
        switch key {
        case .interrupt:
            text = "\u{3}"
        case .endOfTransmission:
            text = ""
        case .returnKey:
            text = "\r"
        }
        return recordDelivery(ptyHandle.writeInput(text))
    }

    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        let ptySnapshot = ptyHandle?.snapshot
        if currentSnapshot.metadata.processExited || ptySnapshot?.exitStatus != nil {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .alreadyExited,
                delivery: nil,
                message: "The terminal process has already exited."
            )
        }
        if currentSnapshot.teardownStatus == .completed {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .unavailable,
                delivery: nil,
                message: "The terminal runtime has already closed."
            )
        }
        guard let ptyHandle else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No Alan-owned PTY runtime is registered for this content."
            )
        }

        let signal = ptyHandle.sendSignal(.interrupt)
        let delivery: TerminalRuntimeDeliveryResult = signal.accepted
            ? .accepted(byteCount: 0, runtimePhase: ptyHandle.snapshot.phase.rawValue)
            : .rejected(
                errorCode: signal.code,
                errorMessage: signal.message ?? "Alan-owned PTY signal delivery failed.",
                runtimePhase: ptyHandle.snapshot.phase.rawValue
            )
        let code: TerminalRuntimeGracefulShutdownRequestCode = signal.accepted ? .requested : .rejected
        _ = recordDelivery(delivery)
        return TerminalRuntimeGracefulShutdownRequestResult(
            contentID: contentID,
            reason: reason,
            code: code,
            delivery: delivery,
            message: delivery.errorMessage
        )
    }

    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus {
        guard currentSnapshot.teardownStatus != .completed else { return .completed }
        updateSnapshot(lifecyclePhase: .closing, teardownStatus: .closing)
#if canImport(GhosttyKit)
        liveHost.teardown()
#endif
        _ = ptyHandle?.terminateForCleanup()
        ptyHandle = nil
        updateSnapshot(
            lifecyclePhase: .closed,
            metadata: .placeholder,
            teardownStatus: .completed,
            attachedViewCount: 0
        )
        return .completed
    }

    private func metadataWithBootProfile(
        _ bootProfile: AlanShellBootProfile?
    ) -> TerminalPaneMetadataSnapshot {
        guard let bootProfile else { return currentSnapshot.metadata }
        return TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: bootProfile.workingDirectory,
            summary: currentSnapshot.metadata.summary,
            attention: currentSnapshot.metadata.attention,
            processExited: currentSnapshot.metadata.processExited,
            lastCommandExitCode: currentSnapshot.metadata.lastCommandExitCode,
            lastUpdatedAt: currentSnapshot.metadata.lastUpdatedAt,
            activeTaskState: currentSnapshot.metadata.activeTaskState
        )
    }

    private func recordDelivery(
        _ delivery: TerminalRuntimeDeliveryResult
    ) -> TerminalRuntimeDeliveryResult {
        updateSnapshot(lastDelivery: delivery)
        return delivery
    }

    private func resizePtyToRendererGridIfAvailable() {
        guard let rendererGrid = rendererTerminalGridForPtyResize else { return }
        guard rendererGrid.isUsable else { return }
        let dimensions = AlanTerminalPtyDimensions(
            columns: rendererGrid.columns,
            rows: rendererGrid.rows
        )
        guard dimensions != lastAppliedPtyGrid else { return }
        guard let ptyHandle else { return }
        let result = ptyHandle.resize(columns: dimensions.columns, rows: dimensions.rows)
        if result.accepted {
            lastAppliedPtyGrid = dimensions
        }
    }

    private var rendererTerminalGridForPtyResize: TerminalGridDimensions? {
#if canImport(GhosttyKit)
        if let rendererGrid = liveHost.terminalGridDimensions?.terminalGridDimensions {
            return rendererGrid
        }
#endif
        return nil
    }

    private func updateSnapshot(
        lifecyclePhase: AlanTerminalSurfaceLifecyclePhase? = nil,
        renderer: TerminalRendererSnapshot? = nil,
        metadata: TerminalPaneMetadataSnapshot? = nil,
        lastDelivery: TerminalRuntimeDeliveryResult? = nil,
        teardownStatus: AlanTerminalSurfaceTeardownStatus? = nil,
        attachedViewCount: Int? = nil
    ) {
        currentSnapshot = AlanTerminalSurfaceSnapshot(
            contentID: contentID,
            paneID: paneID,
            lifecyclePhase: lifecyclePhase ?? currentSnapshot.lifecyclePhase,
            renderer: renderer ?? currentSnapshot.renderer,
            metadata: metadata ?? currentSnapshot.metadata,
            lastDelivery: lastDelivery ?? currentSnapshot.lastDelivery,
            teardownStatus: teardownStatus ?? currentSnapshot.teardownStatus,
            attachedViewCount: attachedViewCount ?? currentSnapshot.attachedViewCount,
            lastUpdatedAt: .now
        )
    }
}

#if canImport(GhosttyKit)
extension AlanGhosttySurfaceHandle: AlanGhosttyEventSurfaceHandle {
    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e {
        liveHost.keyTranslationMods(for: mods)
    }

    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        liveHost.sendKey(keyEvent)
    }

    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool {
        liveHost.keyIsBinding(keyEvent, flags: flags)
    }

    func sendProgrammaticText(_ text: String) {
        liveHost.sendProgrammaticText(text)
    }

    func sendPreedit(_ text: String?) {
        liveHost.sendPreedit(text)
    }

    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e) {
        liveHost.sendMousePosition(x: x, y: y, mods: mods)
    }

    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool {
        liveHost.sendMouseButton(state: state, button: button, mods: mods)
    }

    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t) {
        liveHost.sendMouseScroll(x: x, y: y, mods: mods)
    }

    func sendMousePressure(stage: UInt32, pressure: Double) {
        liveHost.sendMousePressure(stage: stage, pressure: pressure)
    }

    func readSelectionText() -> String? {
        liveHost.readSelectionText()
    }

    func hasSelection() -> Bool {
        liveHost.hasSelection()
    }

    func readText(in range: AlanTerminalBufferRange) -> String? {
        liveHost.readText(in: range)
    }

    func imeRect(in view: NSView) -> NSRect? {
        liveHost.imeRect(in: view)
    }

    func setSearchUpdateHandler(_ handler: ((AlanTerminalSearchEngineUpdate) -> Void)?) {
        liveHost.onSearchUpdate = handler
    }

    func setScrollbackUpdateHandler(_ handler: ((AlanTerminalScrollbackMetrics) -> Void)?) {
        liveHost.onScrollbackUpdate = handler
    }

    func startSearch() -> Bool {
        liveHost.performBindingAction("start_search")
    }

    func updateSearchQuery(_ query: String) -> Bool {
        liveHost.performBindingAction("search:\(query)")
    }

    func navigateSearch(_ direction: AlanTerminalSearchNavigationDirection) -> Bool {
        switch direction {
        case .next:
            return liveHost.performBindingAction("navigate_search:next")
        case .previous:
            return liveHost.performBindingAction("navigate_search:previous")
        }
    }

    func endSearch() -> Bool {
        liveHost.performBindingAction("end_search")
    }

    func scrollTo(row: Int) -> Bool {
        liveHost.performBindingAction("scroll_to_row:\(row)")
    }
}
#endif

@MainActor
protocol AlanTerminalRuntimeService: AnyObject {
    var diagnostics: AlanGhosttyBootstrapDiagnostics { get }
    var registeredContentIDs: Set<String> { get }
    var registeredPaneIDs: Set<String> { get }
    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? { get }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics
    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle
    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle?
    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot?
    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult
    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult
    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    )
    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String)
    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult
    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult
    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus
    func finalizeTerminalContents(excluding activeContentIDs: Set<String>)
}

extension AlanTerminalRuntimeService {
    func surfaceHandle(
        for paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        surfaceHandle(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            mountedAtPaneID: paneID,
            bootProfile: bootProfile
        )
    }

    func existingSurfaceHandle(for paneID: String) -> AlanTerminalSurfaceHandle? {
        existingSurfaceHandle(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID)
        )
    }

    func snapshot(for paneID: String) -> AlanTerminalSurfaceSnapshot? {
        snapshot(forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID))
    }

    func sendText(to paneID: String, text: String) -> TerminalRuntimeDeliveryResult {
        sendText(
            toTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            text: text
        )
    }

    func sendKey(to paneID: String, key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        sendKey(
            toTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            key: key
        )
    }

    func requestGracefulShutdown(
        for paneID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        requestGracefulShutdown(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            reason: reason
        )
    }

    @discardableResult
    func finalizePane(_ paneID: String) -> AlanTerminalSurfaceTeardownStatus {
        finalizeTerminalContent(ShellContentInstance.terminalContentID(forPaneID: paneID))
    }

    func finalizePanes(excluding activePaneIDs: Set<String>) {
        finalizeTerminalContents(
            excluding: Set(activePaneIDs.map { ShellContentInstance.terminalContentID(forPaneID: $0) })
        )
    }
}

@MainActor
private func buildTerminalTranscriptCapture(
    for handle: AlanTerminalSurfaceHandle,
    now: Date = .now
) -> TerminalTranscriptCaptureResult {
    let hostSnapshot = handle.latestHostRuntimeSnapshot
    let surfaceSnapshot = handle.snapshot
    let metrics = hostSnapshot?.surfaceState.scrollback.metrics
    let range = transcriptCaptureRange(metrics: metrics)
    let liveLines = handle.captureTranscriptText(in: range)
        .map(transcriptLines(from:)) ?? []
    let lines = liveLines.isEmpty ? handle.fallbackTranscriptLines : liveLines
    guard !lines.isEmpty else {
        return .failed(
            TerminalTranscriptCaptureFailure(
                contentID: handle.contentID,
                code: .emptyTranscript,
                message: "The terminal runtime did not expose restorable transcript text."
            )
        )
    }

    let metadata = hostSnapshot?.paneMetadata ?? surfaceSnapshot.metadata
    let dimensions = transcriptDimensions(
        ptyDimensions: handle.terminalDimensions,
        metrics: metrics
    )
    let alternateScreen = hostSnapshot?.surfaceState.terminalMode == .alternateScreen
    let snapshot = TerminalTranscriptSnapshot(
        contentID: handle.contentID,
        cwd: metadata.workingDirectory,
        title: metadata.title,
        dimensions: dimensions,
        viewport: TerminalTranscriptViewport(
            firstVisibleRow: metrics?.firstVisibleRow,
            cursorRow: nil
        ),
        transcriptLines: lines,
        processSummary: TerminalTranscriptProcessSummary(
            processState: metadata.processExited
                ? "exited"
                : metadata.activeTaskState?.rawValue,
            program: metadata.activity?.source.label,
            argvPreview: nil,
            lastCommandExitCode: metadata.lastCommandExitCode
        ),
        capturedAt: now,
        alternateScreen: alternateScreen
    )
    return .captured(snapshot.boundedForManifest())
}

private func transcriptCaptureRange(metrics: AlanTerminalScrollbackMetrics?) -> AlanTerminalBufferRange {
    guard let metrics, metrics.totalRows > 0 else {
        return AlanTerminalBufferRange(
            lowerBound: 0,
            upperBound: TerminalTranscriptSnapshot.defaultMaxRows
        )
    }
    let upperBound = max(metrics.totalRows, metrics.firstVisibleRow + metrics.visibleRows)
    return AlanTerminalBufferRange(
        lowerBound: max(0, upperBound - TerminalTranscriptSnapshot.defaultMaxRows),
        upperBound: upperBound
    )
}

private func transcriptLines(from text: String) -> [String] {
    text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
}

private func transcriptDimensions(
    ptyDimensions: AlanTerminalPtyDimensions?,
    metrics: AlanTerminalScrollbackMetrics?
) -> TerminalTranscriptDimensions? {
    let columns = ptyDimensions?.columns ?? 0
    let rows = ptyDimensions?.rows ?? metrics?.visibleRows ?? 0
    guard columns > 0 || rows > 0 else { return nil }
    return TerminalTranscriptDimensions(columns: max(0, columns), rows: max(0, rows))
}

@MainActor
final class AlanWindowTerminalRuntimeService: AlanTerminalRuntimeService {
    typealias SurfaceFactory = (String, String, AlanGhosttyProcessBootstrap) -> AlanTerminalSurfaceHandle

    private let bootstrap: AlanGhosttyProcessBootstrap
    private let ptyRuntime: AlanTerminalPtyRuntime
    private let makeSurfaceHandle: SurfaceFactory
    private var handlesByContentID: [String: AlanTerminalSurfaceHandle] = [:]
    private var restoredTranscriptSnapshotsByContentID: [String: TerminalTranscriptSnapshot] = [:]
    let renderCoordinator: TerminalRenderCoordinator

    init(
        renderCoordinator: TerminalRenderCoordinator = TerminalRenderCoordinator(),
        ptyRuntime: AlanTerminalPtyRuntime? = nil,
        surfaceFactory: SurfaceFactory? = nil
    ) {
        self.renderCoordinator = renderCoordinator
        self.bootstrap = AlanDefaultGhosttyProcessBootstrap.shared
        let ptyRuntime = ptyRuntime ?? AlanDarwinTerminalPtyRuntime()
        self.ptyRuntime = ptyRuntime
        let coordinator = renderCoordinator
        self.makeSurfaceHandle = surfaceFactory ?? { contentID, paneID, bootstrap in
            AlanGhosttySurfaceHandle(
                contentID: contentID,
                paneID: paneID,
                bootstrap: bootstrap,
                ptyRuntime: ptyRuntime,
                renderCoordinator: coordinator
            )
        }
    }

    init(
        bootstrap: AlanGhosttyProcessBootstrap,
        renderCoordinator: TerminalRenderCoordinator = TerminalRenderCoordinator(),
        ptyRuntime: AlanTerminalPtyRuntime? = nil,
        surfaceFactory: SurfaceFactory? = nil
    ) {
        self.renderCoordinator = renderCoordinator
        self.bootstrap = bootstrap
        let ptyRuntime = ptyRuntime ?? AlanDarwinTerminalPtyRuntime()
        self.ptyRuntime = ptyRuntime
        let coordinator = renderCoordinator
        self.makeSurfaceHandle = surfaceFactory ?? { contentID, paneID, bootstrap in
            AlanGhosttySurfaceHandle(
                contentID: contentID,
                paneID: paneID,
                bootstrap: bootstrap,
                ptyRuntime: ptyRuntime,
                renderCoordinator: coordinator
            )
        }
    }

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        bootstrap.diagnostics
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    var registeredPaneIDs: Set<String> {
        Set(handlesByContentID.values.map(\.paneID))
    }

    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? {
        renderCoordinator.metricsSnapshot()
    }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        bootstrap.ensureReady()
    }

    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        ensureReady()
        if let handle = handlesByContentID[contentID] {
            handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
            if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
                handle.seedRestoredTranscriptSnapshot(restored)
            }
            return handle
        }
        let handle = makeSurfaceHandle(contentID, paneID, bootstrap)
        handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
        if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
            handle.seedRestoredTranscriptSnapshot(restored)
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult {
        guard let handle = handlesByContentID[contentID] else {
            return .failed(
                TerminalTranscriptCaptureFailure(
                    contentID: contentID,
                    code: .missingRuntime,
                    message: "No service-owned terminal runtime is registered for this content."
                )
            )
        }
        return buildTerminalTranscriptCapture(for: handle)
    }

    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        guard let handle = handlesByContentID[contentID] else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No service-owned terminal runtime is registered for this content."
            )
        }
        return handle.requestGracefulShutdown(reason: reason)
    }

    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    ) {
        let bounded = snapshot.boundedForManifest()
        restoredTranscriptSnapshotsByContentID[contentID] = bounded
        handlesByContentID[contentID]?.seedRestoredTranscriptSnapshot(bounded)
    }

    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        handlesByContentID[contentID]?.clearRestoredTranscriptSnapshot()
    }

    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a service-owned runtime."
            )
        }
        return handle.sendControlText(text)
    }

    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a service-owned runtime."
            )
        }
        return handle.sendControlKey(key)
    }

    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        guard let handle = handlesByContentID.removeValue(forKey: contentID) else {
            return .notStarted
        }
        return handle.teardown()
    }

    func finalizeTerminalContents(excluding activeContentIDs: Set<String>) {
        let staleContentIDs = Set(handlesByContentID.keys).subtracting(activeContentIDs)
        staleContentIDs.forEach { finalizeTerminalContent($0) }
    }
}

@MainActor
final class FakeAlanGhosttyProcessBootstrap: AlanGhosttyProcessBootstrap {
    private(set) var ensureCallCount = 0
    var nextDiagnostics: AlanGhosttyBootstrapDiagnostics

    init(
        nextDiagnostics: AlanGhosttyBootstrapDiagnostics = AlanGhosttyBootstrapDiagnostics(
            phase: .ready,
            summary: "Fake Ghostty bootstrap ready.",
            detail: nil,
            failureReason: nil,
            dependencies: GhosttyIntegrationStatus.discover(),
            lastUpdatedAt: .now
        )
    ) {
        self.nextDiagnostics = nextDiagnostics
        self.cachedDiagnostics = .pending(dependencies: nextDiagnostics.dependencies)
    }

    private var cachedDiagnostics: AlanGhosttyBootstrapDiagnostics

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        cachedDiagnostics
    }

    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        if cachedDiagnostics.phase == .ready || cachedDiagnostics.phase == .failed {
            return cachedDiagnostics
        }
        ensureCallCount += 1
        cachedDiagnostics = nextDiagnostics
        return cachedDiagnostics
    }
}

@MainActor
final class FakeAlanTerminalPtyRuntime: AlanTerminalPtyRuntime {
    private var handlesByContentID: [String: FakeAlanTerminalPtyHandle] = [:]

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        if let existing = handlesByContentID[contentID] {
            return existing
        }
        let handle = FakeAlanTerminalPtyHandle(
            contentID: contentID,
            bootRequest: bootRequest
        )
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingHandle(forTerminalContentID contentID: String) -> AlanTerminalPtyHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalPtyRuntimeSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }
}

@MainActor
final class FakeAlanTerminalPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    private(set) var deliveredText: [String] = []
    private(set) var resizeRequests: [AlanTerminalPtyDimensions] = []
    private(set) var signalRequests: [AlanTerminalPtySignal] = []
    private(set) var phase: AlanTerminalPtyRuntimePhase = .running
    private(set) var inputClosed = false
    private(set) var exitStatus: AlanTerminalProcessExitStatus?
    private var transcriptRingBufferLines: [String] = []

    init(contentID: String, bootRequest: AlanTerminalBootRequest) {
        self.contentID = contentID
        self.bootRequest = bootRequest
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: phase,
            dimensions: resizeRequests.last,
            acceptedInputBytes: deliveredText.reduce(0) {
                $0 + $1.lengthOfBytes(using: .utf8)
            },
            inputClosed: inputClosed,
            lastSignal: signalRequests.last,
            exitStatus: exitStatus,
            transcriptLines: transcriptRingBufferLines
        )
    }

    var isInputReady: Bool {
        exitStatus == nil && !inputClosed
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard exitStatus == nil else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: phase.rawValue
            )
        }
        guard !inputClosed else {
            return .rejected(
                errorCode: "terminal_pty_input_closed",
                errorMessage: "The terminal PTY input stream is closed.",
                runtimePhase: phase.rawValue
            )
        }
        deliveredText.append(text)
        return .accepted(
            byteCount: text.lengthOfBytes(using: .utf8),
            runtimePhase: phase.rawValue
        )
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        let dimensions = AlanTerminalPtyDimensions(
            columns: max(0, columns),
            rows: max(0, rows)
        )
        resizeRequests.append(dimensions)
        return .accepted("resized")
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        inputClosed = true
        phase = .inputClosed
        return .accepted("input_closed")
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        signalRequests.append(signal)
        return .accepted(signal.rawValue)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        .rejected(
            .rejected(
                "terminal_renderer_attachment_unsupported",
                message: "The fake PTY runtime does not expose renderer file descriptors."
            )
        )
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else { return .accepted("already_exited") }
        inputClosed = true
        phase = .exited
        exitStatus = .unknown
        signalRequests.append(.terminate)
        return .accepted("terminated")
    }

    func recordTranscriptOutput(_ text: String) {
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    func markExited(_ status: AlanTerminalProcessExitStatus) {
        exitStatus = status
        phase = .exited
    }
}

@MainActor
final class FakeAlanTerminalSurfaceHandle: AlanTerminalSurfaceHandle {
    let contentID: String
    private(set) var paneID: String
    private(set) var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    private(set) var configureCount = 0
    private(set) var attachCount = 0
    private(set) var detachCount = 0
    private(set) var teardownCount = 0
    private(set) var renderCatchUpRequestCount = 0
    private(set) var deliveredText: [String] = []
    private(set) var deliveredKeys: [TerminalRuntimeControlKey] = []
    private(set) var gracefulShutdownRequests: [TerminalRuntimeGracefulShutdownReason] = []
    private(set) var searchActions: [String] = []
    private(set) var scrollActions: [String] = []
    var deliveryResult: TerminalRuntimeDeliveryResult?
    var onGracefulShutdownRequest: ((TerminalRuntimeGracefulShutdownReason) -> Void)?
    var searchActionsShouldSucceed = true
    var scrollActionsShouldSucceed = true
    var terminalDimensionsOverride: AlanTerminalPtyDimensions?
    var commandOutputTextByRange: [AlanTerminalBufferRange: String] = [:]
    private(set) var captureTranscriptTextRanges: [AlanTerminalBufferRange] = []
    var selectedText: String?
    var ready = true
    private(set) var seededTranscriptSnapshot: TerminalTranscriptSnapshot?
    private(set) var transcriptRingBufferLines: [String] = []
    private var latestHostRuntime: TerminalHostRuntimeSnapshot?
    private var diagnosticsChangeHandler: ((TerminalRendererSnapshot) -> Void)?
    private var searchUpdateHandler: ((AlanTerminalSearchEngineUpdate) -> Void)?
    private var scrollbackUpdateHandler: ((AlanTerminalScrollbackMetrics) -> Void)?
    private var closeRequestHandler: ((Bool) -> Void)?
    private var currentSnapshot: AlanTerminalSurfaceSnapshot

    init(contentID: String, paneID: String) {
        self.contentID = contentID
        self.paneID = paneID
        self.currentSnapshot = .pending(contentID: contentID, paneID: paneID)
    }

    convenience init(paneID: String) {
        self.init(contentID: ShellContentInstance.terminalContentID(forPaneID: paneID), paneID: paneID)
    }

    var snapshot: AlanTerminalSurfaceSnapshot {
        currentSnapshot
    }

    var isSurfaceReady: Bool {
        ready && currentSnapshot.teardownStatus != .completed
    }

    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? {
        latestHostRuntime
    }

    var fallbackTranscriptLines: [String] {
        transcriptRingBufferLines
    }

    var terminalDimensions: AlanTerminalPtyDimensions? {
        terminalDimensionsOverride
    }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?) {
        self.paneID = paneID
        configureCount += 1
        updateSnapshot(lifecyclePhase: bootProfile == nil ? .pending : .attachable)
    }

    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    ) {
        renderPriority = priority
        if forceCatchUp {
            renderCatchUpRequestCount += 1
        }
    }

    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        updateRenderPriority(renderPriority, forceCatchUp: false)
        attachCount += 1
        diagnosticsChangeHandler = onDiagnosticsChange
        closeRequestHandler = onCloseRequest
        updateSnapshot(lifecyclePhase: .attached, attachedViewCount: 1)
        onDiagnosticsChange(currentSnapshot.renderer)
        onMetadataChange(currentSnapshot.metadata)
    }

    func detach() {
        detachCount += 1
        diagnosticsChangeHandler = nil
        closeRequestHandler = nil
        updateSnapshot(attachedViewCount: 0)
    }

    func emitDiagnosticsSnapshot(_ snapshot: TerminalRendererSnapshot) {
        updateSnapshot(renderer: snapshot)
        diagnosticsChangeHandler?(snapshot)
    }

    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) {
        latestHostRuntime = snapshot
    }

    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String? {
        captureTranscriptTextRanges.append(range)
        return commandOutputTextByRange[range]
    }

    func seedRestoredTranscriptSnapshot(_ snapshot: TerminalTranscriptSnapshot) {
        let bounded = snapshot.boundedForManifest()
        seededTranscriptSnapshot = bounded
        transcriptRingBufferLines = bounded.transcriptLines
    }

    func clearRestoredTranscriptSnapshot() {
        seededTranscriptSnapshot = nil
        transcriptRingBufferLines = []
    }

    func recordTranscriptOutput(_ text: String) {
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !currentSnapshot.metadata.processExited else {
            let result = TerminalRuntimeDeliveryResult.rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        guard isSurfaceReady else {
            let result = TerminalRuntimeDeliveryResult.unavailable(
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        deliveredText.append(text)
        let result = deliveryResult
            ?? .accepted(
                byteCount: text.lengthOfBytes(using: .utf8),
                runtimePhase: currentSnapshot.runtimePhase
            )
        updateSnapshot(lastDelivery: result)
        return result
    }

    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        guard !currentSnapshot.metadata.processExited else {
            let result = TerminalRuntimeDeliveryResult.rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        guard isSurfaceReady else {
            let result = TerminalRuntimeDeliveryResult.unavailable(
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        deliveredKeys.append(key)
        let result = deliveryResult
            ?? .accepted(byteCount: 0, runtimePhase: currentSnapshot.runtimePhase)
        updateSnapshot(lastDelivery: result)
        return result
    }

    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        if currentSnapshot.metadata.processExited {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .alreadyExited,
                delivery: nil,
                message: "The terminal process has already exited."
            )
        }

        gracefulShutdownRequests.append(reason)
        let delivery = sendControlKey(.interrupt)
        let code: TerminalRuntimeGracefulShutdownRequestCode
        switch delivery.code {
        case .accepted, .queued:
            code = .requested
            onGracefulShutdownRequest?(reason)
        case .missingTarget:
            code = .missingRuntime
        case .unavailableRuntime, .timeout:
            code = .unavailable
        case .rejected:
            code = .rejected
        }
        return TerminalRuntimeGracefulShutdownRequestResult(
            contentID: contentID,
            reason: reason,
            code: code,
            delivery: delivery,
            message: delivery.errorMessage
        )
    }

    func markActiveTaskState(_ activeTaskState: ShellTabActiveTaskState?) {
        let metadata = TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: currentSnapshot.metadata.workingDirectory,
            summary: currentSnapshot.metadata.summary,
            attention: activeTaskState?.protectsFromPruning == true ? .active : .idle,
            processExited: currentSnapshot.metadata.processExited,
            lastCommandExitCode: currentSnapshot.metadata.lastCommandExitCode,
            lastUpdatedAt: .now,
            activeTaskState: activeTaskState
        )
        updateSnapshot(metadata: metadata)
    }

    func markProcessExited(exitCode: Int) {
        let metadata = TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: currentSnapshot.metadata.workingDirectory,
            summary: "process exited",
            attention: .awaitingUser,
            processExited: true,
            lastCommandExitCode: exitCode,
            lastUpdatedAt: .now,
            activeTaskState: .inactive
        )
        updateSnapshot(metadata: metadata)
    }

    func requestClose(requiresConfirmation: Bool) {
        closeRequestHandler?(requiresConfirmation)
    }

    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus {
        guard currentSnapshot.teardownStatus != .completed else { return .completed }
        teardownCount += 1
        updateSnapshot(
            lifecyclePhase: .closed,
            teardownStatus: .completed,
            attachedViewCount: 0
        )
        return .completed
    }

    private func updateSnapshot(
        lifecyclePhase: AlanTerminalSurfaceLifecyclePhase? = nil,
        renderer: TerminalRendererSnapshot? = nil,
        metadata: TerminalPaneMetadataSnapshot? = nil,
        lastDelivery: TerminalRuntimeDeliveryResult? = nil,
        teardownStatus: AlanTerminalSurfaceTeardownStatus? = nil,
        attachedViewCount: Int? = nil
    ) {
        currentSnapshot = AlanTerminalSurfaceSnapshot(
            contentID: contentID,
            paneID: paneID,
            lifecyclePhase: lifecyclePhase ?? currentSnapshot.lifecyclePhase,
            renderer: renderer ?? currentSnapshot.renderer,
            metadata: metadata ?? currentSnapshot.metadata,
            lastDelivery: lastDelivery ?? currentSnapshot.lastDelivery,
            teardownStatus: teardownStatus ?? currentSnapshot.teardownStatus,
            attachedViewCount: attachedViewCount ?? currentSnapshot.attachedViewCount,
            lastUpdatedAt: .now
        )
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalSearchEngine {
    func setSearchUpdateHandler(_ handler: ((AlanTerminalSearchEngineUpdate) -> Void)?) {
        searchUpdateHandler = handler
    }

    func startSearch() -> Bool {
        recordSearchAction("start_search")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.started(query: ""))
        return true
    }

    func updateSearchQuery(_ query: String) -> Bool {
        recordSearchAction("search:\(query)")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.started(query: query))
        return true
    }

    func navigateSearch(_ direction: AlanTerminalSearchNavigationDirection) -> Bool {
        switch direction {
        case .next:
            recordSearchAction("navigate_search:next")
        case .previous:
            recordSearchAction("navigate_search:previous")
        }
        return searchActionsShouldSucceed
    }

    func endSearch() -> Bool {
        recordSearchAction("end_search")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.ended)
        return true
    }

    func emitSearchUpdate(_ update: AlanTerminalSearchEngineUpdate) {
        searchUpdateHandler?(update)
    }

    private func recordSearchAction(_ action: String) {
        searchActions.append(action)
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalScrollbackEngine {
    func setScrollbackUpdateHandler(_ handler: ((AlanTerminalScrollbackMetrics) -> Void)?) {
        scrollbackUpdateHandler = handler
    }

    func scrollTo(row: Int) -> Bool {
        scrollActions.append("scroll_to_row:\(row)")
        return scrollActionsShouldSucceed
    }

    func emitScrollbackUpdate(_ metrics: AlanTerminalScrollbackMetrics) {
        scrollbackUpdateHandler?(metrics)
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalSelectionEngine {
    func readSelectionText() -> String? {
        selectedText
    }

    func hasSelection() -> Bool {
        selectedText?.isEmpty == false
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalCommandBufferEngine {
    func readText(in range: AlanTerminalBufferRange) -> String? {
        commandOutputTextByRange[range]
    }
}

@MainActor
final class FakeAlanTerminalRuntimeService: AlanTerminalRuntimeService {
    let bootstrap: FakeAlanGhosttyProcessBootstrap
    private(set) var handlesByContentID: [String: FakeAlanTerminalSurfaceHandle] = [:]
    private var restoredTranscriptSnapshotsByContentID: [String: TerminalTranscriptSnapshot] = [:]

    init() {
        self.bootstrap = FakeAlanGhosttyProcessBootstrap()
    }

    init(bootstrap: FakeAlanGhosttyProcessBootstrap) {
        self.bootstrap = bootstrap
    }

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        bootstrap.diagnostics
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    var registeredPaneIDs: Set<String> {
        Set(handlesByContentID.values.map(\.paneID))
    }

    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? {
        nil
    }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        bootstrap.ensureReady()
    }

    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        ensureReady()
        if let handle = handlesByContentID[contentID] {
            handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
            if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
                handle.seedRestoredTranscriptSnapshot(restored)
            }
            return handle
        }
        let handle = FakeAlanTerminalSurfaceHandle(contentID: contentID, paneID: paneID)
        handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
        if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
            handle.seedRestoredTranscriptSnapshot(restored)
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult {
        guard let handle = handlesByContentID[contentID] else {
            return .failed(
                TerminalTranscriptCaptureFailure(
                    contentID: contentID,
                    code: .missingRuntime,
                    message: "No fake terminal runtime is registered for this content."
                )
            )
        }
        return buildTerminalTranscriptCapture(for: handle)
    }

    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        guard let handle = handlesByContentID[contentID] else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No fake terminal runtime is registered for this content."
            )
        }
        return handle.requestGracefulShutdown(reason: reason)
    }

    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    ) {
        let bounded = snapshot.boundedForManifest()
        restoredTranscriptSnapshotsByContentID[contentID] = bounded
        handlesByContentID[contentID]?.seedRestoredTranscriptSnapshot(bounded)
    }

    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        handlesByContentID[contentID]?.clearRestoredTranscriptSnapshot()
    }

    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a fake terminal runtime."
            )
        }
        return handle.sendControlText(text)
    }

    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a fake terminal runtime."
            )
        }
        return handle.sendControlKey(key)
    }

    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        guard let handle = handlesByContentID.removeValue(forKey: contentID) else {
            return .notStarted
        }
        return handle.teardown()
    }

    func finalizeTerminalContents(excluding activeContentIDs: Set<String>) {
        let staleContentIDs = Set(handlesByContentID.keys).subtracting(activeContentIDs)
        staleContentIDs.forEach { finalizeTerminalContent($0) }
    }
}
#endif
