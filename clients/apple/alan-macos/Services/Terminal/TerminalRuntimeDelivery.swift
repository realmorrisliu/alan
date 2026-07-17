#if os(macOS)
import Foundation

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

#endif
